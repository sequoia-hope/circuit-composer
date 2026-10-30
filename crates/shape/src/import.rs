//! Outlines from drawings: SVG (polygon, polyline, rect, circle, ellipse, path with lines and
//! Bézier curves flattened; arcs as their chord) and DXF (closed LWPOLYLINE with bulges, CIRCLE,
//! and LINEs that chain into closed loops). Each closed shape becomes a polygon feature in mm.
//! SVG user units are taken as mm unless the root's width/height say otherwise (`mm`, `in`,
//! `px` at 96 per inch); transforms are not applied.

use crate::{Feature, Geom, Op, P};

/// Closed shapes from an SVG document, as polygon features.
pub fn svg(text: &str) -> Result<Vec<Feature>, String> {
    let scale = svg_scale(text);
    let mut out = Vec::new();
    for (tag, attrs) in tags(text) {
        let a = |k: &str| attr(&attrs, k);
        let f = |k: &str| a(k).and_then(|v| v.trim().trim_end_matches("px").parse::<f64>().ok()).unwrap_or(0.0);
        let id = a("id").unwrap_or_else(|| format!("{tag} {}", out.len() + 1));
        let mut loops: Vec<Vec<P>> = Vec::new();
        match tag.as_str() {
            "polygon" | "polyline" => {
                let n = numbers(&a("points").unwrap_or_default());
                loops.push(n.chunks_exact(2).map(|c| [c[0], c[1]]).collect());
            }
            "rect" => {
                let (x, y, w, h) = (f("x"), f("y"), f("width"), f("height"));
                loops.push(vec![[x, y], [x + w, y], [x + w, y + h], [x, y + h]]);
            }
            "circle" | "ellipse" => {
                let (cx, cy) = (f("cx"), f("cy"));
                let (rx, ry) = if tag == "circle" { (f("r"), f("r")) } else { (f("rx"), f("ry")) };
                loops.push((0..72).map(|i| {
                    let t = 2.0 * std::f64::consts::PI * i as f64 / 72.0;
                    [cx + rx * t.cos(), cy + ry * t.sin()]
                }).collect());
            }
            "path" => loops.extend(path(&a("d").unwrap_or_default())?),
            _ => continue,
        }
        for (k, l) in loops.into_iter().filter(|l| l.len() >= 3).enumerate() {
            let points = l.into_iter().map(|p| [p[0] * scale, p[1] * scale]).collect();
            out.push(Feature { name: if k == 0 { id.clone() } else { format!("{id} ({})", k + 1) }, op: Op::Add, geom: Geom::Polygon { points }, hidden: false });
        }
    }
    if out.is_empty() {
        return Err("no closed shapes in the SVG".into());
    }
    Ok(out)
}

/// mm per user unit, from the root element's width and viewBox.
fn svg_scale(text: &str) -> f64 {
    let Some((_, root)) = tags(text).into_iter().find(|(t, _)| t == "svg") else { return 1.0 };
    let Some(w) = attr(&root, "width") else { return 1.0 };
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
    let width: f64 = w.trim_end_matches(|c: char| c.is_alphabetic()).trim().parse().unwrap_or(0.0);
    let vb = attr(&root, "viewBox").map(|v| numbers(&v)).unwrap_or_default();
    let user = if vb.len() == 4 && vb[2] > 0.0 { vb[2] } else { width };
    if width > 0.0 && user > 0.0 { width * unit / user } else { 1.0 }
}

/// An SVG path's closed subpaths.
fn path(d: &str) -> Result<Vec<Vec<P>>, String> {
    let toks = path_tokens(d);
    let mut loops = Vec::new();
    let mut cur: Vec<P> = Vec::new();
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
                    loops.push(std::mem::take(&mut cur));
                } else {
                    cur.clear();
                }
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
                    loops.push(std::mem::take(&mut cur));
                }
                cur.clear();
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
                let c1 = if cmd.to_ascii_uppercase() == 'C' {
                    [bx + num(&mut i)?, by + num(&mut i)?]
                } else {
                    last_ctrl.map(|c| [2.0 * x - c[0], 2.0 * y - c[1]]).unwrap_or([x, y])
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
                let c = if cmd.to_ascii_uppercase() == 'Q' {
                    [bx + num(&mut i)?, by + num(&mut i)?]
                } else {
                    last_ctrl.map(|c| [2.0 * x - c[0], 2.0 * y - c[1]]).unwrap_or([x, y])
                };
                let e = [bx + num(&mut i)?, by + num(&mut i)?];
                for k in 1..=12 {
                    let t = k as f64 / 12.0;
                    let m = 1.0 - t;
                    cur.push([m * m * x + 2.0 * m * t * c[0] + t * t * e[0], m * m * y + 2.0 * m * t * c[1] + t * t * e[1]]);
                }
                last_ctrl = Some(c);
                x = e[0];
                y = e[1];
            }
            'A' => {
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
        loops.push(cur);
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
    path_tokens(s).into_iter().filter_map(|t| if let PathTok::Num(v) = t { Some(v) } else { None }).collect()
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
        let name: String = body.chars().take_while(|c| !c.is_whitespace() && *c != '/').collect();
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

/// Closed shapes from a DXF (ASCII) drawing, in its units (taken as mm), as polygon features.
pub fn dxf(text: &str) -> Result<Vec<Feature>, String> {
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    let pairs: Vec<(i32, &str)> = lines.chunks_exact(2).filter_map(|c| c[0].parse().ok().map(|k| (k, c[1]))).collect();
    let mut out = Vec::new();
    let mut segments: Vec<(P, P)> = Vec::new();
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
        let get = |code: i32| ent.iter().find(|(k, _)| *k == code).and_then(|(_, v)| v.parse::<f64>().ok());
        match kind {
            "LWPOLYLINE" => {
                let closed = get(70).map(|f| (f as i64) & 1 == 1).unwrap_or(false);
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
                if closed && verts.len() >= 2 {
                    let mut pts = Vec::new();
                    for k in 0..verts.len() {
                        let (a, bulge) = verts[k];
                        let b = verts[(k + 1) % verts.len()].0;
                        pts.push(a);
                        pts.extend(bulge_arc(a, b, bulge));
                    }
                    out.push(Feature { name: format!("polyline {}", out.len() + 1), op: Op::Add, geom: Geom::Polygon { points: flip(pts) }, hidden: false });
                }
            }
            "CIRCLE" => {
                if let (Some(x), Some(y), Some(r)) = (get(10), get(20), get(40)) {
                    out.push(Feature { name: format!("circle {}", out.len() + 1), op: Op::Add, geom: Geom::Circle { x, y: -y, d: 2.0 * r }, hidden: false });
                }
            }
            "LINE" => {
                if let (Some(x1), Some(y1), Some(x2), Some(y2)) = (get(10), get(20), get(11), get(21)) {
                    segments.push(([x1, y1], [x2, y2]));
                }
            }
            _ => {}
        }
        i = j;
    }
    for l in chain(segments) {
        out.push(Feature { name: format!("lines {}", out.len() + 1), op: Op::Add, geom: Geom::Polygon { points: flip(l) }, hidden: false });
    }
    if out.is_empty() {
        return Err("no closed shapes in the DXF (closed LWPOLYLINE, CIRCLE, or LINEs forming loops)".into());
    }
    Ok(out)
}

/// DXF's y is up; the board's is down.
fn flip(pts: Vec<P>) -> Vec<P> {
    pts.into_iter().map(|p| [p[0], -p[1]]).collect()
}

/// The points strictly between a and b on a bulged segment (bulge = tan(angle / 4)).
fn bulge_arc(a: P, b: P, bulge: f64) -> Vec<P> {
    if bulge.abs() < 1e-9 {
        return Vec::new();
    }
    let theta = 4.0 * bulge.atan();
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let chord = dx.hypot(dy);
    if chord < 1e-12 {
        return Vec::new();
    }
    let r = chord / (2.0 * (theta / 2.0).sin());
    // Centre: from the chord's midpoint, along its normal.
    let (mx, my) = ((a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0);
    let h = r * (theta / 2.0).cos();
    let (nx, ny) = (-dy / chord, dx / chord);
    let (cx, cy) = (mx + nx * h, my + ny * h);
    let a0 = (a[1] - cy).atan2(a[0] - cx);
    let n = ((theta.abs() / (std::f64::consts::PI / 36.0)).ceil() as usize).max(2);
    (1..n).map(|k| {
        let t = a0 + theta * k as f64 / n as f64;
        [cx + r.abs() * t.cos(), cy + r.abs() * t.sin()]
    }).collect()
}

/// Segments joined end to end into closed loops (ends within 1 um); open chains are dropped.
fn chain(mut segs: Vec<(P, P)>) -> Vec<Vec<P>> {
    let near = |a: P, b: P| (a[0] - b[0]).abs() < 1e-3 && (a[1] - b[1]).abs() < 1e-3;
    let mut loops = Vec::new();
    while let Some((a, b)) = segs.pop() {
        let mut l = vec![a, b];
        loop {
            let end = *l.last().unwrap();
            if near(end, l[0]) && l.len() > 3 {
                l.pop();
                loops.push(l);
                break;
            }
            let Some(k) = segs.iter().position(|(p, q)| near(*p, end) || near(*q, end)) else { break };
            let (p, q) = segs.swap_remove(k);
            l.push(if near(p, end) { q } else { p });
        }
    }
    loops
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{evaluate, ShapeDoc};

    fn area(features: Vec<Feature>) -> f64 {
        evaluate(&ShapeDoc { features, ..Default::default() }).area_mm2
    }

    #[test]
    fn svg_shapes_in_mm() {
        let text = r#"<svg xmlns="http://www.w3.org/2000/svg" width="40mm" height="30mm" viewBox="0 0 40 30">
            <rect id="board" x="0" y="0" width="40" height="30"/>
            <path d="M 50 0 h 10 v 10 h -10 z m 20 0 l 10 0 l 0 10 l -10 0 Z"/>
            <circle cx="100" cy="100" r="5"/></svg>"#;
        let f = svg(text).unwrap();
        assert_eq!(f.len(), 4, "{:?}", f.iter().map(|x| &x.name).collect::<Vec<_>>());
        assert_eq!(f[0].name, "board");
        assert!((area(vec![f[0].clone()]) - 1200.0).abs() < 1e-6);
        assert!((area(vec![f[1].clone(), f[2].clone()]) - 200.0).abs() < 1e-6);
    }

    #[test]
    fn svg_pixels_are_scaled() {
        let f = svg(r#"<svg width="96px" height="96px"><polygon points="0,0 96,0 96,96 0,96"/></svg>"#).unwrap();
        assert!((area(f) - 25.4 * 25.4).abs() < 1e-3);
    }

    #[test]
    fn svg_curves_are_flattened() {
        // A circle of radius 10 from four cubic quarter arcs.
        let k = 5.5228;
        let d = format!("M 10 0 C 10 {k} {k} 10 0 10 C -{k} 10 -10 {k} -10 0 C -10 -{k} -{k} -10 0 -10 C {k} -10 10 -{k} 10 0 Z");
        let f = svg(&format!(r#"<svg><path d="{d}"/></svg>"#)).unwrap();
        assert!((area(f) - std::f64::consts::PI * 100.0).abs() < 0.5);
    }

    #[test]
    fn dxf_polylines_circles_and_lines() {
        let mut t = String::from("0\nSECTION\n2\nENTITIES\n");
        // A 20 x 10 closed polyline whose right side bulges out into a half circle of radius 5.
        t += "0\nLWPOLYLINE\n90\n4\n70\n1\n10\n0\n20\n0\n10\n20\n20\n0\n42\n1\n10\n20\n20\n10\n10\n0\n20\n10\n";
        t += "0\nCIRCLE\n10\n50\n20\n0\n40\n3\n";
        for (a, b) in [([60, 0], [70, 0]), ([70, 0], [70, 10]), ([70, 10], [60, 10]), ([60, 10], [60, 0])] {
            t += &format!("0\nLINE\n10\n{}\n20\n{}\n11\n{}\n21\n{}\n", a[0], a[1], b[0], b[1]);
        }
        t += "0\nENDSEC\n0\nEOF\n";
        let f = dxf(&t).unwrap();
        assert_eq!(f.len(), 3);
        let half_disc = std::f64::consts::PI * 25.0 / 2.0;
        assert!((area(vec![f[0].clone()]) - (200.0 + half_disc)).abs() < 0.5, "{}", area(vec![f[0].clone()]));
        assert!((area(vec![f[2].clone()]) - 100.0).abs() < 1e-6);
    }
}
