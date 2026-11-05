//! Outlines from drawings: SVG (polygon, polyline, rect, circle, ellipse, path with lines and
//! Bézier curves flattened; arcs as their chord) and DXF (closed LWPOLYLINE and POLYLINE with
//! bulges, CIRCLE, ARC, and LINE/ARC chains that close into loops). DXF curves stay exact arcs
//! for the curve result; flattened SVG curves mark themselves `approximate`.
//! SVG user units are taken as mm unless the root's width/height say otherwise (`mm`, `in`,
//! `px` at 96 per inch); transforms are not applied.

use board_interface::{Element, ElementKind, NM_PER_MM, Point};

use crate::curves::mm_to_nm;
use crate::{Feature, Geom, Op, P};

/// Closed shapes from an SVG document, as polygon features.
pub fn svg(text: &str) -> Result<Vec<Feature>, String> {
    let scale = svg_scale(text);
    let mut out = Vec::new();
    for (tag, attrs) in tags(text) {
        let a = |k: &str| attr(&attrs, k);
        let f = |k: &str| {
            a(k).and_then(|v| v.trim().trim_end_matches("px").parse::<f64>().ok())
                .unwrap_or(0.0)
        };
        let id = a("id").unwrap_or_else(|| format!("{tag} {}", out.len() + 1));
        let mut loops: Vec<(Vec<P>, bool)> = Vec::new();
        match tag.as_str() {
            "polygon" | "polyline" => {
                let n = numbers(&a("points").unwrap_or_default());
                loops.push((n.chunks_exact(2).map(|c| [c[0], c[1]]).collect(), false));
            }
            "rect" => {
                let (x, y, w, h) = (f("x"), f("y"), f("width"), f("height"));
                loops.push((
                    vec![[x, y], [x + w, y], [x + w, y + h], [x, y + h]],
                    false,
                ));
            }
            "circle" | "ellipse" => {
                let (cx, cy) = (f("cx"), f("cy"));
                let (rx, ry) = if tag == "circle" {
                    (f("r"), f("r"))
                } else {
                    (f("rx"), f("ry"))
                };
                loops.push((
                    (0..72)
                        .map(|i| {
                            let t = 2.0 * std::f64::consts::PI * i as f64 / 72.0;
                            [cx + rx * t.cos(), cy + ry * t.sin()]
                        })
                        .collect(),
                    true,
                ));
            }
            "path" => loops.extend(path(&a("d").unwrap_or_default())?),
            _ => continue,
        }
        for (k, (l, approximate)) in loops.into_iter().filter(|(l, _)| l.len() >= 3).enumerate() {
            let points = l.into_iter().map(|p| [p[0] * scale, p[1] * scale]).collect();
            out.push(Feature {
                name: if k == 0 {
                    id.clone()
                } else {
                    format!("{id} ({})", k + 1)
                },
                op: Op::Add,
                geom: Geom::Polygon {
                    points,
                    approximate,
                },
                hidden: false,
            });
        }
    }
    if out.is_empty() {
        return Err("no closed shapes in the SVG".into());
    }
    Ok(out)
}

/// mm per user unit, from the root element's width and viewBox.
fn svg_scale(text: &str) -> f64 {
    let Some((_, root)) = tags(text).into_iter().find(|(t, _)| t == "svg") else {
        return 1.0;
    };
    let Some(w) = attr(&root, "width") else {
        return 1.0;
    };
    let unit = if w.ends_with("mm") {
        1.0
    } else if w.ends_with("cm") {
        10.0
    } else if w.ends_with("in") {
        25.4
    } else if w.ends_with("px") || w.trim().parse::<f64>().is_ok() {
        25.4 / 96.0
    } else {
        return 1.0;
    };
    let width: f64 = w
        .trim_end_matches(|c: char| c.is_alphabetic())
        .trim()
        .parse()
        .unwrap_or(0.0);
    let vb = attr(&root, "viewBox")
        .map(|v| numbers(&v))
        .unwrap_or_default();
    let user = if vb.len() == 4 && vb[2] > 0.0 {
        vb[2]
    } else {
        width
    };
    if width > 0.0 && user > 0.0 {
        width * unit / user
    } else {
        1.0
    }
}

/// An SVG path's closed subpaths, paired with `true` when curves were flattened into lines.
fn path(d: &str) -> Result<Vec<(Vec<P>, bool)>, String> {
    let toks = path_tokens(d);
    let mut loops = Vec::new();
    let mut cur: Vec<P> = Vec::new();
    let mut cur_approx = false;
    let (mut x, mut y) = (0.0, 0.0);
    let (mut sx, mut sy) = (0.0, 0.0);
    let mut last_ctrl: Option<P> = None;
    let mut i = 0;
    let mut cmd = 'M';
    let num = |i: &mut usize| -> Result<f64, String> {
        match toks.get(*i) {
            Some(PathTok::Num(v)) => {
                *i += 1;
                Ok(*v)
            }
            _ => Err("path: a number was expected".into()),
        }
    };
    while i < toks.len() {
        if let PathTok::Cmd(c) = toks[i] {
            cmd = c;
            i += 1;
            if c == 'Z' || c == 'z' {
                if cur.len() >= 3 {
                    loops.push((std::mem::take(&mut cur), cur_approx));
                } else {
                    cur.clear();
                }
                cur_approx = false;
                x = sx;
                y = sy;
                last_ctrl = None;
                continue;
            }
        }
        let rel = cmd.is_ascii_lowercase();
        let (bx, by) = if rel { (x, y) } else { (0.0, 0.0) };
        match cmd.to_ascii_uppercase() {
            'M' => {
                if cur.len() >= 3 {
                    loops.push((std::mem::take(&mut cur), cur_approx));
                }
                cur.clear();
                cur_approx = false;
                x = bx + num(&mut i)?;
                y = by + num(&mut i)?;
                sx = x;
                sy = y;
                cur.push([x, y]);
                // Further pairs after a move are lines.
                cmd = if rel { 'l' } else { 'L' };
                last_ctrl = None;
            }
            'L' => {
                x = bx + num(&mut i)?;
                y = by + num(&mut i)?;
                cur.push([x, y]);
                last_ctrl = None;
            }
            'H' => {
                x = bx + num(&mut i)?;
                cur.push([x, y]);
                last_ctrl = None;
            }
            'V' => {
                y = by + num(&mut i)?;
                cur.push([x, y]);
                last_ctrl = None;
            }
            'C' | 'S' => {
                cur_approx = true;
                let c1 = if cmd.to_ascii_uppercase() == 'C' {
                    [bx + num(&mut i)?, by + num(&mut i)?]
                } else {
                    last_ctrl
                        .map(|c| [2.0 * x - c[0], 2.0 * y - c[1]])
                        .unwrap_or([x, y])
                };
                let c2 = [bx + num(&mut i)?, by + num(&mut i)?];
                let e = [bx + num(&mut i)?, by + num(&mut i)?];
                for k in 1..=16 {
                    let t = k as f64 / 16.0;
                    let m = 1.0 - t;
                    cur.push([
                        m * m * m * x + 3.0 * m * m * t * c1[0] + 3.0 * m * t * t * c2[0] + t * t * t * e[0],
                        m * m * m * y + 3.0 * m * m * t * c1[1] + 3.0 * m * t * t * c2[1] + t * t * t * e[1],
                    ]);
                }
                last_ctrl = Some(c2);
                x = e[0];
                y = e[1];
            }
            'Q' | 'T' => {
                cur_approx = true;
                let c = if cmd.to_ascii_uppercase() == 'Q' {
                    [bx + num(&mut i)?, by + num(&mut i)?]
                } else {
                    last_ctrl
                        .map(|c| [2.0 * x - c[0], 2.0 * y - c[1]])
                        .unwrap_or([x, y])
                };
                let e = [bx + num(&mut i)?, by + num(&mut i)?];
                for k in 1..=12 {
                    let t = k as f64 / 12.0;
                    let m = 1.0 - t;
                    cur.push([
                        m * m * x + 2.0 * m * t * c[0] + t * t * e[0],
                        m * m * y + 2.0 * m * t * c[1] + t * t * e[1],
                    ]);
                }
                last_ctrl = Some(c);
                x = e[0];
                y = e[1];
            }
            'A' => {
                cur_approx = true;
                // rx ry rotation large-arc sweep x y: taken as its chord.
                for _ in 0..5 {
                    num(&mut i)?;
                }
                x = bx + num(&mut i)?;
                y = by + num(&mut i)?;
                cur.push([x, y]);
                last_ctrl = None;
            }
            c => return Err(format!("path: command `{c}` is not supported")),
        }
    }
    // An unclosed subpath that ends where it began counts as closed.
    if cur.len() >= 4 && (cur[0][0] - x).abs() < 1e-6 && (cur[0][1] - y).abs() < 1e-6 {
        loops.push((cur, cur_approx));
    }
    Ok(loops)
}

enum PathTok {
    Cmd(char),
    Num(f64),
}

fn path_tokens(d: &str) -> Vec<PathTok> {
    let mut out = Vec::new();
    let b: Vec<char> = d.chars().collect();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c.is_ascii_alphabetic() && c != 'e' && c != 'E' {
            out.push(PathTok::Cmd(c));
            i += 1;
        } else if c == '-' || c == '+' || c == '.' || c.is_ascii_digit() {
            let start = i;
            let mut seen_dot = c == '.';
            i += 1;
            while i < b.len() {
                let d = b[i];
                if d.is_ascii_digit() {
                    i += 1;
                } else if d == '.' && !seen_dot {
                    seen_dot = true;
                    i += 1;
                } else if (d == 'e' || d == 'E') && i + 1 < b.len() {
                    i += 1;
                    if b[i] == '-' || b[i] == '+' {
                        i += 1;
                    }
                } else {
                    break;
                }
            }
            if let Ok(v) = b[start..i].iter().collect::<String>().parse() {
                out.push(PathTok::Num(v));
            }
        } else {
            i += 1;
        }
    }
    out
}

fn numbers(s: &str) -> Vec<f64> {
    path_tokens(s)
        .into_iter()
        .filter_map(|t| if let PathTok::Num(v) = t { Some(v) } else { None })
        .collect()
}

/// Start tags, with their attribute text: enough of XML for drawings.
fn tags(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find('<') {
        rest = &rest[i + 1..];
        if rest.starts_with('/') || rest.starts_with('!') || rest.starts_with('?') {
            continue;
        }
        let end = rest.find('>').unwrap_or(rest.len());
        let body = &rest[..end];
        let name: String = body
            .chars()
            .take_while(|c| !c.is_whitespace() && *c != '/')
            .collect();
        let name = name.rsplit(':').next().unwrap_or("").to_string();
        out.push((name.clone(), body[name.len().min(body.len())..].to_string()));
        rest = &rest[end.min(rest.len())..];
    }
    out
}

fn attr(attrs: &str, key: &str) -> Option<String> {
    let mut rest = attrs;
    while let Some(i) = rest.find(key) {
        let before = rest[..i].chars().last();
        let after = &rest[i + key.len()..];
        let after_t = after.trim_start();
        if before.is_none_or(|c| c.is_whitespace()) && after_t.starts_with('=') {
            let v = after_t[1..].trim_start();
            let q = v.chars().next()?;
            if q == '"' || q == '\'' {
                let v = &v[1..];
                return v.find(q).map(|j| v[..j].to_string());
            }
        }
        rest = after;
    }
    None
}

/// An unchained DXF segment in DXF's Y-up millimetre frame.
#[derive(Debug, Clone, Copy)]
enum DxfSeg {
    Line {
        a: P,
        b: P,
    },
    Arc {
        /// Arc centre in DXF Y-up mm.
        c: P,
        r: f64,
        a: P,
        b: P,
        /// `true` when sweeping counter-clockwise from `a` to `b` in DXF Y-up (`angle_increasing = false`
        /// after flipping to KiCad Y-down).
        ccw_yup: bool,
    },
}

impl DxfSeg {
    fn start(self) -> P {
        match self {
            DxfSeg::Line { a, .. } | DxfSeg::Arc { a, .. } => a,
        }
    }

    fn end(self) -> P {
        match self {
            DxfSeg::Line { b, .. } | DxfSeg::Arc { b, .. } => b,
        }
    }

    fn reversed(self) -> Self {
        match self {
            DxfSeg::Line { a, b } => DxfSeg::Line { a: b, b: a },
            DxfSeg::Arc {
                c,
                r,
                a,
                b,
                ccw_yup,
            } => DxfSeg::Arc {
                c,
                r,
                a: b,
                b: a,
                ccw_yup: !ccw_yup,
            },
        }
    }
}

/// Closed shapes from a DXF (ASCII) drawing, in its units (taken as mm), keeping exact arcs for
/// the curve result.
pub fn dxf(text: &str) -> Result<Vec<Feature>, String> {
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    let pairs: Vec<(i32, &str)> = lines
        .chunks_exact(2)
        .filter_map(|c| c[0].parse().ok().map(|k| (k, c[1])))
        .collect();
    let mut out = Vec::new();
    let mut segments: Vec<DxfSeg> = Vec::new();
    let mut i = 0;
    while i < pairs.len() {
        if pairs[i].0 != 0 {
            i += 1;
            continue;
        }
        let kind = pairs[i].1;
        let mut j = i + 1;
        while j < pairs.len() && pairs[j].0 != 0 {
            j += 1;
        }
        let ent = &pairs[i + 1..j];
        let get = |tags: &[(i32, &str)], code: i32| {
            tags.iter()
                .find(|(k, _)| *k == code)
                .and_then(|(_, v)| v.parse::<f64>().ok())
        };
        match kind {
            "LWPOLYLINE" => {
                let mut closed = get(ent, 70).map(|f| (f as i64) & 1 == 1).unwrap_or(false);
                // Vertices: 10/20 pairs, each optionally followed by a 42 bulge.
                let mut verts: Vec<(P, f64)> = Vec::new();
                for (k, v) in ent {
                    match k {
                        10 => verts.push(([v.parse().unwrap_or(0.0), 0.0], 0.0)),
                        20 => {
                            if let Some(l) = verts.last_mut() {
                                l.0[1] = v.parse().unwrap_or(0.0);
                            }
                        }
                        42 => {
                            if let Some(l) = verts.last_mut() {
                                l.1 = v.parse().unwrap_or(0.0);
                            }
                        }
                        _ => {}
                    }
                }
                emit_or_queue_polyline(&mut out, &mut segments, &mut verts, &mut closed);
            }
            "POLYLINE" => {
                let flags = get(ent, 70).map(|f| f as i64).unwrap_or(0);
                let is_mesh = (flags & (16 | 64)) != 0;
                let mut closed = (flags & 1) == 1;
                let mut verts: Vec<(P, f64)> = Vec::new();
                while j < pairs.len() && pairs[j].0 == 0 {
                    let sub_kind = pairs[j].1;
                    let mut next_j = j + 1;
                    while next_j < pairs.len() && pairs[next_j].0 != 0 {
                        next_j += 1;
                    }
                    let v_ent = &pairs[j + 1..next_j];
                    j = next_j;
                    if sub_kind == "SEQEND" {
                        break;
                    }
                    if sub_kind == "VERTEX" {
                        let v_flags = get(v_ent, 70).map(|f| f as i64).unwrap_or(0);
                        // Skip spline-frame control points (16) and polyface mesh records (128).
                        if (v_flags & (16 | 128)) == 0 {
                            if let (Some(vx), Some(vy)) = (get(v_ent, 10), get(v_ent, 20)) {
                                let bulge = get(v_ent, 42).unwrap_or(0.0);
                                verts.push(([vx, vy], bulge));
                            }
                        }
                    } else if matches!(sub_kind, "ENDSEC" | "EOF") {
                        break;
                    }
                }
                if !is_mesh {
                    emit_or_queue_polyline(&mut out, &mut segments, &mut verts, &mut closed);
                }
            }
            "CIRCLE" => {
                if let (Some(x), Some(y), Some(r)) = (get(ent, 10), get(ent, 20), get(ent, 40)) {
                    if r > 0.0 {
                        out.push(Feature {
                            name: format!("circle {}", out.len() + 1),
                            op: Op::Add,
                            geom: Geom::Circle {
                                x,
                                y: -y,
                                d: 2.0 * r,
                            },
                            hidden: false,
                        });
                    }
                }
            }
            "ARC" => {
                if let (Some(cx), Some(cy), Some(r), Some(a0_deg), Some(a1_deg)) = (
                    get(ent, 10),
                    get(ent, 20),
                    get(ent, 40),
                    get(ent, 50),
                    get(ent, 51),
                ) {
                    if r > 0.0 {
                        let a0 = a0_deg.to_radians();
                        let a1 = a1_deg.to_radians();
                        let start = [cx + r * a0.cos(), cy + r * a0.sin()];
                        let end = [cx + r * a1.cos(), cy + r * a1.sin()];
                        let sweep_deg = (a1_deg - a0_deg).rem_euclid(360.0);
                        if (start[0] - end[0]).hypot(start[1] - end[1]) < 1e-7
                            && (sweep_deg < 1e-4 || (360.0 - sweep_deg) < 1e-4)
                        {
                            out.push(Feature {
                                name: format!("circle {}", out.len() + 1),
                                op: Op::Add,
                                geom: Geom::Circle {
                                    x: cx,
                                    y: -cy,
                                    d: 2.0 * r,
                                },
                                hidden: false,
                            });
                        } else {
                            segments.push(DxfSeg::Arc {
                                c: [cx, cy],
                                r,
                                a: start,
                                b: end,
                                ccw_yup: true,
                            });
                        }
                    }
                }
            }
            "LINE" => {
                if let (Some(x1), Some(y1), Some(x2), Some(y2)) =
                    (get(ent, 10), get(ent, 20), get(ent, 11), get(ent, 21))
                {
                    if (x2 - x1).hypot(y2 - y1) > 1e-9 {
                        segments.push(DxfSeg::Line {
                            a: [x1, y1],
                            b: [x2, y2],
                        });
                    }
                }
            }
            _ => {}
        }
        i = j;
    }
    for loop_segs in chain_dxf_segments(segments) {
        let name = format!("lines {}", out.len() + 1);
        if let Some(geom) = dxf_segs_to_geom(&loop_segs) {
            out.push(Feature {
                name,
                op: Op::Add,
                geom,
                hidden: false,
            });
        }
    }
    if out.is_empty() {
        return Err(
            "no closed shapes in the DXF (closed LWPOLYLINE/POLYLINE, CIRCLE, or LINE/ARC loops)"
                .into(),
        );
    }
    Ok(out)
}

fn near(a: P, b: P) -> bool {
    (a[0] - b[0]).abs() < 1e-3 && (a[1] - b[1]).abs() < 1e-3
}

fn emit_or_queue_polyline(
    out: &mut Vec<Feature>,
    segments: &mut Vec<DxfSeg>,
    verts: &mut Vec<(P, f64)>,
    closed: &mut bool,
) {
    // Drop repeated final vertex if an open polyline closed onto its start vertex explicitly.
    if !*closed && verts.len() >= 3 && near(verts[0].0, verts.last().unwrap().0) {
        verts.pop();
        *closed = true;
    } else if *closed && verts.len() >= 3 && near(verts[0].0, verts.last().unwrap().0) && verts.last().unwrap().1.abs() < 1e-9 {
        verts.pop();
    }

    if *closed && verts.len() >= 2 {
        let segs = poly_verts_to_segs(verts, true);
        if let Some(geom) = dxf_segs_to_geom(&segs) {
            out.push(Feature {
                name: format!("polyline {}", out.len() + 1),
                op: Op::Add,
                geom,
                hidden: false,
            });
        }
    } else if !*closed && verts.len() >= 2 {
        segments.extend(poly_verts_to_segs(verts, false));
    }
}

fn poly_verts_to_segs(verts: &[(P, f64)], closed: bool) -> Vec<DxfSeg> {
    let count = if closed { verts.len() } else { verts.len() - 1 };
    let mut out = Vec::with_capacity(count);
    for k in 0..count {
        let (a, bulge) = verts[k];
        let b = verts[(k + 1) % verts.len()].0;
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let chord = dx.hypot(dy);
        if chord < 1e-9 {
            continue;
        }
        if bulge.abs() < 1e-9 {
            out.push(DxfSeg::Line { a, b });
        } else {
            let k_cot = (1.0 - bulge * bulge) / (4.0 * bulge);
            let cx = 0.5 * (a[0] + b[0]) - dy * k_cot;
            let cy = 0.5 * (a[1] + b[1]) + dx * k_cot;
            let r = (a[0] - cx).hypot(a[1] - cy);
            out.push(DxfSeg::Arc {
                c: [cx, cy],
                r,
                a,
                b,
                ccw_yup: bulge > 0.0,
            });
        }
    }
    out
}

/// Convert a closed chain of DXF segments into `Geom::Polygon` when all straight (preserving
/// existing behaviour), or `Geom::Path` with exact `Line`/`Arc` elements when arcs are present.
fn dxf_segs_to_geom(segs: &[DxfSeg]) -> Option<Geom> {
    if segs.is_empty() {
        return None;
    }
    let has_arc = segs.iter().any(|s| matches!(s, DxfSeg::Arc { .. }));
    if !has_arc {
        if segs.len() < 3 {
            return None;
        }
        let pts: Vec<P> = segs.iter().map(|s| s.start()).collect();
        return Some(Geom::Polygon {
            points: flip(pts),
            approximate: false,
        });
    }
    if segs.len() < 2 {
        return None;
    }

    // Two opposite semicircles on the same centre & radius form an exact `Geom::Circle`.
    if segs.len() == 2 {
        if let (
            DxfSeg::Arc {
                c: c1,
                r: r1,
                ccw_yup: dir1,
                ..
            },
            DxfSeg::Arc {
                c: c2,
                r: r2,
                ccw_yup: dir2,
                ..
            },
        ) = (segs[0], segs[1])
        {
            if dir1 == dir2
                && (c1[0] - c2[0]).hypot(c1[1] - c2[1]) < 1e-4
                && (r1 - r2).abs() < 1e-4
            {
                return Some(Geom::Circle {
                    x: c1[0],
                    y: -c1[1],
                    d: 2.0 * r1,
                });
            }
        }
    }

    let n = segs.len();
    let junctions: Vec<Point> = (0..n)
        .map(|i| weld_dxf_junction(segs[(i + n - 1) % n], segs[i]))
        .collect();

    let mut elements = Vec::with_capacity(n);
    for i in 0..n {
        let start = junctions[i];
        let end = junctions[(i + 1) % n];
        if start == end {
            continue;
        }
        match segs[i] {
            DxfSeg::Line { .. } => {
                elements.push(Element::new(None, ElementKind::Line { start, end }));
            }
            DxfSeg::Arc { c, ccw_yup, .. } => {
                let center = Point::new(mm_to_nm(c[0]), mm_to_nm(-c[1]));
                // Flipping Y (`y_board = -y_dxf`) reverses angular sweep: CCW in Y-up becomes
                // `angle_increasing = false` in Y-down.
                elements.push(Element::new(
                    None,
                    ElementKind::Arc {
                        center,
                        start,
                        end,
                        angle_increasing: !ccw_yup,
                    },
                ));
            }
        }
    }
    if elements.len() < 2 {
        None
    } else {
        Some(Geom::Path {
            elements,
            approximate: false,
        })
    }
}

fn weld_dxf_junction(prev: DxfSeg, curr: DxfSeg) -> Point {
    let hint_dxf = curr.start();
    let hint_nm = [
        hint_dxf[0] * (NM_PER_MM as f64),
        -hint_dxf[1] * (NM_PER_MM as f64),
    ];
    let proj = |c_dxf: P, r_mm: f64| -> Point {
        let c_nm = Point::new(mm_to_nm(c_dxf[0]), mm_to_nm(-c_dxf[1]));
        let r_nm = r_mm * (NM_PER_MM as f64);
        let dx = hint_nm[0] - (c_nm.x as f64);
        let dy = hint_nm[1] - (c_nm.y as f64);
        let d = dx.hypot(dy);
        if d < 1e-6 {
            Point::new(c_nm.x + (r_nm.round() as i64), c_nm.y)
        } else {
            Point::new(
                ((c_nm.x as f64) + r_nm * dx / d).round() as i64,
                ((c_nm.y as f64) + r_nm * dy / d).round() as i64,
            )
        }
    };

    match (prev, curr) {
        (DxfSeg::Line { .. }, DxfSeg::Line { .. }) => {
            Point::new(mm_to_nm(hint_dxf[0]), mm_to_nm(-hint_dxf[1]))
        }
        (DxfSeg::Line { .. }, DxfSeg::Arc { c, r, .. })
        | (DxfSeg::Arc { c, r, .. }, DxfSeg::Line { .. }) => proj(c, r),
        (
            DxfSeg::Arc {
                c: c1_dxf, r: r1, ..
            },
            DxfSeg::Arc {
                c: c2_dxf, r: r2, ..
            },
        ) => {
            let c1 = [
                mm_to_nm(c1_dxf[0]) as f64,
                mm_to_nm(-c1_dxf[1]) as f64,
            ];
            let c2 = [
                mm_to_nm(c2_dxf[0]) as f64,
                mm_to_nm(-c2_dxf[1]) as f64,
            ];
            let r1_nm = r1 * (NM_PER_MM as f64);
            let r2_nm = r2 * (NM_PER_MM as f64);
            let dx = c2[0] - c1[0];
            let dy = c2[1] - c1[1];
            let d = dx.hypot(dy);
            if d > 1.0 {
                let ux = dx / d;
                let uy = dy / d;
                if d >= (r1_nm - r2_nm).abs() && d <= r1_nm + r2_nm {
                    let a = (r1_nm * r1_nm - r2_nm * r2_nm + d * d) / (2.0 * d);
                    let h = (r1_nm * r1_nm - a * a).max(0.0).sqrt();
                    let mx = c1[0] + a * ux;
                    let my = c1[1] + a * uy;
                    let p_pos = [mx - h * uy, my + h * ux];
                    let p_neg = [mx + h * uy, my - h * ux];
                    let pick = if (p_pos[0] - hint_nm[0]).hypot(p_pos[1] - hint_nm[1])
                        <= (p_neg[0] - hint_nm[0]).hypot(p_neg[1] - hint_nm[1])
                    {
                        p_pos
                    } else {
                        p_neg
                    };
                    if (pick[0] - hint_nm[0]).hypot(pick[1] - hint_nm[1]) <= 5_000.0 {
                        return Point::new(pick[0].round() as i64, pick[1].round() as i64);
                    }
                } else if (d - (r1_nm + r2_nm)).abs() <= 2_000.0 {
                    return Point::new(
                        (c1[0] + r1_nm * ux).round() as i64,
                        (c1[1] + r1_nm * uy).round() as i64,
                    );
                } else if (d - (r1_nm - r2_nm).abs()).abs() <= 2_000.0 {
                    let sign = if r1_nm >= r2_nm { 1.0 } else { -1.0 };
                    return Point::new(
                        (c1[0] + sign * r1_nm * ux).round() as i64,
                        (c1[1] + sign * r1_nm * uy).round() as i64,
                    );
                }
            }
            proj(c1_dxf, r1)
        }
    }
}

/// DXF's y is up; the board's is down.
fn flip(pts: Vec<P>) -> Vec<P> {
    pts.into_iter().map(|p| [p[0], -p[1]]).collect()
}

/// Segments joined end to end into closed loops (ends within 1 um); open chains are dropped.
fn chain_dxf_segments(mut segs: Vec<DxfSeg>) -> Vec<Vec<DxfSeg>> {
    let mut loops = Vec::new();
    while let Some(first) = segs.pop() {
        let mut chain = vec![first];
        loop {
            let start = chain[0].start();
            let end = chain.last().unwrap().end();
            let has_arc = chain.iter().any(|s| matches!(s, DxfSeg::Arc { .. }));
            let min_segs = if has_arc { 2 } else { 3 };
            if chain.len() >= min_segs && near(end, start) {
                loops.push(chain);
                break;
            }
            let Some(k) = segs
                .iter()
                .position(|s| near(s.start(), end) || near(s.end(), end))
            else {
                break;
            };
            let s = segs.swap_remove(k);
            chain.push(if near(s.start(), end) {
                s
            } else {
                s.reversed()
            });
        }
    }
    loops
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curves::evaluate_curves;
    use crate::{ShapeDoc, evaluate};

    fn area(features: Vec<Feature>) -> f64 {
        evaluate(&ShapeDoc {
            features,
            ..Default::default()
        })
        .area_mm2
    }

    #[test]
    fn svg_shapes_in_mm() {
        let text = r#"<svg xmlns="http://www.w3.org/2000/svg" width="40mm" height="30mm" viewBox="0 0 40 30">
            <rect id="board" x="0" y="0" width="40" height="30"/>
            <path d="M 50 0 h 10 v 10 h -10 z m 20 0 l 10 0 l 0 10 l -10 0 Z"/>
            <circle cx="100" cy="100" r="5"/></svg>"#;
        let f = svg(text).unwrap();
        assert_eq!(
            f.len(),
            4,
            "{:?}",
            f.iter().map(|x| &x.name).collect::<Vec<_>>()
        );
        assert_eq!(f[0].name, "board");
        assert!((area(vec![f[0].clone()]) - 1200.0).abs() < 1e-6);
        assert!((area(vec![f[1].clone(), f[2].clone()]) - 200.0).abs() < 1e-6);
        // Straight SVG rect is exact; flattened SVG circle is marked approximate.
        assert!(!evaluate_curves(&ShapeDoc { features: vec![f[0].clone()], ..Default::default() }).approximate);
        assert!(evaluate_curves(&ShapeDoc { features: vec![f[3].clone()], ..Default::default() }).approximate);
    }

    #[test]
    fn svg_pixels_are_scaled() {
        let f =
            svg(r#"<svg width="96px" height="96px"><polygon points="0,0 96,0 96,96 0,96"/></svg>"#)
                .unwrap();
        assert!((area(f) - 25.4 * 25.4).abs() < 1e-3);
    }

    #[test]
    fn svg_curves_are_flattened() {
        // A circle of radius 10 from four cubic quarter arcs.
        let k = 5.5228;
        let d = format!(
            "M 10 0 C 10 {k} {k} 10 0 10 C -{k} 10 -10 {k} -10 0 C -10 -{k} -{k} -10 0 -10 C {k} -10 10 -{k} 10 0 Z"
        );
        let f = svg(&format!(r#"<svg><path d="{d}"/></svg>"#)).unwrap();
        assert!((area(f.clone()) - std::f64::consts::PI * 100.0).abs() < 0.5);
        assert!(evaluate_curves(&ShapeDoc { features: f, ..Default::default() }).approximate);
    }

    #[test]
    fn dxf_polylines_circles_and_lines() {
        let mut t = String::from("0\nSECTION\n2\nENTITIES\n");
        // A 20 x 10 closed polyline whose right side bulges out into a half circle of radius 5.
        t += "0\nLWPOLYLINE\n90\n4\n70\n1\n10\n0\n20\n0\n10\n20\n20\n0\n42\n1\n10\n20\n20\n10\n10\n0\n20\n10\n";
        t += "0\nCIRCLE\n10\n50\n20\n0\n40\n3\n";
        for (a, b) in [
            ([60, 0], [70, 0]),
            ([70, 0], [70, 10]),
            ([70, 10], [60, 10]),
            ([60, 10], [60, 0]),
        ] {
            t += &format!(
                "0\nLINE\n10\n{}\n20\n{}\n11\n{}\n21\n{}\n",
                a[0], a[1], b[0], b[1]
            );
        }
        t += "0\nENDSEC\n0\nEOF\n";
        let f = dxf(&t).unwrap();
        assert_eq!(f.len(), 3);
        let half_disc = std::f64::consts::PI * 25.0 / 2.0;
        assert!(
            (area(vec![f[0].clone()]) - (200.0 + half_disc)).abs() < 0.5,
            "{}",
            area(vec![f[0].clone()])
        );
        assert!((area(vec![f[2].clone()]) - 100.0).abs() < 1e-6);
    }

    #[test]
    fn dxf_arcs_and_r12_polyline_preserve_true_arcs() {
        // 1. Third-party DXF capsule (stadium 30 x 10 + two R=5 end caps) built from two LINEs and
        //    two ARC entities in DXF Y-up.
        let mut t = String::from("0\nSECTION\n2\nENTITIES\n");
        t += "0\nLINE\n10\n0.0\n20\n5.0\n11\n30.0\n21\n5.0\n";
        t += "0\nARC\n10\n30.0\n20\n0.0\n40\n5.0\n50\n-90.0\n51\n90.0\n";
        t += "0\nLINE\n10\n30.0\n20\n-5.0\n11\n0.0\n21\n-5.0\n";
        t += "0\nARC\n10\n0.0\n20\n0.0\n40\n5.0\n50\n90.0\n51\n270.0\n";
        // 2. Separate R12 POLYLINE + VERTEX + SEQEND rounded stadium at X=[60..80], R=4.
        t += "0\nPOLYLINE\n66\n1\n70\n1\n";
        t += "0\nVERTEX\n10\n60.0\n20\n4.0\n";
        t += "0\nVERTEX\n10\n80.0\n20\n4.0\n42\n-1.0\n";
        t += "0\nVERTEX\n10\n80.0\n20\n-4.0\n";
        t += "0\nVERTEX\n10\n60.0\n20\n-4.0\n42\n-1.0\n";
        t += "0\nSEQEND\n0\nENDSEC\n0\nEOF\n";

        let f = dxf(&t).unwrap();
        assert_eq!(f.len(), 2);

        // Capsule from POLYLINE (f[0]): exact 2 lines + 2 arcs, approximate == false.
        let ce_poly = evaluate_curves(&ShapeDoc {
            features: vec![f[0].clone()],
            ..Default::default()
        });
        assert!(!ce_poly.approximate);
        assert_eq!(ce_poly.outline.len(), 1);
        assert_eq!(
            ce_poly.outline[0]
                .elements
                .iter()
                .filter(|e| matches!(e.kind, ElementKind::Arc { .. }))
                .count(),
            2
        );

        // Capsule from LINE + ARC chain (f[1]): exact 2 lines + 2 arcs, approximate == false.
        let ce_arc = evaluate_curves(&ShapeDoc {
            features: vec![f[1].clone()],
            ..Default::default()
        });
        assert!(!ce_arc.approximate);
        assert_eq!(ce_arc.outline.len(), 1);
        assert_eq!(
            ce_arc.outline[0]
                .elements
                .iter()
                .filter(|e| matches!(e.kind, ElementKind::Arc { .. }))
                .count(),
            2
        );
        let exact_area = 30.0 * 10.0 + std::f64::consts::PI * 25.0;
        assert!((area(vec![f[1].clone()]) - exact_area).abs() < 0.1);
    }
}
