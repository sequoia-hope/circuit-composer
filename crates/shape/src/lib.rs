//! A board's shape: its outline, built from features (rectangles, circles, slots, polygons and
//! parametric generators), each added or cut away in order; corner rounding and symmetry over
//! the result; mounting holes; keep-outs by side. `shape.json` is this document, and `evaluate`
//! turns it into polygons in board millimetres (KiCad's frame, y down), the outline the
//! floorplanner keeps every part inside.
//!
//! Geometry is done by Clipper2 (`clipper2-rust`, a pure Rust port) on integer coordinates at
//! 0.1 um, while [`curves::evaluate_curves`] preserves exact `Line`/`Arc`/`Circle` elements in
//! whole nanometres for `board-interface`.

use std::collections::{BTreeMap, BTreeSet};

pub use board_interface;
use clipper2_rust::{
    difference_64, inflate_paths_64, intersect_64, union_64, EndType, FillRule, JoinType, Path64,
    Paths64, Point64,
};
use serde::{Deserialize, Serialize};

pub mod curves;
pub mod generators;
pub mod import;
pub mod sync;

pub use curves::{CurveEval, evaluate_curves, loop_area_mm2, loop_perimeter_mm, loop_to_polygon_mm};
pub use sync::{
    FollowedState, PublishOutcome, PullOutcome, RecipeAside, StatusOutcome, SyncState,
    SyncedGeometry, TakeOverOutcome, driver_label, is_locked, mint_ids, prepare_publish,
    prepare_publish_with_followed, pull, status, take_over,
};

pub type P = [f64; 2];

const SCALE: f64 = 10_000.0;
/// How far a round join's chords may stray from the true arc, mm.
pub const ARC_TOLERANCE: f64 = 0.005;
/// Default PCB thickness, mm.
pub const DEFAULT_THICKNESS_MM: f64 = 1.6;

fn default_thickness() -> f64 {
    DEFAULT_THICKNESS_MM
}

fn is_default_thickness(v: &f64) -> bool {
    (*v - DEFAULT_THICKNESS_MM).abs() < 1e-12
}

fn is_zero_u64(v: &u64) -> bool {
    *v == 0
}

/// `shape.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShapeDoc {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Applied in order: each adds to the outline or cuts it away.
    #[serde(default)]
    pub features: Vec<Feature>,
    /// Every corner of the result, convex and concave, rounded to this radius (mm).
    #[serde(default)]
    pub corner_radius: f64,
    #[serde(default)]
    pub symmetry: Symmetry,
    /// Board thickness (mm).
    #[serde(default = "default_thickness", skip_serializing_if = "is_default_thickness")]
    pub thickness: f64,
    #[serde(default)]
    pub holes: Vec<Hole>,
    #[serde(default)]
    pub keepouts: Vec<Keepout>,
    /// High-water counter for minted `composer:hole/<n>` IDs so retired hole IDs are never reused.
    #[serde(default, skip_serializing_if = "is_zero_u64")]
    pub next_hole_id: u64,
    /// High-water counter for minted `composer:keepout/<n>` IDs so retired keep-out IDs are never reused.
    #[serde(default, skip_serializing_if = "is_zero_u64")]
    pub next_keepout_id: u64,
    /// All hole and keep-out IDs ever minted on this document, kept across deletions and restarts.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub minted_ids: BTreeSet<String>,
    /// Followed `board-interface` revision, driver, synced geometry, and set-aside recipe.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sync: Option<SyncState>,
}

impl Default for ShapeDoc {
    fn default() -> Self {
        Self {
            note: None,
            features: Vec::new(),
            corner_radius: 0.0,
            symmetry: Symmetry::default(),
            thickness: DEFAULT_THICKNESS_MM,
            holes: Vec::new(),
            keepouts: Vec::new(),
            next_hole_id: 0,
            next_keepout_id: 0,
            minted_ids: BTreeSet::new(),
            sync: None,
        }
    }
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
    Circle {
        x: f64,
        y: f64,
        d: f64,
    },
    /// A stadium: the segment from (x1, y1) to (x2, y2), `w` wide with round ends.
    Slot {
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        w: f64,
    },
    Polygon {
        points: Vec<P>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        approximate: bool,
    },
    /// Exact curve loop in board nanometres (from DXF arc/polyline import or interface take-over).
    Path {
        elements: Vec<board_interface::Element>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        approximate: bool,
    },
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub name: String,
    pub geom: Geom,
    #[serde(default)]
    pub side: Side,
    /// Maximum component height allowed in mm (`Some(0.0)` forbids all components; `None` leaves
    /// height uncapped).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_height: Option<f64>,
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
    if let Some(synced) = doc.sync.as_ref().and_then(|s| s.synced.as_ref()) {
        return evaluate_synced(doc, synced);
    }

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

fn evaluate_synced(doc: &ShapeDoc, synced: &SyncedGeometry) -> Shape {
    let mut warnings = Vec::new();
    let outer_pts = loop_to_polygon_mm(&synced.outline, ARC_TOLERANCE, Some(true));
    let outline = if outer_pts.len() >= 3 {
        vec![outer_pts]
    } else {
        Vec::new()
    };
    let cutouts: Vec<Vec<P>> = synced
        .cutouts
        .iter()
        .map(|c| loop_to_polygon_mm(c, ARC_TOLERANCE, Some(false)))
        .filter(|p| p.len() >= 3)
        .collect();
    let area = outline
        .iter()
        .map(|p| curves::poly_signed_area_mm(p))
        .sum::<f64>()
        + cutouts
            .iter()
            .map(|p| curves::poly_signed_area_mm(p))
            .sum::<f64>();

    let mut bbox = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for p in outline.iter().flatten() {
        bbox = [
            bbox[0].min(p[0]),
            bbox[1].min(p[1]),
            bbox[2].max(p[0]),
            bbox[3].max(p[1]),
        ];
    }
    if outline.is_empty() {
        bbox = [0.0; 4];
    }

    let holes = sync::bi_holes_to_doc(&synced.holes);
    for h in &holes {
        if h.d <= 0.0 {
            warnings.push(format!("a hole at ({}, {}) has no diameter", h.x, h.y));
        }
    }

    let doc_ko_names: Vec<&str> = doc.keepouts.iter().map(|k| k.name.as_str()).collect();
    let keepouts = synced
        .keepouts
        .iter()
        .enumerate()
        .filter_map(|(i, ko)| {
            let poly = loop_to_polygon_mm(&ko.boundary, ARC_TOLERANCE, Some(true));
            if poly.len() < 3 {
                return None;
            }
            let name = doc_ko_names
                .get(i)
                .map(|s| (*s).to_string())
                .or_else(|| {
                    ko.boundary
                        .elements
                        .first()
                        .and_then(|e| e.id.as_deref())
                        .map(|s| s.rsplit(':').next().unwrap_or(s).to_string())
                })
                .unwrap_or_else(|| format!("keepout {}", i + 1));
            Some(KeepoutShape {
                name,
                side: curves::side_from_bi(ko.side),
                polygon: vec![poly],
            })
        })
        .collect();

    Shape {
        outline,
        cutouts,
        area_mm2: area,
        bbox,
        holes,
        keepouts,
        features: doc.features.iter().map(|_| Vec::new()).collect(),
        warnings,
    }
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
        Geom::Polygon { points, .. } => {
            if points.len() < 3 {
                return Err("a polygon needs three points".into());
            }
            Ok(union_64(&from_mm(&[points.clone()]), &Paths64::new(), FillRule::NonZero))
        }
        Geom::Path { elements, .. } => {
            if elements.is_empty() {
                return Err("a path needs at least one element".into());
            }
            let pts = loop_to_polygon_mm(
                &board_interface::Loop::new(elements.clone()),
                ARC_TOLERANCE,
                Some(true),
            );
            if pts.len() < 3 {
                return Err("a path needs three points".into());
            }
            Ok(union_64(&from_mm(&[pts]), &Paths64::new(), FillRule::NonZero))
        }
        Geom::Generator { name, params } => {
            let gen = generators::find(name).ok_or_else(|| format!("no generator `{name}`"))?;
            let pts = (gen.build)(&generators::with_defaults(gen, params))?;
            Ok(union_64(&from_mm(&pts), &Paths64::new(), FillRule::NonZero))
        }
    }
}

pub(crate) fn symmetric(acc: &Paths64, sym: &Symmetry) -> Paths64 {
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
pub(crate) fn signed_area(p: &Path64) -> f64 {
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
    use std::path::PathBuf;

    use board_interface::{ElementKind, hash, read};
    use sha2::{Digest, Sha256};

    use super::*;

    fn doc(features: Vec<Feature>) -> ShapeDoc {
        ShapeDoc { features, ..Default::default() }
    }

    fn add(name: &str, geom: Geom) -> Feature {
        Feature { name: name.into(), op: Op::Add, geom, hidden: false }
    }

    fn sub(name: &str, geom: Geom) -> Feature {
        Feature { name: name.into(), op: Op::Subtract, geom, hidden: false }
    }

    fn sha256_hex(bytes: &[u8]) -> String {
        let mut h = Sha256::new();
        h.update(bytes);
        let d = h.finalize();
        let mut s = String::with_capacity(64);
        for b in d {
            use std::fmt::Write as _;
            let _ = write!(&mut s, "{b:02x}");
        }
        s
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
        let mut d = doc(vec![add("half", Geom::Polygon { points: vec![[0.0, 0.0], [10.0, 0.0], [0.0, 20.0]], approximate: false })]);
        d.symmetry = Symmetry { kind: SymmetryKind::MirrorX, centre: [0.0, 0.0] };
        let s = evaluate(&d);
        assert!((s.area_mm2 - 200.0).abs() < 1e-3);
        assert_eq!(s.bbox, [-10.0, 0.0, 10.0, 20.0]);
    }

    #[test]
    fn rounding_rounds_both_kinds_of_corner() {
        // An L: five convex corners and one concave.
        let mut d = doc(vec![add("l", Geom::Polygon { points: vec![[0.0, 0.0], [20.0, 0.0], [20.0, 10.0], [10.0, 10.0], [10.0, 20.0], [0.0, 20.0]], approximate: false })]);
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

    /// Proves `evaluate` stays byte-identical on every `shape.json` in the repo against the
    /// serialized JSON SHA-256 and exact IEEE-754 bit patterns captured before any edits.
    #[test]
    fn repo_shape_json_polygon_results_are_byte_identical_to_pre_change_capture() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let cases = [
            (
                "boards/easy2350/shape.json",
                280usize,
                "3e78ff404c47d41c5dce42964674bbff0d7fb7f3dbea4f389a47cf784d142b0d",
                0x4090ca68f5c28f5c_u64,
                [
                    4636735532136031846_u64,
                    4636735532136031846,
                    4638216794200971674,
                    4639517736358956237,
                ],
                vec![4usize],
            ),
            (
                "app/web/public/examples/easy2350/shape.json",
                280,
                "3e78ff404c47d41c5dce42964674bbff0d7fb7f3dbea4f389a47cf784d142b0d",
                0x4090ca68f5c28f5c,
                [
                    4636735532136031846,
                    4636735532136031846,
                    4638216794200971674,
                    4639517736358956237,
                ],
                vec![4],
            ),
            (
                "boards/bolt-earring/shape.json",
                65326,
                "f848d4797136dd4a37bc4d3b06a0f7d6950674f4fe1504e2b1281beb3983e4ea",
                0x409178a6d42530bc,
                [
                    13848769123610204832,
                    0,
                    4625484878800665077,
                    4634749557290352142,
                ],
                vec![1817],
            ),
            (
                "app/web/public/examples/bolt-earring/shape.json",
                65326,
                "f848d4797136dd4a37bc4d3b06a0f7d6950674f4fe1504e2b1281beb3983e4ea",
                0x409178a6d42530bc,
                [
                    13848769123610204832,
                    0,
                    4625484878800665077,
                    4634749557290352142,
                ],
                vec![1817],
            ),
        ];

        for (rel, want_len, want_sha, want_area_bits, want_bbox_bits, want_pts) in cases {
            let text = std::fs::read_to_string(root.join(rel)).expect(rel);
            let parsed: ShapeDoc = serde_json::from_str(&text).expect(rel);
            let evaluated = evaluate(&parsed);
            let json_bytes = serde_json::to_vec(&evaluated).unwrap();
            assert_eq!(json_bytes.len(), want_len, "{rel} byte length");
            assert_eq!(sha256_hex(&json_bytes), want_sha, "{rel} sha256");
            assert_eq!(evaluated.area_mm2.to_bits(), want_area_bits, "{rel} area bits");
            assert_eq!(
                evaluated.bbox.map(f64::to_bits),
                want_bbox_bits,
                "{rel} bbox bits"
            );
            let pts: Vec<usize> = evaluated.outline.iter().map(Vec::len).collect();
            assert_eq!(pts, want_pts, "{rel} outline vertex counts");
        }
    }

    fn publish_and_round_trip(mut d: ShapeDoc, name: &str) -> board_interface::Record {
        let out = prepare_publish(&mut d, name, "test-rev", "2026-11-03T12:00:00Z", None)
            .expect("publish succeeds");
        let PublishOutcome::Write {
            dxf,
            manifest_json,
            record,
            ..
        } = out
        else {
            panic!("expected Write");
        };
        let back = read(&dxf, &manifest_json).expect("board_interface::read succeeds");
        assert_eq!(hash(&back), hash(&record), "{name} hash mismatch");
        assert_eq!(back.manifest.geometry_hash, record.manifest.geometry_hash);
        back
    }

    #[test]
    fn composer_authored_primitives_and_booleans_publish_true_arcs_and_round_trip() {
        // 1a. Rounded Rect via feature `r`
        let rect_r = doc(vec![add(
            "board",
            Geom::Rect {
                x: 30.0,
                y: 20.0,
                w: 60.0,
                h: 40.0,
                r: 4.0,
                angle: 0.0,
            },
        )]);
        let poly_area = evaluate(&rect_r).area_mm2;
        let rec = publish_and_round_trip(rect_r, "rect_r");
        assert!(!rec.manifest.approximate);
        assert_eq!(rec.outline.elements.len(), 8);
        assert_eq!(
            rec.outline
                .elements
                .iter()
                .filter(|e| matches!(e.kind, ElementKind::Arc { .. }))
                .count(),
            4
        );
        let exact_rect_area = 60.0 * 40.0 - (4.0 - std::f64::consts::PI) * 16.0;
        assert!((loop_area_mm2(&rec.outline) - exact_rect_area).abs() < 1e-6);
        assert!((loop_area_mm2(&rec.outline) - poly_area).abs() < 0.1);

        // 1b. Rounded Rect via document `corner_radius`
        let mut rect_cr = doc(vec![add(
            "board",
            Geom::Rect {
                x: 30.0,
                y: 20.0,
                w: 60.0,
                h: 40.0,
                r: 0.0,
                angle: 0.0,
            },
        )]);
        rect_cr.corner_radius = 4.0;
        let poly_cr_area = evaluate(&rect_cr).area_mm2;
        let rec_cr = publish_and_round_trip(rect_cr, "rect_cr");
        assert!(!rec_cr.manifest.approximate);
        assert_eq!(rec_cr.outline.elements.len(), 8);
        assert_eq!(
            rec_cr
                .outline
                .elements
                .iter()
                .filter(|e| matches!(e.kind, ElementKind::Arc { .. }))
                .count(),
            4
        );
        assert!((loop_area_mm2(&rec_cr.outline) - exact_rect_area).abs() < 1e-6);
        assert!((loop_area_mm2(&rec_cr.outline) - poly_cr_area).abs() < 0.1);

        // 2. Circle
        let circ_doc = doc(vec![add(
            "disc",
            Geom::Circle {
                x: 15.0,
                y: -10.0,
                d: 30.0,
            },
        )]);
        let poly_circ_area = evaluate(&circ_doc).area_mm2;
        let rec_circ = publish_and_round_trip(circ_doc, "circle");
        assert!(!rec_circ.manifest.approximate);
        assert_eq!(rec_circ.outline.elements.len(), 1);
        assert!(matches!(
            rec_circ.outline.elements[0].kind,
            ElementKind::Circle {
                diameter_nm: 30_000_000,
                ..
            }
        ));
        assert!(
            (loop_area_mm2(&rec_circ.outline) - std::f64::consts::PI * 225.0).abs() < 1e-6
        );
        // Polygon area of an inscribed n-gon with sagitta ARC_TOLERANCE loses ~2/3 * perimeter * ARC_TOLERANCE.
        let circ_tol = loop_perimeter_mm(&rec_circ.outline) * ARC_TOLERANCE;
        assert!(
            (loop_area_mm2(&rec_circ.outline) - poly_circ_area).abs() <= circ_tol,
            "{} vs {poly_circ_area} (tol {circ_tol})",
            loop_area_mm2(&rec_circ.outline)
        );

        // 3. Slot
        let slot_doc = doc(vec![add(
            "pill",
            Geom::Slot {
                x1: 10.0,
                y1: 20.0,
                x2: 40.0,
                y2: 20.0,
                w: 12.0,
            },
        )]);
        let poly_slot_area = evaluate(&slot_doc).area_mm2;
        let rec_slot = publish_and_round_trip(slot_doc, "slot");
        assert!(!rec_slot.manifest.approximate);
        assert_eq!(rec_slot.outline.elements.len(), 4);
        assert_eq!(
            rec_slot
                .outline
                .elements
                .iter()
                .filter(|e| matches!(e.kind, ElementKind::Arc { .. }))
                .count(),
            2
        );
        let exact_slot_area = 30.0 * 12.0 + std::f64::consts::PI * 36.0;
        assert!((loop_area_mm2(&rec_slot.outline) - exact_slot_area).abs() < 1e-6);
        assert!((loop_area_mm2(&rec_slot.outline) - poly_slot_area).abs() < 0.15);

        // 4. Rect + Circle boolean union (with rounded rect corners AND circular ear!)
        let union_doc = doc(vec![
            add(
                "body",
                Geom::Rect {
                    x: 0.0,
                    y: 0.0,
                    w: 40.0,
                    h: 30.0,
                    r: 3.0,
                    angle: 0.0,
                },
            ),
            add(
                "ear",
                Geom::Circle {
                    x: 20.0,
                    y: 0.0,
                    d: 16.0,
                },
            ),
        ]);
        let poly_union_area = evaluate(&union_doc).area_mm2;
        let rec_union = publish_and_round_trip(union_doc, "rect_plus_circle");
        assert!(!rec_union.manifest.approximate);
        // 4 rounded rect corners + 1 semicircular ear = 5 true Arcs!
        let union_arcs = rec_union
            .outline
            .elements
            .iter()
            .filter(|e| matches!(e.kind, ElementKind::Arc { .. }))
            .count();
        assert_eq!(union_arcs, 5, "{:?}", rec_union.outline.elements);
        let exact_union_area =
            (40.0 * 30.0 - (4.0 - std::f64::consts::PI) * 9.0) + 0.5 * std::f64::consts::PI * 64.0;
        assert!((loop_area_mm2(&rec_union.outline) - exact_union_area).abs() < 1e-5);
        assert!((loop_area_mm2(&rec_union.outline) - poly_union_area).abs() < 0.2);

        // 5. Subtracting a Circle & a Slot (cutout + concave edge notch), plus L-polygon with
        //    corner_radius (5 convex + 1 concave true fillet arcs), plus MirrorX symmetry.
        let sub_doc = doc(vec![
            add(
                "body",
                Geom::Rect {
                    x: 0.0,
                    y: 0.0,
                    w: 50.0,
                    h: 40.0,
                    r: 2.0,
                    angle: 0.0,
                },
            ),
            sub(
                "window",
                Geom::Circle {
                    x: -10.0,
                    y: 0.0,
                    d: 10.0,
                },
            ),
            sub(
                "notch",
                Geom::Circle {
                    x: 25.0,
                    y: 0.0,
                    d: 12.0,
                },
            ),
        ]);
        let rec_sub = publish_and_round_trip(sub_doc, "rect_minus_circles");
        assert!(!rec_sub.manifest.approximate);
        assert_eq!(rec_sub.cutouts.len(), 1);
        assert!(matches!(
            rec_sub.cutouts[0].elements[0].kind,
            ElementKind::Circle {
                diameter_nm: 10_000_000,
                ..
            }
        ));
        assert_eq!(
            rec_sub
                .outline
                .elements
                .iter()
                .filter(|e| matches!(e.kind, ElementKind::Arc { .. }))
                .count(),
            5 // 4 rect corners + 1 concave semicircular notch
        );

        // L-shape with post-boolean corner_radius (5 convex + 1 concave fillet arcs):
        let mut l_doc = doc(vec![add(
            "l",
            Geom::Polygon {
                points: vec![
                    [0.0, 0.0],
                    [20.0, 0.0],
                    [20.0, 10.0],
                    [10.0, 10.0],
                    [10.0, 20.0],
                    [0.0, 20.0],
                ],
                approximate: false,
            },
        )]);
        l_doc.corner_radius = 2.0;
        let rec_l = publish_and_round_trip(l_doc, "l_rounded");
        assert!(!rec_l.manifest.approximate);
        assert_eq!(
            rec_l
                .outline
                .elements
                .iter()
                .filter(|e| matches!(e.kind, ElementKind::Arc { .. }))
                .count(),
            6
        );

        // Symmetry (MirrorX) with rounded Rect + Circle ear:
        let mut sym_doc = doc(vec![
            add(
                "core",
                Geom::Rect {
                    x: 0.0,
                    y: 0.0,
                    w: 30.0,
                    h: 20.0,
                    r: 2.0,
                    angle: 0.0,
                },
            ),
            add(
                "ear",
                Geom::Circle {
                    x: 15.0,
                    y: 0.0,
                    d: 10.0,
                },
            ),
        ]);
        sym_doc.symmetry = Symmetry {
            kind: SymmetryKind::MirrorX,
            centre: [0.0, 0.0],
        };
        let rec_sym = publish_and_round_trip(sym_doc, "sym_board");
        assert!(!rec_sym.manifest.approximate);
        assert_eq!(
            rec_sym
                .outline
                .elements
                .iter()
                .filter(|e| matches!(e.kind, ElementKind::Arc { .. }))
                .count(),
            6 // 4 rect corners + right ear + mirrored left ear!
        );

        // Union + post-boolean corner_radius (concave neck fillets between Rect and Circle!):
        let mut union_fillet = doc(vec![
            add(
                "body",
                Geom::Rect {
                    x: 0.0,
                    y: 0.0,
                    w: 40.0,
                    h: 30.0,
                    r: 3.0,
                    angle: 0.0,
                },
            ),
            add(
                "ear",
                Geom::Circle {
                    x: 20.0,
                    y: 0.0,
                    d: 14.0,
                },
            ),
        ]);
        union_fillet.corner_radius = 2.0;
        let rec_uf = publish_and_round_trip(union_fillet, "union_filleted");
        assert!(!rec_uf.manifest.approximate);
        assert_eq!(
            rec_uf
                .outline
                .elements
                .iter()
                .filter(|e| matches!(e.kind, ElementKind::Arc { .. }))
                .count(),
            7 // 4 rect corners + 1 ear arc + 2 concave neck fillets!
        );
    }

    #[test]
    fn generator_board_publishes_approximate_true() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let text = std::fs::read_to_string(root.join("boards/bolt-earring/shape.json")).unwrap();
        let bolt: ShapeDoc = serde_json::from_str(&text).unwrap();
        let rec = publish_and_round_trip(bolt, "bolt-earring");
        assert!(rec.manifest.approximate, "generator outline must be marked approximate");
        assert!(
            rec.outline
                .elements
                .iter()
                .all(|e| matches!(e.kind, ElementKind::Line { .. })),
            "generator chords must not be fit to noisy arcs"
        );
    }

    #[test]
    fn element_ids_survive_parameter_resize_and_hole_ids_never_remint() {
        let mut d1 = doc(vec![add(
            "board",
            Geom::Rect {
                x: 0.0,
                y: 0.0,
                w: 40.0,
                h: 30.0,
                r: 3.0,
                angle: 0.0,
            },
        )]);
        d1.holes.push(Hole {
            id: None,
            x: -15.0,
            y: -10.0,
            d: 3.2,
            plated: false,
            clearance: 0.5,
        });
        d1.holes.push(Hole {
            id: None,
            x: 15.0,
            y: 10.0,
            d: 3.2,
            plated: false,
            clearance: 0.5,
        });
        let PublishOutcome::Write { record: r1, .. } =
            prepare_publish(&mut d1, "resize_test", "rev1", "2026-11-03T12:00:00Z", None).unwrap()
        else {
            panic!()
        };
        let ids_before: std::collections::BTreeSet<_> = r1
            .outline
            .elements
            .iter()
            .map(|e| e.id.clone().unwrap())
            .collect();
        assert_eq!(d1.holes[0].id.as_deref(), Some("composer:hole/1"));
        assert_eq!(d1.holes[1].id.as_deref(), Some("composer:hole/2"));

        // Resize width/height/corner radius AND delete hole 1 + add a third hole: hole 2 keeps
        // `composer:hole/2`, retired `composer:hole/1` is NEVER reused (the new hole gets
        // `composer:hole/3`), and all 8 outline element IDs match!
        d1.features[0].geom = Geom::Rect {
            x: 5.0,
            y: -2.0,
            w: 55.0,
            h: 36.0,
            r: 5.0,
            angle: 0.0,
        };
        d1.holes.remove(0);
        d1.holes.push(Hole {
            id: None,
            x: 0.0,
            y: 0.0,
            d: 2.5,
            plated: true,
            clearance: 0.3,
        });
        let PublishOutcome::Write { record: r2, .. } = prepare_publish(
            &mut d1,
            "resize_test",
            "rev2",
            "2026-11-03T12:05:00Z",
            Some(&r1),
        )
        .unwrap() else {
            panic!()
        };
        let ids_after: std::collections::BTreeSet<_> = r2
            .outline
            .elements
            .iter()
            .map(|e| e.id.clone().unwrap())
            .collect();
        assert_eq!(ids_before, ids_after, "outline element IDs must survive resize");
        assert_eq!(d1.holes[0].id.as_deref(), Some("composer:hole/2"));
        assert_eq!(
            d1.holes[1].id.as_deref(),
            Some("composer:hole/3"),
            "retired composer:hole/1 must never be reused"
        );
    }

    #[test]
    fn regression_defect_1_corner_radius_tangent_junctions_safety_net_and_readback() {
        // 1a. Exact audit reproducer: tilted slot with corner_radius = 1.0 must emit zero bogus
        //     r=1 fillets on already-tangent slot-to-cap junctions, pass readback, and match area.
        let mut slot_cr = doc(vec![add(
            "f0",
            Geom::Slot {
                x1: 28.0,
                y1: 17.0,
                x2: 19.0,
                y2: 48.0,
                w: 5.0,
            },
        )]);
        slot_cr.corner_radius = 1.0;
        let poly_slot_area = evaluate(&slot_cr).area_mm2;
        let rec_slot = publish_and_round_trip(slot_cr, "audit_slot_cr1");
        assert!(!rec_slot.manifest.approximate);
        assert_eq!(rec_slot.outline.elements.len(), 4, "{:?}", rec_slot.outline.elements);
        for el in &rec_slot.outline.elements {
            assert!(
                !el.id.as_deref().unwrap_or("").contains("fillet"),
                "no fillet should be synthesized on an already-tangent slot: {el:?}"
            );
            if let ElementKind::Arc {
                center,
                start,
                end,
                angle_increasing,
            } = el.kind
            {
                let sweep = curves::arc_signed_sweep(center, start, end, angle_increasing).abs();
                assert!(
                    (sweep - std::f64::consts::PI).abs() < 0.05,
                    "expected 180-deg slot cap arc, got sweep={sweep}"
                );
            }
        }
        assert!(
            (loop_area_mm2(&rec_slot.outline) - poly_slot_area).abs() < 0.15,
            "curve area {} vs polygon area {poly_slot_area}",
            loop_area_mm2(&rec_slot.outline)
        );

        // 1b. Rotated rounded Rect with corner_radius = 1.0 (fuzz c286: previously self-intersected
        //     from a 360-deg tangent-junction fillet).
        let mut rect_rot_cr = doc(vec![add(
            "f0",
            Geom::Rect {
                x: 29.198,
                y: 35.716,
                w: 24.193,
                h: 15.336,
                r: 4.869,
                angle: 30.0,
            },
        )]);
        rect_rot_cr.corner_radius = 1.0;
        let poly_r_area = evaluate(&rect_rot_cr).area_mm2;
        let rec_r = publish_and_round_trip(rect_rot_cr, "audit_c286");
        assert!(
            (loop_area_mm2(&rec_r.outline) - poly_r_area).abs() < 0.25,
            "curve area {} vs polygon area {poly_r_area}",
            loop_area_mm2(&rec_r.outline)
        );

        // 1c. Polygon + 45-deg rounded Rect with corner_radius = 0.5 (fuzz_cr_rect c60: previously
        //     wrote a degenerate zero/full-circle sweep ARC that board_interface::read rejected).
        let mut c60 = doc(vec![
            add(
                "f0",
                Geom::Polygon {
                    points: vec![
                        [9.351, 49.251],
                        [7.576, 43.228],
                        [-1.37, 43.935],
                        [12.966, 18.768],
                        [14.824, 37.135],
                    ],
                    approximate: false,
                },
            ),
            add(
                "f1",
                Geom::Rect {
                    x: 16.0,
                    y: 19.0,
                    w: 40.0,
                    h: 20.0,
                    r: 1.0,
                    angle: 45.0,
                },
            ),
        ]);
        c60.corner_radius = 0.5;
        let poly_c60_area = evaluate(&c60).area_mm2;
        let rec_c60 = publish_and_round_trip(c60, "audit_c60");
        assert!(
            (loop_area_mm2(&rec_c60.outline) - poly_c60_area).abs() < 0.30,
            "c60 curve area {} vs polygon area {poly_c60_area}",
            loop_area_mm2(&rec_c60.outline)
        );
    }

    #[test]
    fn regression_defect_2_retired_hole_and_keepout_ids_never_reused_across_restarts() {
        let mut d = doc(vec![add(
            "board",
            Geom::Rect {
                x: 0.0,
                y: 0.0,
                w: 50.0,
                h: 40.0,
                r: 2.0,
                angle: 0.0,
            },
        )]);
        d.holes.push(Hole {
            id: None,
            x: -15.0,
            y: -10.0,
            d: 3.0,
            plated: false,
            clearance: 0.0,
        });
        d.holes.push(Hole {
            id: None,
            x: 15.0,
            y: -10.0,
            d: 3.0,
            plated: false,
            clearance: 0.0,
        });
        d.keepouts.push(Keepout {
            id: None,
            name: "antenna".into(),
            geom: Geom::Rect {
                x: -10.0,
                y: 8.0,
                w: 6.0,
                h: 4.0,
                r: 0.0,
                angle: 0.0,
            },
            side: Side::Front,
            max_height: Some(1.5),
        });

        let PublishOutcome::Write { record: r1, .. } =
            prepare_publish(&mut d, "id_retire", "s1", "2026-11-04T00:00:00Z", None).unwrap()
        else {
            panic!()
        };
        assert_eq!(d.holes[0].id.as_deref(), Some("composer:hole/1"));
        assert_eq!(d.holes[1].id.as_deref(), Some("composer:hole/2"));
        assert_eq!(d.keepouts[0].id.as_deref(), Some("composer:keepout/antenna"));

        // Simulate saving `shape.json` and reloading in a fresh process: delete `hole/1` and
        // `antenna`, then add a new hole and a new keep-out named `antenna` elsewhere.
        let saved_json = serde_json::to_string_pretty(&d).unwrap();
        let mut d2: ShapeDoc = serde_json::from_str(&saved_json).unwrap();
        d2.holes.remove(0);
        d2.holes.push(Hole {
            id: None,
            x: 0.0,
            y: 12.0,
            d: 3.0,
            plated: false,
            clearance: 0.0,
        });
        d2.keepouts.clear();
        d2.keepouts.push(Keepout {
            id: None,
            name: "antenna".into(),
            geom: Geom::Rect {
                x: 12.0,
                y: 8.0,
                w: 5.0,
                h: 5.0,
                r: 0.0,
                angle: 0.0,
            },
            side: Side::Back,
            max_height: None,
        });

        let PublishOutcome::Write { record: r2, .. } =
            prepare_publish(&mut d2, "id_retire", "s2", "2026-11-04T00:05:00Z", Some(&r1)).unwrap()
        else {
            panic!()
        };
        assert_eq!(d2.holes[0].id.as_deref(), Some("composer:hole/2"));
        assert_eq!(d2.holes[1].id.as_deref(), Some("composer:hole/3"));
        assert_eq!(d2.keepouts[0].id.as_deref(), Some("composer:keepout/1"));

        let cmp = board_interface::compare(&r1, &r2);
        assert!(
            !cmp.changes
                .iter()
                .any(|c| c.id.as_deref() == Some("composer:hole/1")
                    && matches!(c.kind, board_interface::ChangeKind::Moved { .. })),
            "deleted composer:hole/1 must appear Removed, never Moved: {:?}",
            cmp.changes
        );
    }

    #[test]
    fn regression_defect_3_and_4_handover_back_to_own_folder_and_followed_checks() {
        // 1. Composer publishes A to its own folder.
        let mut comp_doc = doc(vec![add(
            "plate",
            Geom::Rect {
                x: 0.0,
                y: 0.0,
                w: 30.0,
                h: 20.0,
                r: 2.0,
                angle: 0.0,
            },
        )]);
        let PublishOutcome::Write { record: rec_a, .. } =
            prepare_publish(&mut comp_doc, "handback", "c1", "2026-11-04T01:00:00Z", None).unwrap()
        else {
            panic!()
        };

        // 2. Waffle takes over from A and publishes W1 (parent = A), then edits again to W2
        //    (parent = W1, last_handover.from_revision = A).
        let mut rec_w1 = (*rec_a).clone();
        rec_w1.manifest.parent = Some(rec_a.manifest.revision.clone());
        rec_w1.manifest.driver = board_interface::Driver::Waffle;
        rec_w1.manifest.author = board_interface::Author {
            app: board_interface::Driver::Waffle,
            source: "w1".into(),
            at: "2026-11-04T01:05:00Z".into(),
        };
        rec_w1.manifest.last_handover = Some(board_interface::Handover {
            by: board_interface::Driver::Waffle,
            at: "2026-11-04T01:05:00Z".into(),
            from_revision: rec_a.manifest.revision.clone(),
        });
        rec_w1.holes.push(board_interface::Hole {
            id: Some("waffle:hole/1".into()),
            center: board_interface::Point::new(0, 0),
            diameter_nm: 3_000_000,
            plated: false,
            clearance_nm: 0,
        });
        let gh_w1 = hash(&rec_w1);
        rec_w1.manifest.geometry_hash = gh_w1.clone();
        rec_w1.manifest.revision = gh_w1;

        let mut rec_w2 = rec_w1.clone();
        rec_w2.holes[0].center.x = 2_000_000;
        rec_w2.manifest.parent = Some(rec_w1.manifest.revision.clone());
        rec_w2.manifest.author.at = "2026-11-04T01:06:00Z".into();
        let gh_w2 = hash(&rec_w2);
        rec_w2.manifest.geometry_hash = gh_w2.clone();
        rec_w2.manifest.revision = gh_w2;

        // 3. Composer pulls W1, takes over, and BEFORE publishing, Waffle advances to W2:
        //    publishing with FollowedState::Readable(&rec_w2) MUST refuse ("base moved")!
        pull(&mut comp_doc, &rec_w1, Some("/virtual/waffle".into()));
        take_over(&mut comp_doc).unwrap();
        let err_stale = prepare_publish_with_followed(
            &mut comp_doc,
            "handback",
            "c2",
            "2026-11-04T01:07:00Z",
            Some(&rec_a),
            FollowedState::Readable(&rec_w2),
            false,
        )
        .unwrap_err();
        assert!(err_stale.contains("base moved"), "{err_stale}");
        assert!(err_stale.contains("followed interface"), "{err_stale}");

        // 4. Now Composer pulls W2 (whose parent is W1, and whose last_handover.from_revision is A!),
        //    takes over, and publishes back to Composer's own folder still holding A.
        pull(&mut comp_doc, &rec_w2, Some("/virtual/waffle".into()));
        take_over(&mut comp_doc).unwrap();

        // Refuses an unrelated on-disk record with instructions on what to do:
        let mut unrelated = (*rec_a).clone();
        unrelated.manifest.revision = "sha256:0000000000000000000000000000000000000000000000000000000000000000".into();
        let err_unrel = prepare_publish_with_followed(
            &mut comp_doc,
            "handback",
            "c2",
            "2026-11-04T01:08:00Z",
            Some(&unrelated),
            FollowedState::Readable(&rec_w2),
            false,
        )
        .unwrap_err();
        assert!(err_unrel.contains("base moved") && err_unrel.contains("ccomp shape pull"), "{err_unrel}");

        // Accepts on-disk A (ancestor via last_handover.from_revision) and warns if followed is unreadable:
        let PublishOutcome::Write {
            record: rec_c,
            warnings,
            ..
        } = prepare_publish_with_followed(
            &mut comp_doc,
            "handback",
            "c2",
            "2026-11-04T01:09:00Z",
            Some(&rec_a),
            FollowedState::Unreadable {
                path: "/virtual/waffle".into(),
                error: "ENOENT".into(),
            },
            false,
        )
        .unwrap()
        else {
            panic!()
        };
        assert!(
            warnings.iter().any(|w| w.contains("/virtual/waffle") && w.contains("could not be read")),
            "expected unreadable followed warning, got {warnings:?}"
        );
        assert_eq!(rec_c.manifest.parent.as_deref(), Some(rec_w2.manifest.revision.as_str()));
        assert_eq!(
            board_interface::compare(&rec_w2, &rec_c).classification,
            board_interface::Classification::CleanHandover
        );
    }

    fn find_golden_dir() -> PathBuf {
        if let Ok(p) = std::env::var("BOARD_INTERFACE_GOLDEN") {
            let pb = PathBuf::from(p);
            if pb.join("easy2350.json").is_file() {
                return pb;
            }
        }
        let cargo_home = std::env::var_os("CARGO_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cargo")));
        if let Some(ch) = cargo_home {
            if let Ok(checkouts) = std::fs::read_dir(ch.join("git/checkouts")) {
                for entry in checkouts.flatten() {
                    if entry
                        .file_name()
                        .to_string_lossy()
                        .starts_with("board-interface-")
                    {
                        if let Ok(revs) = std::fs::read_dir(entry.path()) {
                            for rev in revs.flatten() {
                                let g = rev.path().join("golden");
                                if g.join("easy2350.json").is_file() {
                                    return g;
                                }
                            }
                        }
                    }
                }
            }
        }
        for rel in ["../../../board-interface/golden", "/home/sequoia/Software/board-interface/golden"] {
            let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(rel);
            if p.join("easy2350.json").is_file() {
                return p;
            }
        }
        panic!("could not locate board-interface golden fixtures");
    }

    #[test]
    fn all_three_valid_goldens_pull_take_over_and_republish_with_identical_hash() {
        let golden_dir = find_golden_dir();
        for stem in ["easy2350", "rounded_rect", "round_board"] {
            let dxf_in = std::fs::read(golden_dir.join(format!("{stem}.dxf"))).expect(stem);
            let json_in = std::fs::read(golden_dir.join(format!("{stem}.json"))).expect(stem);
            let golden = read(&dxf_in, &json_in).expect(stem);
            let want_hash = hash(&golden);
            assert_eq!(want_hash, golden.manifest.geometry_hash);

            // Start from an existing Composer recipe (with a generator) to verify set-aside too!
            let mut d = doc(vec![add(
                "old_bolt",
                Geom::Generator {
                    name: "bolt".into(),
                    params: BTreeMap::new(),
                },
            )]);
            let pull_out = pull(&mut d, &golden, None);
            assert!(!pull_out.warnings.is_empty(), "{stem} pull should warn about set-aside recipe");
            assert!(is_locked(&d), "{stem} must be locked right after pull");

            //Sync polygon evaluation works for the floorplanner without Clipper2:
            let poly = evaluate(&d);
            assert_eq!(poly.outline.len(), 1);
            assert_eq!(poly.cutouts.len(), golden.cutouts.len());
            assert_eq!(poly.holes.len(), golden.holes.len());

            // Take over:
            let to_out = take_over(&mut d).expect("take_over succeeds");
            assert!(!to_out.warnings.is_empty());
            assert!(!is_locked(&d), "{stem} must be unlocked after take_over");

            // Publish:
            let on_disk = if golden.manifest.driver != board_interface::Driver::Composer {
                Some(&golden)
            } else {
                None
            };
            let pub_out = prepare_publish(
                &mut d,
                &golden.manifest.board,
                "composer-commit-1",
                "2026-11-03T15:00:00Z",
                on_disk,
            )
            .expect("publish after take_over succeeds");
            let PublishOutcome::Write {
                dxf,
                manifest_json,
                record: _,
                ..
            } = pub_out
            else {
                panic!("{stem}: expected Write");
            };
            let read_back = read(&dxf, &manifest_json).expect("read back succeeds");

            assert_eq!(
                hash(&read_back),
                want_hash,
                "{stem}: geometry hash must be 100% identical after pull -> take_over -> publish"
            );
            assert_eq!(read_back.manifest.geometry_hash, want_hash);

            // Arcs/Circles remain exact Arcs/Circles:
            let want_arcs = golden
                .outline
                .elements
                .iter()
                .filter(|e| matches!(e.kind, ElementKind::Arc { .. }))
                .count();
            let got_arcs = read_back
                .outline
                .elements
                .iter()
                .filter(|e| matches!(e.kind, ElementKind::Arc { .. }))
                .count();
            assert_eq!(got_arcs, want_arcs, "{stem} arc count");

            let want_circles = golden
                .outline
                .elements
                .iter()
                .filter(|e| matches!(e.kind, ElementKind::Circle { .. }))
                .count();
            let got_circles = read_back
                .outline
                .elements
                .iter()
                .filter(|e| matches!(e.kind, ElementKind::Circle { .. }))
                .count();
            assert_eq!(got_circles, want_circles, "{stem} circle count");

            if golden.manifest.driver != board_interface::Driver::Composer {
                let cmp = board_interface::compare(&golden, &read_back);
                assert_eq!(
                    cmp.classification,
                    board_interface::Classification::CleanHandover,
                    "{stem} must classify as CleanHandover"
                );
                assert!(cmp.changes.is_empty(), "{stem} must have zero per-ID changes");
            }
        }
    }
}
