//! A board's shape: its outline, built from features (rectangles, circles, slots, polygons and
//! parametric generators), each added or cut away in order; corner rounding and symmetry over
//! the result; mounting holes; keep-outs by side. `shape.json` is this document, and `evaluate`
//! turns it into polygons in board millimetres (KiCad's frame, y down), the outline the
//! floorplanner keeps every part inside.
//!
//! Geometry is done by Clipper2 (`clipper2-rust`, a pure Rust port) on integer coordinates at
//! 0.1 um.

use std::collections::BTreeMap;

use clipper2_rust::{difference_64, inflate_paths_64, intersect_64, union_64, EndType, FillRule, JoinType, Path64, Paths64, Point64};
use serde::{Deserialize, Serialize};

pub mod generators;
pub mod import;

pub type P = [f64; 2];

const SCALE: f64 = 10_000.0;
/// How far a round join's chords may stray from the true arc, mm.
const ARC_TOLERANCE: f64 = 0.005;

/// `shape.json`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ShapeDoc {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Applied in order: each adds to the outline or cuts it away.
    pub features: Vec<Feature>,
    /// Every corner of the result, convex and concave, rounded to this radius (mm).
    #[serde(default)]
    pub corner_radius: f64,
    #[serde(default)]
    pub symmetry: Symmetry,
    #[serde(default)]
    pub holes: Vec<Hole>,
    #[serde(default)]
    pub keepouts: Vec<Keepout>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Feature {
    pub name: String,
    #[serde(default)]
    pub op: Op,
    pub geom: Geom,
    /// Left out of the result, kept in the document.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hidden: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Op {
    #[default]
    Add,
    Subtract,
}

/// A feature's geometry. Positions are centres; mm and degrees.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Geom {
    Rect {
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        /// Corner radius.
        #[serde(default)]
        r: f64,
        #[serde(default)]
        angle: f64,
    },
    Circle { x: f64, y: f64, d: f64 },
    /// A stadium: the segment from (x1, y1) to (x2, y2), `w` wide with round ends.
    Slot { x1: f64, y1: f64, x2: f64, y2: f64, w: f64 },
    Polygon { points: Vec<P> },
    /// A parametric shape from `generators`: parameters not given take their defaults.
    Generator {
        name: String,
        #[serde(default)]
        params: BTreeMap<String, f64>,
    },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SymmetryKind {
    #[default]
    None,
    /// Mirrored in the vertical line through the centre (left matches right).
    MirrorX,
    /// Mirrored in the horizontal line through the centre.
    MirrorY,
    /// Unchanged by a half turn about the centre.
    HalfTurn,
}

/// Made symmetric by adding the result's image to itself.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct Symmetry {
    #[serde(default)]
    pub kind: SymmetryKind,
    #[serde(default)]
    pub centre: P,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hole {
    pub x: f64,
    pub y: f64,
    /// Drill diameter.
    pub d: f64,
    #[serde(default)]
    pub plated: bool,
    /// Copper and parts kept this far from the drill's edge.
    #[serde(default)]
    pub clearance: f64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    #[default]
    Both,
    Front,
    Back,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Keepout {
    pub name: String,
    pub geom: Geom,
    #[serde(default)]
    pub side: Side,
}

/// A shape, evaluated.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Shape {
    /// The board: its outer edge first (the largest piece), then any separate pieces.
    pub outline: Vec<Vec<P>>,
    /// Holes through the board that subtract features made (inner edges).
    pub cutouts: Vec<Vec<P>>,
    pub area_mm2: f64,
    /// x0, y0, x1, y1.
    pub bbox: [f64; 4],
    pub holes: Vec<Hole>,
    pub keepouts: Vec<KeepoutShape>,
    /// Each feature's own outline, in document order (empty when hidden), for drawing handles.
    pub features: Vec<Vec<Vec<P>>>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct KeepoutShape {
    pub name: String,
    pub side: Side,
    pub polygon: Vec<Vec<P>>,
}

pub fn evaluate(doc: &ShapeDoc) -> Shape {
    let mut warnings = Vec::new();
    let mut acc = Paths64::new();
    let mut features = Vec::new();
    for f in &doc.features {
        if f.hidden {
            features.push(Vec::new());
            continue;
        }
        let g = match geom(&f.geom) {
            Ok(g) => g,
            Err(e) => {
                warnings.push(format!("{}: {e}", f.name));
                features.push(Vec::new());
                continue;
            }
        };
        features.push(to_mm(&g));
        acc = match f.op {
            Op::Add => union_64(&acc, &g, FillRule::NonZero),
            Op::Subtract => difference_64(&acc, &g, FillRule::NonZero),
        };
    }
    acc = symmetric(&acc, &doc.symmetry);
    if doc.corner_radius > 0.0 {
        acc = round(&acc, doc.corner_radius);
    }
    let mut outer: Vec<Path64> = acc.iter().filter(|p| signed_area(p) > 0.0).cloned().collect();
    let inner: Vec<Path64> = acc.iter().filter(|p| signed_area(p) < 0.0).cloned().collect();
    outer.sort_by(|a, b| signed_area(b).total_cmp(&signed_area(a)));
    if outer.len() > 1 {
        warnings.push(format!("the outline is in {} separate pieces", outer.len()));
    }
    if outer.is_empty() && !doc.features.is_empty() {
        warnings.push("nothing is left of the outline".into());
    }
    let area = acc.iter().map(signed_area).sum::<f64>() / (SCALE * SCALE);
    let outline = to_mm(&outer);
    let mut bbox = [f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY];
    for p in outline.iter().flatten() {
        bbox = [bbox[0].min(p[0]), bbox[1].min(p[1]), bbox[2].max(p[0]), bbox[3].max(p[1])];
    }
    if outline.is_empty() {
        bbox = [0.0; 4];
    }
    let keepouts = doc
        .keepouts
        .iter()
        .filter_map(|k| match geom(&k.geom) {
            Ok(g) => Some(KeepoutShape { name: k.name.clone(), side: k.side, polygon: to_mm(&g) }),
            Err(e) => {
                warnings.push(format!("keep-out {}: {e}", k.name));
                None
            }
        })
        .collect();
    for h in &doc.holes {
        if h.d <= 0.0 {
            warnings.push(format!("a hole at ({}, {}) has no diameter", h.x, h.y));
        }
    }
    Shape { outline, cutouts: to_mm(&inner), area_mm2: area, bbox, holes: doc.holes.clone(), keepouts, features, warnings }
}

/// One feature's geometry as closed paths.
pub fn geom(g: &Geom) -> Result<Paths64, String> {
    match g {
        Geom::Rect { x, y, w, h, r, angle } => {
            if *w <= 0.0 || *h <= 0.0 {
                return Err("a rectangle needs a width and a height".into());
            }
            let r = r.clamp(0.0, w.min(*h) / 2.0);
            let (hw, hh) = (w / 2.0 - r, h / 2.0 - r);
            let core = vec![vec![[-hw, -hh], [hw, -hh], [hw, hh], [-hw, hh]]];
            let mut p = if r > 0.0 {
                if hw <= 0.0 && hh <= 0.0 {
                    circle(0.0, 0.0, r)
                } else {
                    inflate(&from_mm(&core), r)
                }
            } else {
                from_mm(&core)
            };
            transform(&mut p, *x, *y, *angle);
            Ok(p)
        }
        Geom::Circle { x, y, d } => {
            if *d <= 0.0 {
                return Err("a circle needs a diameter".into());
            }
            Ok(circle(*x, *y, d / 2.0))
        }
        Geom::Slot { x1, y1, x2, y2, w } => {
            if *w <= 0.0 {
                return Err("a slot needs a width".into());
            }
            let line = vec![vec![Point64::new(s(*x1), s(*y1)), Point64::new(s(*x2), s(*y2))]];
            Ok(inflate_paths_64(&line, s(w / 2.0) as f64, JoinType::Round, EndType::Round, 2.0, ARC_TOLERANCE * SCALE))
        }
        Geom::Polygon { points } => {
            if points.len() < 3 {
                return Err("a polygon needs three points".into());
            }
            Ok(union_64(&from_mm(&[points.clone()]), &Paths64::new(), FillRule::NonZero))
        }
        Geom::Generator { name, params } => {
            let gen = generators::find(name).ok_or_else(|| format!("no generator `{name}`"))?;
            let pts = (gen.build)(&generators::with_defaults(gen, params))?;
            Ok(union_64(&from_mm(&pts), &Paths64::new(), FillRule::NonZero))
        }
    }
}

fn symmetric(acc: &Paths64, sym: &Symmetry) -> Paths64 {
    let (cx, cy) = (s(sym.centre[0]), s(sym.centre[1]));
    let image: Paths64 = match sym.kind {
        SymmetryKind::None => return acc.clone(),
        SymmetryKind::MirrorX => acc.iter().map(|p| p.iter().rev().map(|q| Point64::new(2 * cx - q.x, q.y)).collect()).collect(),
        SymmetryKind::MirrorY => acc.iter().map(|p| p.iter().rev().map(|q| Point64::new(q.x, 2 * cy - q.y)).collect()).collect(),
        SymmetryKind::HalfTurn => acc.iter().map(|p| p.iter().map(|q| Point64::new(2 * cx - q.x, 2 * cy - q.y)).collect()).collect(),
    };
    union_64(acc, &image, FillRule::NonZero)
}

/// Round every corner to `r`: concave ones by closing (grow, shrink), convex ones by opening.
pub fn round(paths: &Paths64, r: f64) -> Paths64 {
    inflate(&inflate(&inflate(paths, r), -2.0 * r), r)
}

/// Grow (or, negative, shrink) by `d` mm with round joins.
pub fn inflate(paths: &Paths64, d: f64) -> Paths64 {
    inflate_paths_64(paths, d * SCALE, JoinType::Round, EndType::Polygon, 2.0, ARC_TOLERANCE * SCALE)
}

pub fn intersect(a: &Paths64, b: &Paths64) -> Paths64 {
    intersect_64(a, b, FillRule::NonZero)
}

pub fn union(a: &Paths64, b: &Paths64) -> Paths64 {
    union_64(a, b, FillRule::NonZero)
}

fn circle(x: f64, y: f64, r: f64) -> Paths64 {
    // Chords within ARC_TOLERANCE of the circle.
    let n = ((std::f64::consts::PI / (1.0 - ARC_TOLERANCE / r).clamp(-1.0, 1.0).acos()).ceil() as usize).clamp(12, 720);
    vec![(0..n)
        .map(|i| {
            let a = 2.0 * std::f64::consts::PI * i as f64 / n as f64;
            Point64::new(s(x + r * a.cos()), s(y + r * a.sin()))
        })
        .collect()]
}

fn transform(p: &mut Paths64, x: f64, y: f64, angle: f64) {
    let (sn, cs) = angle.to_radians().sin_cos();
    for path in p.iter_mut() {
        for q in path.iter_mut() {
            let (px, py) = (q.x as f64 / SCALE, q.y as f64 / SCALE);
            // KiCad's sense: counter-clockwise on screen with y down.
            *q = Point64::new(s(x + px * cs + py * sn), s(y - px * sn + py * cs));
        }
    }
}

fn s(v: f64) -> i64 {
    (v * SCALE).round() as i64
}

/// Area with y down: positive for a path that runs clockwise on screen, as Clipper's outer paths
/// do in this frame.
fn signed_area(p: &Path64) -> f64 {
    let mut a = 0.0;
    for i in 0..p.len() {
        let (u, v) = (p[i], p[(i + 1) % p.len()]);
        a += u.x as f64 * v.y as f64 - v.x as f64 * u.y as f64;
    }
    a / 2.0
}

pub fn from_mm(paths: &[Vec<P>]) -> Paths64 {
    paths.iter().map(|p| p.iter().map(|q| Point64::new(s(q[0]), s(q[1]))).collect()).collect()
}

pub fn to_mm(paths: &Paths64) -> Vec<Vec<P>> {
    paths.iter().map(|p| p.iter().map(|q| [q.x as f64 / SCALE, q.y as f64 / SCALE]).collect()).collect()
}

/// The convex hull of points (Andrew's monotone chain).
pub fn hull(mut pts: Vec<P>) -> Vec<P> {
    pts.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    pts.dedup();
    if pts.len() < 3 {
        return pts;
    }
    let cross = |o: P, a: P, b: P| (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0]);
    let mut lo: Vec<P> = Vec::new();
    for &p in &pts {
        while lo.len() >= 2 && cross(lo[lo.len() - 2], lo[lo.len() - 1], p) <= 0.0 {
            lo.pop();
        }
        lo.push(p);
    }
    let mut hi: Vec<P> = Vec::new();
    for &p in pts.iter().rev() {
        while hi.len() >= 2 && cross(hi[hi.len() - 2], hi[hi.len() - 1], p) <= 0.0 {
            hi.pop();
        }
        hi.push(p);
    }
    lo.pop();
    hi.pop();
    lo.extend(hi);
    lo
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(features: Vec<Feature>) -> ShapeDoc {
        ShapeDoc { features, ..Default::default() }
    }

    fn add(name: &str, geom: Geom) -> Feature {
        Feature { name: name.into(), op: Op::Add, geom, hidden: false }
    }

    #[test]
    fn a_rounded_rectangle_has_the_area_it_should() {
        let s = evaluate(&doc(vec![add("board", Geom::Rect { x: 0.0, y: 0.0, w: 40.0, h: 30.0, r: 3.0, angle: 0.0 })]));
        // Arcs are chords within ARC_TOLERANCE inside the true arc: a little area is lost.
        let want = 40.0 * 30.0 - (4.0 - std::f64::consts::PI) * 9.0;
        assert!(s.area_mm2 < want && want - s.area_mm2 < 0.1, "{} vs {want}", s.area_mm2);
        assert_eq!(s.bbox.map(|v| v.round()), [-20.0, -15.0, 20.0, 15.0]);
        assert!(s.warnings.is_empty());
    }

    #[test]
    fn subtracting_inside_makes_a_cutout_and_at_the_edge_a_notch() {
        let mut d = doc(vec![
            add("board", Geom::Rect { x: 0.0, y: 0.0, w: 40.0, h: 30.0, r: 0.0, angle: 0.0 }),
            Feature { name: "window".into(), op: Op::Subtract, geom: Geom::Circle { x: 0.0, y: 0.0, d: 10.0 }, hidden: false },
        ]);
        let s = evaluate(&d);
        assert_eq!((s.outline.len(), s.cutouts.len()), (1, 1));
        let want = 1200.0 - std::f64::consts::PI * 25.0;
        assert!(s.area_mm2 > want && s.area_mm2 - want < 0.2, "{}", s.area_mm2);
        d.features[1].geom = Geom::Circle { x: 20.0, y: 0.0, d: 10.0 };
        let s = evaluate(&d);
        assert_eq!((s.outline.len(), s.cutouts.len()), (1, 0));
    }

    #[test]
    fn symmetry_adds_the_image() {
        let mut d = doc(vec![add("half", Geom::Polygon { points: vec![[0.0, 0.0], [10.0, 0.0], [0.0, 20.0]] })]);
        d.symmetry = Symmetry { kind: SymmetryKind::MirrorX, centre: [0.0, 0.0] };
        let s = evaluate(&d);
        assert!((s.area_mm2 - 200.0).abs() < 1e-3);
        assert_eq!(s.bbox, [-10.0, 0.0, 10.0, 20.0]);
    }

    #[test]
    fn rounding_rounds_both_kinds_of_corner() {
        // An L: five convex corners and one concave.
        let mut d = doc(vec![add("l", Geom::Polygon { points: vec![[0.0, 0.0], [20.0, 0.0], [20.0, 10.0], [10.0, 10.0], [10.0, 20.0], [0.0, 20.0]] })]);
        let sharp = evaluate(&d).area_mm2;
        d.corner_radius = 2.0;
        let r = evaluate(&d).area_mm2;
        // Five convex corners lose (4 - pi) r^2 / 4 * 4 each; the concave one gains as much.
        let corner = (4.0 - std::f64::consts::PI) * 4.0 / 4.0 * 4.0 / 4.0;
        assert!((sharp - r - 4.0 * corner).abs() < 0.05, "{sharp} {r}");
    }

    #[test]
    fn a_bad_feature_is_a_warning_not_a_failure() {
        let s = evaluate(&doc(vec![
            add("board", Geom::Rect { x: 0.0, y: 0.0, w: 10.0, h: 10.0, r: 0.0, angle: 0.0 }),
            add("broken", Geom::Circle { x: 0.0, y: 0.0, d: 0.0 }),
            add("ghost", Geom::Generator { name: "nope".into(), params: BTreeMap::new() }),
        ]));
        assert_eq!(s.warnings.len(), 2, "{:?}", s.warnings);
        assert!((s.area_mm2 - 100.0).abs() < 1e-6);
    }
}
