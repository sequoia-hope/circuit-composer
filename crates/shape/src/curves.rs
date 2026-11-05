//! Curve-preserving shape evaluation for `board-interface`.
//!
//! `evaluate` in `lib.rs` produces polygon outlines through Clipper2 for the floorplanner and
//! stays untouched for existing documents. Alongside it, [`evaluate_curves`] walks the Clipper2
//! boolean output against the exact analytical primitives of each input feature (`Circle`, `Slot`,
//! `Rect` corners, imported DXF `Path`s, `Symmetry` images, and post-boolean `corner_radius`
//! tangent fillets), snapping intersections to exact whole nanometres so consecutive `Line`/`Arc`
//! elements share `0 nm` endpoints and pass `board-interface`'s concentricity invariants.

use std::collections::{BTreeMap, BTreeSet};
use std::f64::consts::{PI, TAU};

use board_interface::{
    Element, ElementKind, Hole as BiHole, Keepout as BiKeepout, Loop, NM_PER_MM, Point,
    Side as BiSide,
};
use clipper2_rust::{difference_64, union_64, FillRule, Path64, Paths64};

use crate::{
    from_mm, geom, round, signed_area, symmetric, to_mm, ARC_TOLERANCE, Feature, Geom, Op, P,
    SCALE, ShapeDoc, Side, Symmetry, SymmetryKind,
};

/// Evaluated curve boundary, holes and keep-outs in `board-interface`'s nanometre frame.
#[derive(Debug, Clone)]
pub struct CurveEval {
    /// Closed outer loops (largest area first). Publishing requires `outline.len() == 1`.
    pub outline: Vec<Loop>,
    /// Closed cutout loops strictly inside the board.
    pub cutouts: Vec<Loop>,
    pub holes: Vec<BiHole>,
    pub keepouts: Vec<BiKeepout>,
    /// `true` iff a curve source (such as a parametric generator or flattened SVG curve) became
    /// straight chord lines in the output.
    pub approximate: bool,
    pub warnings: Vec<String>,
}

#[inline]
pub fn mm_to_nm(v: f64) -> i64 {
    (v * (NM_PER_MM as f64)).round() as i64
}

#[inline]
pub fn nm_to_mm(v: i64) -> f64 {
    (v as f64) / (NM_PER_MM as f64)
}

#[inline]
pub fn side_to_bi(side: Side) -> BiSide {
    match side {
        Side::Both => BiSide::Both,
        Side::Front => BiSide::Front,
        Side::Back => BiSide::Back,
    }
}

#[inline]
pub fn side_from_bi(side: BiSide) -> Side {
    match side {
        BiSide::Both => Side::Both,
        BiSide::Front => Side::Front,
        BiSide::Back => Side::Back,
    }
}

/// Positive angle sweep `(a1 - a0) mod 2pi` in `[0, 2pi)`.
#[inline]
fn pos_sweep(a0: f64, a1: f64) -> f64 {
    let d = (a1 - a0).rem_euclid(TAU);
    if d < 1e-12 { TAU } else { d }
}

/// Signed angle sweep of an `Arc` in KiCad's Y-down frame (`> 0` when `angle_increasing`).
pub fn arc_signed_sweep(center: Point, start: Point, end: Point, angle_increasing: bool) -> f64 {
    let a0 = ((start.y - center.y) as f64).atan2((start.x - center.x) as f64);
    let a1 = ((end.y - center.y) as f64).atan2((end.x - center.x) as f64);
    if angle_increasing {
        pos_sweep(a0, a1)
    } else {
        -pos_sweep(a1, a0)
    }
}

/// Exact analytical area of a closed `Loop` in `mm^2` (always non-negative).
pub fn loop_area_mm2(lp: &Loop) -> f64 {
    if lp.elements.len() == 1 {
        if let ElementKind::Circle { diameter_nm, .. } = lp.elements[0].kind {
            let r = nm_to_mm(diameter_nm) * 0.5;
            return PI * r * r;
        }
    }
    let mut signed = 0.0;
    for el in &lp.elements {
        match el.kind {
            ElementKind::Line { start, end } => {
                let (x0, y0) = (nm_to_mm(start.x), nm_to_mm(start.y));
                let (x1, y1) = (nm_to_mm(end.x), nm_to_mm(end.y));
                signed += 0.5 * (x0 * y1 - x1 * y0);
            }
            ElementKind::Arc {
                center,
                start,
                end,
                angle_increasing,
            } => {
                let (x0, y0) = (nm_to_mm(start.x), nm_to_mm(start.y));
                let (x1, y1) = (nm_to_mm(end.x), nm_to_mm(end.y));
                signed += 0.5 * (x0 * y1 - x1 * y0);
                let (cx, cy) = (nm_to_mm(center.x), nm_to_mm(center.y));
                let r = (x0 - cx).hypot(y0 - cy);
                let delta = arc_signed_sweep(center, start, end, angle_increasing);
                signed += 0.5 * r * r * (delta - delta.sin());
            }
            ElementKind::Circle { diameter_nm, .. } => {
                let r = nm_to_mm(diameter_nm) * 0.5;
                return PI * r * r;
            }
        }
    }
    signed.abs()
}

/// Exact analytical perimeter of a closed `Loop` in `mm`.
pub fn loop_perimeter_mm(lp: &Loop) -> f64 {
    let mut total = 0.0;
    for el in &lp.elements {
        match el.kind {
            ElementKind::Line { start, end } => {
                total += nm_to_mm(end.x - start.x).hypot(nm_to_mm(end.y - start.y));
            }
            ElementKind::Arc {
                center,
                start,
                end,
                angle_increasing,
            } => {
                let r = nm_to_mm(start.x - center.x).hypot(nm_to_mm(start.y - center.y));
                let delta = arc_signed_sweep(center, start, end, angle_increasing).abs();
                total += r * delta;
            }
            ElementKind::Circle { diameter_nm, .. } => {
                total += PI * nm_to_mm(diameter_nm);
            }
        }
    }
    total
}

/// Polygonize a `Loop` into board-mm points within `arc_tol_mm` (`ARC_TOLERANCE`).
///
/// When `want_positive_area` is `Some(true)`, vertices wind clockwise on screen (`signed_area > 0`,
/// matching Clipper2 outer loops); when `Some(false)`, counter-clockwise (`signed_area < 0`,
/// matching Clipper2 cutouts).
pub fn loop_to_polygon_mm(lp: &Loop, arc_tol_mm: f64, want_positive_area: Option<bool>) -> Vec<P> {
    let tol = arc_tol_mm.max(1e-4);
    let mut pts = Vec::new();
    if lp.elements.len() == 1 {
        if let ElementKind::Circle {
            center,
            diameter_nm,
        } = lp.elements[0].kind
        {
            let (cx, cy) = (nm_to_mm(center.x), nm_to_mm(center.y));
            let r = nm_to_mm(diameter_nm) * 0.5;
            if r <= 0.0 {
                return pts;
            }
            let n =
                ((PI / (1.0 - tol / r).clamp(-1.0, 1.0).acos()).ceil() as usize).clamp(12, 720);
            let sign = if want_positive_area.unwrap_or(true) {
                1.0
            } else {
                -1.0
            };
            for i in 0..n {
                let a = sign * TAU * (i as f64) / (n as f64);
                pts.push([cx + r * a.cos(), cy + r * a.sin()]);
            }
            return pts;
        }
    }

    for el in &lp.elements {
        match el.kind {
            ElementKind::Line { start, .. } => {
                pts.push([nm_to_mm(start.x), nm_to_mm(start.y)]);
            }
            ElementKind::Arc {
                center,
                start,
                end,
                angle_increasing,
            } => {
                let (cx, cy) = (nm_to_mm(center.x), nm_to_mm(center.y));
                let (sx, sy) = (nm_to_mm(start.x), nm_to_mm(start.y));
                pts.push([sx, sy]);
                let r = (sx - cx).hypot(sy - cy);
                if r > 1e-9 {
                    let a0 = (sy - cy).atan2(sx - cx);
                    let delta = arc_signed_sweep(center, start, end, angle_increasing);
                    let half_step = (1.0 - (tol / r).min(1.0)).clamp(-1.0, 1.0).acos();
                    let max_step = (2.0 * half_step).max(1e-3);
                    let steps = ((delta.abs() / max_step).ceil() as usize).clamp(1, 720);
                    for k in 1..steps {
                        let a = a0 + delta * (k as f64) / (steps as f64);
                        pts.push([cx + r * a.cos(), cy + r * a.sin()]);
                    }
                }
            }
            ElementKind::Circle { .. } => {}
        }
    }

    if let Some(want_pos) = want_positive_area {
        if pts.len() >= 3 {
            let area = poly_signed_area_mm(&pts);
            if area.abs() > 1e-12 && (area > 0.0) != want_pos {
                pts.reverse();
            }
        }
    }
    pts
}

pub(crate) fn poly_signed_area_mm(pts: &[P]) -> f64 {
    let mut a = 0.0;
    for i in 0..pts.len() {
        let (u, v) = (pts[i], pts[(i + 1) % pts.len()]);
        a += u[0] * v[1] - v[0] * u[1];
    }
    0.5 * a
}

fn point_in_poly_mm(q: P, poly: &[P]) -> bool {
    let mut inside = false;
    let n = poly.len();
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        if (a[1] > q[1]) != (b[1] > q[1]) {
            let x_cross = a[0] + (q[1] - a[1]) * (b[0] - a[0]) / (b[1] - a[1]);
            if q[0] < x_cross {
                inside = !inside;
            }
        }
    }
    inside
}

fn point_seg_dist_mm(q: P, a: P, b: P) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let l2 = dx * dx + dy * dy;
    let t = if l2 > 0.0 {
        (((q[0] - a[0]) * dx + (q[1] - a[1]) * dy) / l2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (q[0] - (a[0] + t * dx)).hypot(q[1] - (a[1] + t * dy))
}

fn min_poly_clearance_mm(a: &[P], b: &[P]) -> f64 {
    let mut best = f64::INFINITY;
    for &p in a {
        for j in 0..b.len() {
            best = best.min(point_seg_dist_mm(p, b[j], b[(j + 1) % b.len()]));
        }
    }
    for &p in b {
        for i in 0..a.len() {
            best = best.min(point_seg_dist_mm(p, a[i], a[(i + 1) % a.len()]));
        }
    }
    best
}

/// Evaluate `doc` into exact `Line`/`Arc`/`Circle` loops, holes and keep-outs.
pub fn evaluate_curves(doc: &ShapeDoc) -> CurveEval {
    // 1. Synced geometry never enters Clipper2.
    if let Some(synced) = doc.sync.as_ref().and_then(|s| s.synced.as_ref()) {
        let mut warnings = Vec::new();
        for h in &synced.holes {
            if h.diameter_nm <= 0 {
                warnings.push(format!(
                    "a hole at ({}, {}) has no diameter",
                    nm_to_mm(h.center.x),
                    nm_to_mm(h.center.y)
                ));
            }
        }
        return CurveEval {
            outline: vec![synced.outline.clone()],
            cutouts: synced.cutouts.clone(),
            holes: synced.holes.clone(),
            keepouts: synced.keepouts.clone(),
            approximate: synced.approximate,
            warnings,
        };
    }

    let mut warnings = Vec::new();
    let mut holes = build_holes(doc, &mut warnings);
    let mut keepouts_approx = false;
    let mut keepouts = build_keepouts(doc, &mut keepouts_approx, &mut warnings);

    // 2. Fast exact path for untouched / non-overlapping `Geom::Path` loops (e.g. after `take_over`
    //    before boolean edits, preserving every nanometre coordinate and original ID verbatim).
    if let Some((mut outline, mut cutouts, path_approx)) = try_direct_path_recipe(doc) {
        ensure_unique_ids(&mut outline, &mut cutouts, &mut holes, &mut keepouts);
        return CurveEval {
            outline,
            cutouts,
            holes,
            keepouts,
            approximate: keepouts_approx || path_approx,
            warnings,
        };
    }

    // 3. General Composer-authored recipe through Clipper2 + analytical source primitive matching.
    let mut acc = Paths64::new();
    let mut prims: Vec<SourcePrim> = Vec::new();
    let mut has_generator = false;
    let mut feat_name_counts: BTreeMap<String, usize> = BTreeMap::new();

    for f in &doc.features {
        if f.hidden {
            continue;
        }
        let g = match geom(&f.geom) {
            Ok(g) => g,
            Err(e) => {
                warnings.push(format!("{}: {e}", f.name));
                continue;
            }
        };
        let slug = slugify(&f.name);
        let cnt = feat_name_counts.entry(slug.clone()).or_insert(0);
        *cnt += 1;
        let feat_prefix = if *cnt == 1 {
            format!("composer:{slug}")
        } else {
            format!("composer:{slug}-{}", *cnt)
        };

        if matches!(f.geom, Geom::Generator { .. }) {
            has_generator = true;
        } else {
            collect_geom_prims(&f.geom, &feat_prefix, &mut prims);
        }

        acc = match f.op {
            Op::Add => union_64(&acc, &g, FillRule::NonZero),
            Op::Subtract => difference_64(&acc, &g, FillRule::NonZero),
        };
    }

    acc = symmetric(&acc, &doc.symmetry);
    if doc.symmetry.kind != SymmetryKind::None {
        let sym_prims: Vec<SourcePrim> = prims
            .iter()
            .map(|p| symmetric_prim(p, &doc.symmetry))
            .collect();
        prims.extend(sym_prims);
    }

    let mut approximate = keepouts_approx || has_generator;

    let final_acc = if doc.corner_radius > 0.0 {
        let rounded = round(&acc, doc.corner_radius);
        if !has_generator {
            // Reconstruct pre-round exact curves so analytical radius-`corner_radius` fillets can
            // be added to `prims` before walking `rounded`.
            let rounded_mm = to_mm(&rounded);
            let mut pre_approx = false;
            let pre_loops: Vec<ReconstructedLoop> = acc
                .iter()
                .filter(|p| signed_area(p).abs() > 0.0)
                .map(|p| reconstruct_loop_segments(p, &prims, false, false, &mut pre_approx))
                .collect();
            for rl in &pre_loops {
                generate_fillet_prims(&rl.segs, doc.corner_radius, &rounded_mm, &mut prims);
            }
        }
        rounded
    } else {
        acc
    };

    let mut outer_paths: Vec<Path64> = final_acc
        .iter()
        .filter(|p| signed_area(p) > 0.0)
        .cloned()
        .collect();
    let inner_paths: Vec<Path64> = final_acc
        .iter()
        .filter(|p| signed_area(p) < 0.0)
        .cloned()
        .collect();
    outer_paths.sort_by(|a, b| signed_area(b).total_cmp(&signed_area(a)));

    if outer_paths.len() > 1 {
        warnings.push(format!(
            "the outline is in {} separate pieces",
            outer_paths.len()
        ));
    }
    if outer_paths.is_empty() && !doc.features.is_empty() {
        warnings.push("nothing is left of the outline".into());
    }

    let force_lines = has_generator;
    let post_round = doc.corner_radius > 0.0;
    let mut outline: Vec<Loop> = outer_paths
        .iter()
        .enumerate()
        .map(|(idx, p)| {
            let fallback_stem = if idx == 0 {
                "composer:outline".to_string()
            } else {
                format!("composer:outline-{}", idx + 1)
            };
            build_loop_from_path(
                p,
                &prims,
                force_lines,
                post_round,
                &fallback_stem,
                &mut approximate,
            )
        })
        .collect();

    let mut cutouts: Vec<Loop> = inner_paths
        .iter()
        .enumerate()
        .map(|(idx, p)| {
            let fallback_stem = format!("composer:cutout-{}", idx + 1);
            build_loop_from_path(
                p,
                &prims,
                force_lines,
                post_round,
                &fallback_stem,
                &mut approximate,
            )
        })
        .collect();

    ensure_unique_ids(&mut outline, &mut cutouts, &mut holes, &mut keepouts);

    CurveEval {
        outline,
        cutouts,
        holes,
        keepouts,
        approximate,
        warnings,
    }
}

/// If `doc` consists solely of a single `Add` `Geom::Path` outline and zero or more `Subtract`
/// `Geom::Path` cutouts strictly inside it (with no `corner_radius` or `symmetry`), return those
/// loops directly without Clipper2.
fn try_direct_path_recipe(doc: &ShapeDoc) -> Option<(Vec<Loop>, Vec<Loop>, bool)> {
    if doc.corner_radius > 0.0 || doc.symmetry.kind != SymmetryKind::None {
        return None;
    }
    let active: Vec<&Feature> = doc.features.iter().filter(|f| !f.hidden).collect();
    if active.is_empty() {
        return None;
    }
    let Geom::Path {
        elements: outer_elems,
        approximate: outer_approx,
    } = &active[0].geom
    else {
        return None;
    };
    if active[0].op != Op::Add || outer_elems.is_empty() {
        return None;
    }
    let mut approx = *outer_approx;
    let mut sub_loops = Vec::new();
    for f in &active[1..] {
        if f.op != Op::Subtract {
            return None;
        }
        let Geom::Path {
            elements,
            approximate: sub_approx,
        } = &f.geom
        else {
            return None;
        };
        if elements.is_empty() {
            return None;
        }
        approx |= *sub_approx;
        sub_loops.push(Loop::new(elements.clone()));
    }

    let outer_loop = Loop::new(outer_elems.clone());
    if !sub_loops.is_empty() {
        let outer_poly = loop_to_polygon_mm(&outer_loop, ARC_TOLERANCE, Some(true));
        if outer_poly.len() < 3 {
            return None;
        }
        let sub_polys: Vec<Vec<P>> = sub_loops
            .iter()
            .map(|lp| loop_to_polygon_mm(lp, ARC_TOLERANCE, Some(false)))
            .collect();
        for (i, sp) in sub_polys.iter().enumerate() {
            if sp.len() < 3 {
                return None;
            }
            // Every cutout vertex must lie strictly inside `outer_poly` with > 2 um clearance.
            if !sp.iter().all(|&pt| point_in_poly_mm(pt, &outer_poly))
                || min_poly_clearance_mm(sp, &outer_poly) <= 0.002
            {
                return None;
            }
            // Cutouts must be pairwise non-nested and non-touching.
            for (j, other) in sub_polys.iter().enumerate() {
                if i != j
                    && (point_in_poly_mm(sp[0], other)
                        || (j > i && min_poly_clearance_mm(sp, other) <= 0.002))
                {
                    return None;
                }
            }
        }
    }

    // Fill in deterministic IDs for any element that arrived with `id: None`.
    let mut outer_loop = outer_loop;
    let out_slug = slugify(&active[0].name);
    for (k, el) in outer_loop.elements.iter_mut().enumerate() {
        if el.id.is_none() {
            el.id = Some(format!("composer:{out_slug}/seg{k}"));
        }
    }
    for (ci, lp) in sub_loops.iter_mut().enumerate() {
        let c_slug = slugify(&active[ci + 1].name);
        for (k, el) in lp.elements.iter_mut().enumerate() {
            if el.id.is_none() {
                el.id = Some(format!("composer:{c_slug}/seg{k}"));
            }
        }
    }

    Some((vec![outer_loop], sub_loops, approx))
}

fn build_holes(doc: &ShapeDoc, warnings: &mut Vec<String>) -> Vec<BiHole> {
    let mut out = Vec::new();
    let mut used = doc.minted_ids.clone();
    let mut next_n = doc.next_hole_id;
    for h in &doc.holes {
        if let Some(id) = &h.id {
            used.insert(id.clone());
            if let Some(n) = id.strip_prefix("composer:hole/").and_then(|s| s.parse::<u64>().ok()) {
                next_n = next_n.max(n);
            }
        }
    }
    for h in &doc.holes {
        if h.d <= 0.0 || mm_to_nm(h.d) <= 0 {
            warnings.push(format!("a hole at ({}, {}) has no diameter", h.x, h.y));
            continue;
        }
        let id = match h.id.as_ref().filter(|s| !s.trim().is_empty()) {
            Some(existing) => existing.clone(),
            None => loop {
                next_n += 1;
                let cand = format!("composer:hole/{next_n}");
                if used.insert(cand.clone()) {
                    break cand;
                }
            },
        };
        out.push(BiHole {
            id: Some(id),
            center: Point::new(mm_to_nm(h.x), mm_to_nm(h.y)),
            diameter_nm: mm_to_nm(h.d),
            plated: h.plated,
            clearance_nm: mm_to_nm(h.clearance.max(0.0)),
        });
    }
    out
}

fn build_keepouts(
    doc: &ShapeDoc,
    approximate: &mut bool,
    warnings: &mut Vec<String>,
) -> Vec<BiKeepout> {
    let mut out = Vec::new();
    let mut used = doc.minted_ids.clone();
    let mut next_n = doc.next_keepout_id;
    for ko in &doc.keepouts {
        if let Some(id) = &ko.id {
            used.insert(id.clone());
            if let Some(n) = id
                .strip_prefix("composer:keepout/")
                .and_then(|s| s.parse::<u64>().ok())
            {
                next_n = next_n.max(n);
            }
        }
    }
    for ko in &doc.keepouts {
        let base_id = match ko.id.as_ref().filter(|s| !s.trim().is_empty()) {
            Some(existing) => existing.clone(),
            None => {
                let slug = slugify(&ko.name);
                let preferred = format!("composer:keepout/{slug}");
                if !slug.is_empty() && slug != "feature" && used.insert(preferred.clone()) {
                    preferred
                } else {
                    loop {
                        next_n += 1;
                        let cand = format!("composer:keepout/{next_n}");
                        if used.insert(cand.clone()) {
                            break cand;
                        }
                    }
                }
            }
        };
        let max_height_nm = ko.max_height.map(|h| mm_to_nm(h.max(0.0)));
        let side = side_to_bi(ko.side);

        if let Geom::Path {
            elements,
            approximate: path_approx,
        } = &ko.geom
        {
            if !elements.is_empty() {
                *approximate |= *path_approx;
                let mut elems = elements.clone();
                let elems_len = elems.len();
                for (k, el) in elems.iter_mut().enumerate() {
                    if el.id.is_none() {
                        el.id = Some(if elems_len == 1 {
                            base_id.clone()
                        } else {
                            format!("{base_id}/seg{k}")
                        });
                    }
                }
                out.push(BiKeepout {
                    boundary: Loop::new(elems),
                    side,
                    max_height_nm,
                });
                continue;
            }
        }

        let g = match geom(&ko.geom) {
            Ok(g) => g,
            Err(e) => {
                warnings.push(format!("keep-out {}: {e}", ko.name));
                continue;
            }
        };
        let is_gen = matches!(ko.geom, Geom::Generator { .. });
        if is_gen {
            *approximate = true;
        }
        let mut prims = Vec::new();
        if !is_gen {
            collect_geom_prims(&ko.geom, &base_id, &mut prims);
        }
        for (p_idx, path) in g.iter().filter(|p| signed_area(p) > 0.0).enumerate() {
            let stem = if p_idx == 0 {
                base_id.clone()
            } else {
                format!("{base_id}/p{}", p_idx + 1)
            };
            let lp = build_loop_from_path(path, &prims, is_gen, false, &stem, approximate);
            if !lp.elements.is_empty() {
                out.push(BiKeepout {
                    boundary: lp,
                    side,
                    max_height_nm,
                });
            }
        }
    }
    out
}

pub(crate) fn slugify(name: &str) -> String {
    let mut s = String::new();
    let mut prev_dash = false;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            s.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash && !s.is_empty() {
            s.push('-');
            prev_dash = true;
        }
    }
    while s.ends_with('-') {
        s.pop();
    }
    if s.is_empty() {
        "feature".to_string()
    } else {
        s
    }
}

#[derive(Debug, Clone)]
enum SourcePrim {
    Line(LinePrim),
    Circle(CirclePrim),
}

#[derive(Debug, Clone)]
struct LinePrim {
    id: String,
    a: P,
    b: P,
    ux: f64,
    uy: f64,
    len: f64,
    exact_nm: Option<(Point, Point)>,
    from_approx: bool,
}

#[derive(Debug, Clone)]
struct CirclePrim {
    id: String,
    cx: f64,
    cy: f64,
    r: f64,
    center_nm: Point,
    r_nm: f64,
    diameter_nm: i64,
    full_ok: bool,
    /// Optional `(start_angle_rad, positive_ccw_angle_span_rad)` in Y-down coordinates.
    arc_span: Option<(f64, f64)>,
    /// Original exact endpoints when sourced from `Geom::Path`.
    exact_arc: Option<(Point, Point, bool)>,
    /// Required `angle_increasing` direction when known (e.g. post-boolean fillets).
    expected_inc: Option<bool>,
}

impl LinePrim {
    fn new(id: String, a: P, b: P, exact_nm: Option<(Point, Point)>, from_approx: bool) -> Option<Self> {
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let len = dx.hypot(dy);
        if len < 1e-9 {
            return None;
        }
        Some(Self {
            id,
            a,
            b,
            ux: dx / len,
            uy: dy / len,
            len,
            exact_nm,
            from_approx,
        })
    }
}

fn collect_geom_prims(g: &Geom, prefix: &str, out: &mut Vec<SourcePrim>) {
    match g {
        Geom::Circle { x, y, d } => {
            if *d <= 0.0 {
                return;
            }
            let r = *d * 0.5;
            let center_nm = Point::new(mm_to_nm(*x), mm_to_nm(*y));
            let diameter_nm = mm_to_nm(*d);
            out.push(SourcePrim::Circle(CirclePrim {
                id: format!("{prefix}/circle"),
                cx: nm_to_mm(center_nm.x),
                cy: nm_to_mm(center_nm.y),
                r,
                center_nm,
                r_nm: (diameter_nm as f64) * 0.5,
                diameter_nm,
                full_ok: true,
                arc_span: None,
                exact_arc: None,
                expected_inc: None,
            }));
        }
        Geom::Rect {
            x,
            y,
            w,
            h,
            r,
            angle,
        } => {
            if *w <= 0.0 || *h <= 0.0 {
                return;
            }
            let r = r.clamp(0.0, w.min(*h) * 0.5);
            let (hw, hh) = (w * 0.5 - r, h * 0.5 - r);
            let (sn, cs) = angle.to_radians().sin_cos();
            let tr = |px: f64, py: f64| -> P { [x + px * cs + py * sn, y - px * sn + py * cs] };

            if r <= 1e-9 {
                let c0 = tr(-w * 0.5, -h * 0.5);
                let c1 = tr(w * 0.5, -h * 0.5);
                let c2 = tr(w * 0.5, h * 0.5);
                let c3 = tr(-w * 0.5, h * 0.5);
                for (name, a, b) in [
                    ("top", c0, c1),
                    ("right", c1, c2),
                    ("bottom", c2, c3),
                    ("left", c3, c0),
                ] {
                    if let Some(lp) = LinePrim::new(format!("{prefix}/{name}"), a, b, None, false) {
                        out.push(SourcePrim::Line(lp));
                    }
                }
                return;
            }

            if hw <= 1e-9 && hh <= 1e-9 {
                let center_nm = Point::new(mm_to_nm(*x), mm_to_nm(*y));
                let diameter_nm = mm_to_nm(2.0 * r);
                out.push(SourcePrim::Circle(CirclePrim {
                    id: format!("{prefix}/circle"),
                    cx: nm_to_mm(center_nm.x),
                    cy: nm_to_mm(center_nm.y),
                    r,
                    center_nm,
                    r_nm: (diameter_nm as f64) * 0.5,
                    diameter_nm,
                    full_ok: true,
                    arc_span: None,
                    exact_arc: None,
                    expected_inc: None,
                }));
                return;
            }

            if hw > 1e-9 {
                if let Some(lp) = LinePrim::new(
                    format!("{prefix}/top"),
                    tr(-hw, -h * 0.5),
                    tr(hw, -h * 0.5),
                    None,
                    false,
                ) {
                    out.push(SourcePrim::Line(lp));
                }
                if let Some(lp) = LinePrim::new(
                    format!("{prefix}/bottom"),
                    tr(hw, h * 0.5),
                    tr(-hw, h * 0.5),
                    None,
                    false,
                ) {
                    out.push(SourcePrim::Line(lp));
                }
            }
            if hh > 1e-9 {
                if let Some(lp) = LinePrim::new(
                    format!("{prefix}/right"),
                    tr(w * 0.5, -hh),
                    tr(w * 0.5, hh),
                    None,
                    false,
                ) {
                    out.push(SourcePrim::Line(lp));
                }
                if let Some(lp) = LinePrim::new(
                    format!("{prefix}/left"),
                    tr(-w * 0.5, hh),
                    tr(-w * 0.5, -hh),
                    None,
                    false,
                ) {
                    out.push(SourcePrim::Line(lp));
                }
            }

            let push_arc = |out: &mut Vec<SourcePrim>, name: &str, c_loc: P, s_loc: P, sweep: f64| {
                let c = tr(c_loc[0], c_loc[1]);
                let s = tr(s_loc[0], s_loc[1]);
                let center_nm = Point::new(mm_to_nm(c[0]), mm_to_nm(c[1]));
                let diameter_nm = mm_to_nm(2.0 * r);
                let a0 = (s[1] - c[1]).atan2(s[0] - c[0]);
                out.push(SourcePrim::Circle(CirclePrim {
                    id: format!("{prefix}/{name}"),
                    cx: nm_to_mm(center_nm.x),
                    cy: nm_to_mm(center_nm.y),
                    r,
                    center_nm,
                    r_nm: (diameter_nm as f64) * 0.5,
                    diameter_nm,
                    full_ok: false,
                    arc_span: Some((a0, sweep)),
                    exact_arc: None,
                    expected_inc: None,
                }));
            };

            if hw > 1e-9 && hh > 1e-9 {
                push_arc(out, "tr", [hw, -hh], [hw, -h * 0.5], 0.5 * PI);
                push_arc(out, "br", [hw, hh], [w * 0.5, hh], 0.5 * PI);
                push_arc(out, "bl", [-hw, hh], [-hw, h * 0.5], 0.5 * PI);
                push_arc(out, "tl", [-hw, -hh], [-w * 0.5, -hh], 0.5 * PI);
            } else if hw <= 1e-9 {
                push_arc(out, "top", [0.0, -hh], [-r, -hh], PI);
                push_arc(out, "bottom", [0.0, hh], [r, hh], PI);
            } else {
                push_arc(out, "right", [hw, 0.0], [hw, -r], PI);
                push_arc(out, "left", [-hw, 0.0], [-hw, r], PI);
            }
        }
        Geom::Slot { x1, y1, x2, y2, w } => {
            if *w <= 0.0 {
                return;
            }
            let r = *w * 0.5;
            let (dx, dy) = (*x2 - *x1, *y2 - *y1);
            let len = dx.hypot(dy);
            if len <= 1e-9 {
                let center_nm = Point::new(mm_to_nm(*x1), mm_to_nm(*y1));
                let diameter_nm = mm_to_nm(*w);
                out.push(SourcePrim::Circle(CirclePrim {
                    id: format!("{prefix}/circle"),
                    cx: nm_to_mm(center_nm.x),
                    cy: nm_to_mm(center_nm.y),
                    r,
                    center_nm,
                    r_nm: (diameter_nm as f64) * 0.5,
                    diameter_nm,
                    full_ok: true,
                    arc_span: None,
                    exact_arc: None,
                    expected_inc: None,
                }));
                return;
            }
            // Normal pointing to the "top/left" side when walking `(x1, y1) -> (x2, y2)`, so
            // `side1 -> cap2 -> side2 -> cap1` winds clockwise in Y-down.
            let (nx, ny) = (dy / len, -dx / len);
            let p1_top = [*x1 + nx * r, *y1 + ny * r];
            let p2_top = [*x2 + nx * r, *y2 + ny * r];
            let p2_bot = [*x2 - nx * r, *y2 - ny * r];
            let p1_bot = [*x1 - nx * r, *y1 - ny * r];
            if let Some(lp) = LinePrim::new(format!("{prefix}/side1"), p1_top, p2_top, None, false) {
                out.push(SourcePrim::Line(lp));
            }
            if let Some(lp) = LinePrim::new(format!("{prefix}/side2"), p2_bot, p1_bot, None, false) {
                out.push(SourcePrim::Line(lp));
            }
            let diameter_nm = mm_to_nm(*w);
            for (name, cx, cy, start_pt) in [
                ("cap2", *x2, *y2, p2_top),
                ("cap1", *x1, *y1, p1_bot),
            ] {
                let center_nm = Point::new(mm_to_nm(cx), mm_to_nm(cy));
                let a0 = (start_pt[1] - cy).atan2(start_pt[0] - cx);
                out.push(SourcePrim::Circle(CirclePrim {
                    id: format!("{prefix}/{name}"),
                    cx: nm_to_mm(center_nm.x),
                    cy: nm_to_mm(center_nm.y),
                    r,
                    center_nm,
                    r_nm: (diameter_nm as f64) * 0.5,
                    diameter_nm,
                    full_ok: false,
                    arc_span: Some((a0, PI)),
                    exact_arc: None,
                    expected_inc: None,
                }));
            }
        }
        Geom::Polygon {
            points,
            approximate,
        } => {
            if points.len() < 3 {
                return;
            }
            for k in 0..points.len() {
                let a = points[k];
                let b = points[(k + 1) % points.len()];
                if let Some(lp) = LinePrim::new(format!("{prefix}/e{k}"), a, b, None, *approximate) {
                    out.push(SourcePrim::Line(lp));
                }
            }
        }
        Geom::Path {
            elements,
            approximate: path_approx,
        } => {
            for (k, el) in elements.iter().enumerate() {
                let id = el
                    .id
                    .clone()
                    .unwrap_or_else(|| format!("{prefix}/seg{k}"));
                match el.kind {
                    ElementKind::Line { start, end } => {
                        let a = [nm_to_mm(start.x), nm_to_mm(start.y)];
                        let b = [nm_to_mm(end.x), nm_to_mm(end.y)];
                        if let Some(lp) = LinePrim::new(id, a, b, Some((start, end)), *path_approx) {
                            out.push(SourcePrim::Line(lp));
                        }
                    }
                    ElementKind::Arc {
                        center,
                        start,
                        end,
                        angle_increasing,
                    } => {
                        let r_nm = ((start.x - center.x) as f64)
                            .hypot((start.y - center.y) as f64);
                        let r = r_nm / (NM_PER_MM as f64);
                        let a_start = ((start.y - center.y) as f64)
                            .atan2((start.x - center.x) as f64);
                        let a_end =
                            ((end.y - center.y) as f64).atan2((end.x - center.x) as f64);
                        let span = if angle_increasing {
                            (a_start, pos_sweep(a_start, a_end))
                        } else {
                            (a_end, pos_sweep(a_end, a_start))
                        };
                        out.push(SourcePrim::Circle(CirclePrim {
                            id,
                            cx: nm_to_mm(center.x),
                            cy: nm_to_mm(center.y),
                            r,
                            center_nm: center,
                            r_nm,
                            diameter_nm: (2.0 * r_nm).round() as i64,
                            full_ok: false,
                            arc_span: Some(span),
                            exact_arc: Some((start, end, angle_increasing)),
                            expected_inc: None,
                        }));
                    }
                    ElementKind::Circle {
                        center,
                        diameter_nm,
                    } => {
                        let r_nm = (diameter_nm as f64) * 0.5;
                        out.push(SourcePrim::Circle(CirclePrim {
                            id,
                            cx: nm_to_mm(center.x),
                            cy: nm_to_mm(center.y),
                            r: r_nm / (NM_PER_MM as f64),
                            center_nm: center,
                            r_nm,
                            diameter_nm,
                            full_ok: true,
                            arc_span: None,
                            exact_arc: None,
                            expected_inc: None,
                        }));
                    }
                }
            }
        }
        Geom::Generator { .. } => {}
    }
}

fn symmetric_prim(p: &SourcePrim, sym: &Symmetry) -> SourcePrim {
    let [sx, sy] = sym.centre;
    let map_pt = |pt: P| -> P {
        match sym.kind {
            SymmetryKind::None => pt,
            SymmetryKind::MirrorX => [2.0 * sx - pt[0], pt[1]],
            SymmetryKind::MirrorY => [pt[0], 2.0 * sy - pt[1]],
            SymmetryKind::HalfTurn => [2.0 * sx - pt[0], 2.0 * sy - pt[1]],
        }
    };
    let map_nm = |pt: Point| -> Point {
        let cx_nm = mm_to_nm(sx);
        let cy_nm = mm_to_nm(sy);
        match sym.kind {
            SymmetryKind::None => pt,
            SymmetryKind::MirrorX => Point::new(2 * cx_nm - pt.x, pt.y),
            SymmetryKind::MirrorY => Point::new(pt.x, 2 * cy_nm - pt.y),
            SymmetryKind::HalfTurn => Point::new(2 * cx_nm - pt.x, 2 * cy_nm - pt.y),
        }
    };
    match p {
        SourcePrim::Line(lp) => {
            let a = map_pt(lp.a);
            let b = map_pt(lp.b);
            let exact_nm = lp.exact_nm.map(|(p0, p1)| (map_nm(p0), map_nm(p1)));
            SourcePrim::Line(
                LinePrim::new(format!("{}:sym", lp.id), a, b, exact_nm, lp.from_approx).unwrap(),
            )
        }
        SourcePrim::Circle(cp) => {
            let center_nm = map_nm(cp.center_nm);
            let c_mm = [nm_to_mm(center_nm.x), nm_to_mm(center_nm.y)];
            let flips = matches!(sym.kind, SymmetryKind::MirrorX | SymmetryKind::MirrorY);
            let arc_span = cp.arc_span.map(|(a0, sweep)| {
                let p0 = [cp.cx + cp.r * a0.cos(), cp.cy + cp.r * a0.sin()];
                let p1 = [
                    cp.cx + cp.r * (a0 + sweep).cos(),
                    cp.cy + cp.r * (a0 + sweep).sin(),
                ];
                let q0 = map_pt(p0);
                let q1 = map_pt(p1);
                let b0 = (q0[1] - c_mm[1]).atan2(q0[0] - c_mm[0]);
                let b1 = (q1[1] - c_mm[1]).atan2(q1[0] - c_mm[0]);
                if flips {
                    // Reflection reverses orientation, so the CCW span `[a0, a0 + sweep]` maps to
                    // `[b1, b1 + sweep]`.
                    (b1, sweep)
                } else {
                    (b0, sweep)
                }
            });
            let exact_arc = cp
                .exact_arc
                .map(|(s, e, inc)| (map_nm(s), map_nm(e), if flips { !inc } else { inc }));
            let expected_inc = cp.expected_inc.map(|inc| if flips { !inc } else { inc });
            SourcePrim::Circle(CirclePrim {
                id: format!("{}:sym", cp.id),
                cx: c_mm[0],
                cy: c_mm[1],
                r: cp.r,
                center_nm,
                r_nm: cp.r_nm,
                diameter_nm: cp.diameter_nm,
                full_ok: cp.full_ok,
                arc_span,
                exact_arc,
                expected_inc,
            })
        }
    }
}

#[derive(Debug, Clone)]
enum SegGeom {
    Line {
        id: Option<String>,
        /// Line anchor & unit direction in nanometres (for analytical intersections).
        a_nm: [f64; 2],
        u_nm: [f64; 2],
        start_clip: Point,
        end_clip: Point,
        exact_nm: Option<(Point, Point)>,
    },
    Arc {
        id: String,
        center_nm: Point,
        r_nm: f64,
        start_clip: Point,
        end_clip: Point,
        mid_clip: [f64; 2],
        run_len_nm: f64,
        angle_hint: Option<bool>,
        exact_arc: Option<(Point, Point, bool)>,
    },
}

#[derive(Debug, Clone)]
struct ReconstructedLoop {
    full_circle: Option<(String, Point, i64)>,
    segs: Vec<SegGeom>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EdgeMatch {
    Prim(usize),
    RawChord,
}

fn reconstruct_loop_segments(
    path: &Path64,
    prims: &[SourcePrim],
    force_lines: bool,
    post_round: bool,
    approximate: &mut bool,
) -> ReconstructedLoop {
    // Strip adjacent identical vertices.
    let mut pts: Vec<P> = Vec::with_capacity(path.len());
    let mut pts_nm: Vec<Point> = Vec::with_capacity(path.len());
    for q in path {
        let p_nm = Point::new(q.x * 100, q.y * 100);
        if pts_nm.last().copied() != Some(p_nm) {
            pts.push([q.x as f64 / SCALE, q.y as f64 / SCALE]);
            pts_nm.push(p_nm);
        }
    }
    if pts_nm.len() >= 2 && pts_nm.first() == pts_nm.last() {
        pts.pop();
        pts_nm.pop();
    }
    let m = pts.len();
    if m < 3 {
        return ReconstructedLoop {
            full_circle: None,
            segs: Vec::new(),
        };
    }

    if force_lines {
        *approximate = true;
        let segs = build_raw_line_runs(&pts_nm);
        return ReconstructedLoop {
            full_circle: None,
            segs,
        };
    }

    // Check if the entire path is one unbroken full circle.
    if let Some((id, center_nm, diameter_nm)) = match_full_circle(&pts, prims, post_round) {
        return ReconstructedLoop {
            full_circle: Some((id, center_nm, diameter_nm)),
            segs: Vec::new(),
        };
    }

    // Match each edge `k -> (k + 1) % m` to the best analytical primitive.
    let mut labels: Vec<EdgeMatch> = (0..m)
        .map(|k| {
            let a = pts[k];
            let b = pts[(k + 1) % m];
            classify_edge(a, b, prims, post_round)
        })
        .collect();

    // Absorb tiny (< 20 um) single-edge Clipper2 rounding slivers at primitive junctions, and
    // resolve single chords that straddle a tangent/inflection transition between two primitives
    // (where `p_k` lies on `prev` and `p_{k+1}` lies on `next`).
    if m >= 4 {
        for pass in 0..2 {
            for k in 0..m {
                let a = pts[k];
                let b = pts[(k + 1) % m];
                let len = (b[0] - a[0]).hypot(b[1] - a[1]);
                let prev = labels[(k + m - 1) % m];
                let next = labels[(k + 1) % m];
                if len <= 0.020 && labels[k] != prev && labels[k] != next {
                    labels[k] = if prev == next || pass == 1 { prev } else { next };
                } else if labels[k] == EdgeMatch::RawChord
                    && len <= 1.5
                    && prev != next
                {
                    if let (EdgeMatch::Prim(pi), EdgeMatch::Prim(ni)) = (prev, next) {
                        if prims_meet_along_chord(&prims[pi], &prims[ni], a, b) {
                            labels[k] = next;
                        }
                    }
                }
            }
        }
    }

    // Group consecutive edges with identical `Prim(idx)` into runs; adjacent `RawChord` edges
    // merge only when collinear within 0.2 um.
    #[derive(Clone, Copy)]
    struct Run {
        kind: EdgeMatch,
        start_v: usize,
        end_v: usize,
        edge_count: usize,
    }

    let can_merge = |r: &Run, next_lbl: EdgeMatch, v_prev: usize, v_cur: usize, v_next: usize| -> bool {
        match (r.kind, next_lbl) {
            (EdgeMatch::Prim(i), EdgeMatch::Prim(j)) => i == j,
            (EdgeMatch::RawChord, EdgeMatch::RawChord) => {
                is_collinear_nm(pts_nm[v_prev], pts_nm[v_cur], pts_nm[v_next])
            }
            _ => false,
        }
    };

    let mut runs: Vec<Run> = Vec::new();
    for (k, &lbl) in labels.iter().enumerate() {
        let next_v = (k + 1) % m;
        let merged = if let Some(last) = runs.last_mut() {
            if can_merge(last, lbl, last.start_v, k, next_v) {
                last.end_v = next_v;
                last.edge_count += 1;
                true
            } else {
                false
            }
        } else {
            false
        };
        if !merged {
            runs.push(Run {
                kind: lbl,
                start_v: k,
                end_v: next_v,
                edge_count: 1,
            });
        }
    }

    // Merge wrap-around run across vertex 0 if last and first belong to the same primitive.
    if runs.len() >= 2 {
        let first = runs[0];
        let last = *runs.last().unwrap();
        let merge_wrap = match (last.kind, first.kind) {
            (EdgeMatch::Prim(i), EdgeMatch::Prim(j)) => i == j,
            (EdgeMatch::RawChord, EdgeMatch::RawChord) => {
                is_collinear_nm(pts_nm[last.start_v], pts_nm[0], pts_nm[first.end_v])
            }
            _ => false,
        };
        if merge_wrap {
            let total = last.edge_count + first.edge_count;
            runs[0].start_v = last.start_v;
            runs[0].edge_count = total;
            runs.pop();
        }
    }

    // If a single CirclePrim ended up covering the entire loop after sliver cleanup, emit Circle.
    if runs.len() == 1 {
        if let EdgeMatch::Prim(idx) = runs[0].kind {
            if let SourcePrim::Circle(cp) = &prims[idx] {
                if cp.full_ok {
                    return ReconstructedLoop {
                        full_circle: Some((cp.id.clone(), cp.center_nm, cp.diameter_nm)),
                        segs: Vec::new(),
                    };
                }
            }
        }
    }

    let mut segs = Vec::with_capacity(runs.len());
    for r in &runs {
        let start_clip = pts_nm[r.start_v];
        let end_clip = pts_nm[r.end_v];
        let mut run_len_nm = 0.0;
        for step in 0..r.edge_count {
            let u = pts_nm[(r.start_v + step) % m];
            let v = pts_nm[(r.start_v + step + 1) % m];
            run_len_nm += ((v.x - u.x) as f64).hypot((v.y - u.y) as f64);
        }
        match r.kind {
            EdgeMatch::Prim(idx) => match &prims[idx] {
                SourcePrim::Line(lp) => {
                    if lp.from_approx {
                        *approximate = true;
                    }
                    let (dx, dy) = (
                        (end_clip.x - start_clip.x) as f64,
                        (end_clip.y - start_clip.y) as f64,
                    );
                    let dot = dx * lp.ux + dy * lp.uy;
                    let (ux, uy) = if dot >= 0.0 {
                        (lp.ux, lp.uy)
                    } else {
                        (-lp.ux, -lp.uy)
                    };
                    let a_nm = if let Some((p0, _)) = lp.exact_nm {
                        [p0.x as f64, p0.y as f64]
                    } else {
                        [lp.a[0] * (NM_PER_MM as f64), lp.a[1] * (NM_PER_MM as f64)]
                    };
                    segs.push(SegGeom::Line {
                        id: Some(lp.id.clone()),
                        a_nm,
                        u_nm: [ux, uy],
                        start_clip,
                        end_clip,
                        exact_nm: lp.exact_nm,
                    });
                }
                SourcePrim::Circle(cp) => {
                    let mid_clip = if r.edge_count >= 2 {
                        let mid_v = (r.start_v + r.edge_count / 2) % m;
                        [pts_nm[mid_v].x as f64, pts_nm[mid_v].y as f64]
                    } else {
                        [
                            0.5 * ((start_clip.x + end_clip.x) as f64),
                            0.5 * ((start_clip.y + end_clip.y) as f64),
                        ]
                    };
                    // Use known direction (`expected_inc`) when available; otherwise for a 1-chord
                    // arc run, infer sweep direction from cross((start - c), (end - c)).
                    let angle_hint = if let Some(inc) = cp.expected_inc {
                        Some(inc)
                    } else if r.edge_count == 1 {
                        let v0x = (start_clip.x - cp.center_nm.x) as f64;
                        let v0y = (start_clip.y - cp.center_nm.y) as f64;
                        let v1x = (end_clip.x - cp.center_nm.x) as f64;
                        let v1y = (end_clip.y - cp.center_nm.y) as f64;
                        Some(v0x * v1y - v0y * v1x > 0.0)
                    } else {
                        None
                    };
                    segs.push(SegGeom::Arc {
                        id: cp.id.clone(),
                        center_nm: cp.center_nm,
                        r_nm: cp.r_nm,
                        start_clip,
                        end_clip,
                        mid_clip,
                        run_len_nm,
                        angle_hint,
                        exact_arc: cp.exact_arc,
                    });
                }
            },
            EdgeMatch::RawChord => {
                // An unmatched chord means Clipper2 rounded or generated a curve we have no exact
                // source arc for (e.g. a collapsed fillet); mark approximate cleanly.
                *approximate = true;
                let dx = (end_clip.x - start_clip.x) as f64;
                let dy = (end_clip.y - start_clip.y) as f64;
                let len = dx.hypot(dy).max(1.0);
                segs.push(SegGeom::Line {
                    id: None,
                    a_nm: [start_clip.x as f64, start_clip.y as f64],
                    u_nm: [dx / len, dy / len],
                    start_clip,
                    end_clip,
                    exact_nm: None,
                });
            }
        }
    }

    ReconstructedLoop {
        full_circle: None,
        segs,
    }
}

fn prims_meet_along_chord(p1: &SourcePrim, p2: &SourcePrim, a: P, b: P) -> bool {
    let a_nm = [a[0] * (NM_PER_MM as f64), a[1] * (NM_PER_MM as f64)];
    let b_nm = [b[0] * (NM_PER_MM as f64), b[1] * (NM_PER_MM as f64)];
    let cands: Vec<[f64; 2]> = match (p1, p2) {
        (SourcePrim::Line(l1), SourcePrim::Line(l2)) => intersect_lines_f64(
            [l1.a[0] * 1e6, l1.a[1] * 1e6],
            [l1.ux, l1.uy],
            [l2.a[0] * 1e6, l2.a[1] * 1e6],
            [l2.ux, l2.uy],
        )
        .into_iter()
        .collect(),
        (SourcePrim::Line(l), SourcePrim::Circle(c))
        | (SourcePrim::Circle(c), SourcePrim::Line(l)) => intersect_line_circle_f64(
            [l.a[0] * 1e6, l.a[1] * 1e6],
            [l.ux, l.uy],
            [c.center_nm.x as f64, c.center_nm.y as f64],
            c.r_nm,
            30_000.0,
        ),
        (SourcePrim::Circle(c1), SourcePrim::Circle(c2)) => intersect_circles_f64(
            [c1.center_nm.x as f64, c1.center_nm.y as f64],
            c1.r_nm,
            [c2.center_nm.x as f64, c2.center_nm.y as f64],
            c2.r_nm,
            30_000.0,
        ),
    };
    let (dx, dy) = (b_nm[0] - a_nm[0], b_nm[1] - a_nm[1]);
    let l2 = dx * dx + dy * dy;
    cands.into_iter().any(|q| {
        let t = if l2 > 0.0 {
            (((q[0] - a_nm[0]) * dx + (q[1] - a_nm[1]) * dy) / l2).clamp(-0.25, 1.25)
        } else {
            0.0
        };
        let proj = [a_nm[0] + t * dx, a_nm[1] + t * dy];
        (q[0] - proj[0]).hypot(q[1] - proj[1]) <= 25_000.0
    })
}

fn build_raw_line_runs(pts_nm: &[Point]) -> Vec<SegGeom> {
    let m = pts_nm.len();
    let mut verts: Vec<Point> = Vec::with_capacity(m);
    for &p in pts_nm {
        if verts.len() >= 2 && is_collinear_nm(verts[verts.len() - 2], verts[verts.len() - 1], p) {
            *verts.last_mut().unwrap() = p;
        } else {
            verts.push(p);
        }
    }
    if verts.len() >= 3 && is_collinear_nm(verts[verts.len() - 2], verts[verts.len() - 1], verts[0])
    {
        verts.pop();
    }
    let k = verts.len();
    (0..k)
        .map(|i| {
            let s = verts[i];
            let e = verts[(i + 1) % k];
            let (dx, dy) = ((e.x - s.x) as f64, (e.y - s.y) as f64);
            let len = dx.hypot(dy).max(1.0);
            SegGeom::Line {
                id: None,
                a_nm: [s.x as f64, s.y as f64],
                u_nm: [dx / len, dy / len],
                start_clip: s,
                end_clip: e,
                exact_nm: None,
            }
        })
        .collect()
}

fn is_collinear_nm(a: Point, b: Point, c: Point) -> bool {
    let (ux, uy) = ((b.x - a.x) as f64, (b.y - a.y) as f64);
    let (vx, vy) = ((c.x - a.x) as f64, (c.y - a.y) as f64);
    let len = vx.hypot(vy);
    if len < 1.0 {
        return true;
    }
    let perp = (ux * vy - uy * vx).abs() / len;
    perp <= 50.0 && (ux * (c.x - b.x) as f64 + uy * (c.y - b.y) as f64) >= 0.0
}

fn match_full_circle(
    pts: &[P],
    prims: &[SourcePrim],
    post_round: bool,
) -> Option<(String, Point, i64)> {
    if pts.len() < 8 {
        return None;
    }
    let tol = if post_round {
        0.020_f64
    } else {
        0.008_f64.max(2.0 * ARC_TOLERANCE)
    };
    for prim in prims {
        let SourcePrim::Circle(cp) = prim else {
            continue;
        };
        if !cp.full_ok {
            continue;
        }
        if !pts
            .iter()
            .all(|p| ((p[0] - cp.cx).hypot(p[1] - cp.cy) - cp.r).abs() <= tol)
        {
            continue;
        }
        // Verify edges also have small sagitta (rules out regular inscribed triangle/square) and
        // total angular winding around `(cx, cy)` is +-2pi.
        let mut total_turn = 0.0;
        let mut valid_chords = true;
        for i in 0..pts.len() {
            let p0 = pts[i];
            let p1 = pts[(i + 1) % pts.len()];
            let chord = (p1[0] - p0[0]).hypot(p1[1] - p0[1]);
            let sag = cp.r - (cp.r * cp.r - 0.25 * chord * chord).max(0.0).sqrt();
            if sag > 0.020 {
                valid_chords = false;
                break;
            }
            let a0 = (p0[1] - cp.cy).atan2(p0[0] - cp.cx);
            let a1 = (p1[1] - cp.cy).atan2(p1[0] - cp.cx);
            let mut da = (a1 - a0).rem_euclid(TAU);
            if da > PI {
                da -= TAU;
            }
            total_turn += da;
        }
        if valid_chords && (total_turn.abs() - TAU).abs() < 0.25 {
            return Some((cp.id.clone(), cp.center_nm, cp.diameter_nm));
        }
    }
    None
}

fn classify_edge(a: P, b: P, prims: &[SourcePrim], post_round: bool) -> EdgeMatch {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len = dx.hypot(dy);
    if len < 1e-9 {
        return EdgeMatch::RawChord;
    }
    let (ex, ey) = (dx / len, dy / len);
    let mid = [0.5 * (a[0] + b[0]), 0.5 * (a[1] + b[1])];
    let (rad_min_tol, rad_max_tol) = if post_round {
        // `round` runs three chained Clipper2 `inflate` passes at `ARC_TOLERANCE` (0.005 mm), so
        // curves can accumulate up to ~3 * ARC_TOLERANCE radial drift.
        (0.016, 0.022)
    } else {
        (0.004, 0.0085)
    };

    let mut best_line: Option<(usize, f64)> = None;
    let mut best_circle: Option<(usize, f64, f64)> = None; // (idx, score, r)

    for (idx, prim) in prims.iter().enumerate() {
        match prim {
            SourcePrim::Line(lp) => {
                let d0 = ((a[0] - lp.a[0]) * (-lp.uy) + (a[1] - lp.a[1]) * lp.ux).abs();
                let d1 = ((b[0] - lp.a[0]) * (-lp.uy) + (b[1] - lp.a[1]) * lp.ux).abs();
                if d0 > 0.002 || d1 > 0.002 {
                    continue;
                }
                let cross = (ex * lp.uy - ey * lp.ux).abs();
                let max_cross = if len > 0.05 { 0.005 } else { 0.03 };
                if cross > max_cross {
                    continue;
                }
                let t0 = (a[0] - lp.a[0]) * lp.ux + (a[1] - lp.a[1]) * lp.uy;
                let t1 = (b[0] - lp.a[0]) * lp.ux + (b[1] - lp.a[1]) * lp.uy;
                let overhang = (-t0.min(t1)).max(t0.max(t1) - lp.len).max(0.0);
                if overhang > 0.025 {
                    continue;
                }
                let score = d0.max(d1) + 0.1 * overhang;
                if best_line.is_none_or(|(_, b_sc)| score < b_sc) {
                    best_line = Some((idx, score));
                }
            }
            SourcePrim::Circle(cp) => {
                let er0 = ((a[0] - cp.cx).hypot(a[1] - cp.cy) - cp.r).abs();
                let er1 = ((b[0] - cp.cx).hypot(b[1] - cp.cy) - cp.r).abs();
                if er0.max(er1) > rad_max_tol || er0.min(er1) > rad_min_tol {
                    continue;
                }
                // Reject straight secant chords across the circle: genuine Clipper2 arc chords have
                // sagitta <= ~ARC_TOLERANCE (0.005 mm).
                if len >= 2.0 * cp.r {
                    continue;
                }
                let sag = cp.r - (cp.r * cp.r - 0.25 * len * len).max(0.0).sqrt();
                if sag > 0.016 {
                    continue;
                }
                if let Some(want_inc) = cp.expected_inc {
                    let cross_dir = (mid[0] - cp.cx) * dy - (mid[1] - cp.cy) * dx;
                    if (cross_dir > 0.0) != want_inc {
                        continue;
                    }
                }
                if let Some((a0, sweep)) = cp.arc_span {
                    let margin_mm = if cp.expected_inc.is_some() { 0.012 } else { 0.022 };
                    let margin = (margin_mm / cp.r.max(0.05)).clamp(0.004, 0.22);
                    let in_span = |pt: P, m: f64| -> bool {
                        let ang = (pt[1] - cp.cy).atan2(pt[0] - cp.cx);
                        let rel = (ang - a0).rem_euclid(TAU);
                        rel <= sweep + m || rel >= TAU - m
                    };
                    if !in_span(mid, margin)
                        || !in_span(a, margin * 1.6)
                        || !in_span(b, margin * 1.6)
                    {
                        continue;
                    }
                }
                let score = 0.5 * (er0 + er1);
                if best_circle.is_none_or(|(_, b_sc, _)| score < b_sc) {
                    best_circle = Some((idx, score, cp.r));
                }
            }
        }
    }

    match (best_line, best_circle) {
        (Some((li, l_sc)), Some((ci, c_sc, r))) => {
            // A chord longer than a single Clipper2 arc step that hugs a line within 1 um is the
            // straight line, never the tangent arc.
            let max_arc_chord = 2.4 * (2.0 * r * ARC_TOLERANCE).max(1e-6).sqrt();
            if len > max_arc_chord || l_sc + 0.0003 < c_sc {
                EdgeMatch::Prim(li)
            } else {
                EdgeMatch::Prim(ci)
            }
        }
        (Some((li, _)), None) => EdgeMatch::Prim(li),
        (None, Some((ci, _, _))) => EdgeMatch::Prim(ci),
        (None, None) => EdgeMatch::RawChord,
    }
}

/// Generate analytical radius-`cr` tangent fillet `CirclePrim`s for non-tangent corners of `segs`
/// whose arc actually appears on `rounded_mm`.
fn generate_fillet_prims(
    segs: &[SegGeom],
    cr: f64,
    rounded_mm: &[Vec<P>],
    prims: &mut Vec<SourcePrim>,
) {
    let n = segs.len();
    if n < 2 || cr <= 0.0 {
        return;
    }
    let cr_nm = cr * (NM_PER_MM as f64);

    // Check adjacent pairs `(i, (i + 1) % n)` and also pairs separated by up to 2 short segments
    // that `round(&acc, cr)` may consume (`gap <= 2.4 * cr`).
    for i in 0..n {
        for step in 1..=3.min(n - 1) {
            let j = (i + step) % n;
            let corner_hint = segs[i].end_clip();
            let j_start = segs[j].start_clip();
            let gap_nm = ((j_start.x - corner_hint.x) as f64)
                .hypot((j_start.y - corner_hint.y) as f64);
            if step > 1 && gap_nm > 2.4 * cr_nm {
                continue;
            }
            let mid_hint = [
                0.5 * ((corner_hint.x + j_start.x) as f64),
                0.5 * ((corner_hint.y + j_start.y) as f64),
            ];
            let id_a = segs[i].id_stem(i);
            let id_b = segs[j].id_stem(j);

            // Try both convex (`+cr_nm` into material, `angle_increasing = true`) and concave
            // (`-cr_nm` into air, `angle_increasing = false`) offsets.
            for (tag, s_nm, is_cvx) in [("cvx", cr_nm, true), ("ccv", -cr_nm, false)] {
                for cand_c in offset_intersections(&segs[i], &segs[j], s_nm) {
                    let dist_to_corner =
                        (cand_c[0] - mid_hint[0]).hypot(cand_c[1] - mid_hint[1]);
                    if dist_to_corner > 4.0 * cr_nm + 50_000.0 {
                        continue;
                    }
                    let (Some(t1), Some(t2)) = (
                        fillet_tangent_on_seg(&segs[i], cand_c, cr_nm, is_cvx),
                        fillet_tangent_on_seg(&segs[j], cand_c, cr_nm, is_cvx),
                    ) else {
                        continue;
                    };
                    // Reject already-tangent junctions (`t1 ~= t2`) and degenerate tiny fillets.
                    let chord_nm = (t2[0] - t1[0]).hypot(t2[1] - t1[1]);
                    if chord_nm < 15_000.0 {
                        continue;
                    }
                    let a1 = (t1[1] - cand_c[1]).atan2(t1[0] - cand_c[0]);
                    let a2 = (t2[1] - cand_c[1]).atan2(t2[0] - cand_c[0]);
                    let sweep = if is_cvx {
                        pos_sweep(a1, a2)
                    } else {
                        pos_sweep(a2, a1)
                    };
                    // Every valid corner fillet turns strictly between ~3 deg and 180 deg; values
                    // near 0 or > pi are tangent ghosts or wrap-around roots.
                    if !(0.05..=PI + 0.02).contains(&sweep) {
                        continue;
                    }
                    let span_a0 = if is_cvx { a1 } else { a2 };
                    if !rounded_mm.is_empty() {
                        let cx_mm = cand_c[0] / (NM_PER_MM as f64);
                        let cy_mm = cand_c[1] / (NM_PER_MM as f64);
                        let sample_d = |f: f64| -> f64 {
                            let ang = span_a0 + f * sweep;
                            let pt = [cx_mm + cr * ang.cos(), cy_mm + cr * ang.sin()];
                            min_dist_to_paths_mm(pt, rounded_mm)
                        };
                        let d_mid = sample_d(0.5);
                        let d_min = d_mid.min(sample_d(0.25)).min(sample_d(0.75));
                        if d_min > 0.022 || d_mid > 0.035 {
                            continue;
                        }
                    }

                    let center_nm = Point::new(cand_c[0].round() as i64, cand_c[1].round() as i64);
                    let diameter_nm = mm_to_nm(2.0 * cr);
                    let id = if tag == "cvx" {
                        format!("composer:fillet/{id_a}__{id_b}")
                    } else {
                        format!("composer:fillet/{id_a}__{id_b}:in")
                    };
                    prims.push(SourcePrim::Circle(CirclePrim {
                        id,
                        cx: nm_to_mm(center_nm.x),
                        cy: nm_to_mm(center_nm.y),
                        r: cr,
                        center_nm,
                        r_nm: (diameter_nm as f64) * 0.5,
                        diameter_nm,
                        full_ok: false,
                        arc_span: Some((span_a0, sweep)),
                        exact_arc: None,
                        expected_inc: Some(is_cvx),
                    }));
                }
            }
        }
    }
}

/// Compute and validate the exact point of tangency `t_pt` (in nm) where a candidate fillet circle
/// `(cand_c, cr_nm, is_cvx)` touches `seg` with matching forward tangent and within `seg`'s span.
fn fillet_tangent_on_seg(
    seg: &SegGeom,
    cand_c: [f64; 2],
    cr_nm: f64,
    is_cvx: bool,
) -> Option<[f64; 2]> {
    let s_nm = if is_cvx { cr_nm } else { -cr_nm };
    match seg {
        SegGeom::Line {
            a_nm,
            u_nm,
            start_clip,
            end_clip,
            ..
        } => {
            let proj = (cand_c[0] - a_nm[0]) * u_nm[0] + (cand_c[1] - a_nm[1]) * u_nm[1];
            let t_pt = [a_nm[0] + proj * u_nm[0], a_nm[1] + proj * u_nm[1]];
            let signed_d =
                (cand_c[0] - t_pt[0]) * (-u_nm[1]) + (cand_c[1] - t_pt[1]) * u_nm[0];
            if (signed_d - s_nm).abs() > 2_500.0 {
                return None;
            }
            let t_s = (start_clip.x as f64 - a_nm[0]) * u_nm[0]
                + (start_clip.y as f64 - a_nm[1]) * u_nm[1];
            let t_e = (end_clip.x as f64 - a_nm[0]) * u_nm[0]
                + (end_clip.y as f64 - a_nm[1]) * u_nm[1];
            if proj < t_s.min(t_e) - 35_000.0 || proj > t_s.max(t_e) + 35_000.0 {
                return None;
            }
            Some(t_pt)
        }
        SegGeom::Arc {
            center_nm,
            r_nm,
            start_clip,
            end_clip,
            ..
        } => {
            let inc = seg.resolved_angle_increasing();
            let r_expected = if inc { *r_nm - s_nm } else { *r_nm + s_nm };
            if r_expected <= 100.0 {
                return None;
            }
            let c = [center_nm.x as f64, center_nm.y as f64];
            let vx = cand_c[0] - c[0];
            let vy = cand_c[1] - c[1];
            let d = vx.hypot(vy);
            if d < 1.0 || (d - r_expected).abs() > 2_500.0 {
                return None;
            }
            let t_pt = [c[0] + r_nm * vx / d, c[1] + r_nm * vy / d];
            let rx = (t_pt[0] - c[0]) / r_nm;
            let ry = (t_pt[1] - c[1]) / r_nm;
            let seg_tan = if inc { [-ry, rx] } else { [ry, -rx] };
            let fx = (t_pt[0] - cand_c[0]) / cr_nm;
            let fy = (t_pt[1] - cand_c[1]) / cr_nm;
            let fil_tan = if is_cvx { [-fy, fx] } else { [fy, -fx] };
            if seg_tan[0] * fil_tan[0] + seg_tan[1] * fil_tan[1] < 0.998 {
                return None;
            }
            let a_s = ((start_clip.y - center_nm.y) as f64).atan2((start_clip.x - center_nm.x) as f64);
            let a_e = ((end_clip.y - center_nm.y) as f64).atan2((end_clip.x - center_nm.x) as f64);
            let a_t = (t_pt[1] - c[1]).atan2(t_pt[0] - c[0]);
            let (span_0, span_len) = if inc {
                (a_s, pos_sweep(a_s, a_e))
            } else {
                (a_e, pos_sweep(a_e, a_s))
            };
            let ang_slack = (35_000.0 / r_nm.max(50_000.0)).clamp(0.005, 0.25);
            let rel = (a_t - span_0).rem_euclid(TAU);
            if rel > span_len + ang_slack && rel < TAU - ang_slack {
                return None;
            }
            Some(t_pt)
        }
    }
}

fn min_dist_to_paths_mm(q: P, paths: &[Vec<P>]) -> f64 {
    let mut best = f64::INFINITY;
    for poly in paths {
        let m = poly.len();
        if m < 2 {
            continue;
        }
        for i in 0..m {
            best = best.min(point_seg_dist_mm(q, poly[i], poly[(i + 1) % m]));
        }
    }
    best
}

impl SegGeom {
    fn start_clip(&self) -> Point {
        match *self {
            SegGeom::Line { start_clip, .. } | SegGeom::Arc { start_clip, .. } => start_clip,
        }
    }

    fn end_clip(&self) -> Point {
        match *self {
            SegGeom::Line { end_clip, .. } | SegGeom::Arc { end_clip, .. } => end_clip,
        }
    }

    fn id_stem(&self, idx: usize) -> String {
        let raw = match self {
            SegGeom::Line { id: Some(s), .. } => s.as_str(),
            SegGeom::Line { id: None, .. } => return format!("e{idx}"),
            SegGeom::Arc { id, .. } => id.as_str(),
        };
        raw.strip_prefix("composer:").unwrap_or(raw).replace('/', "-")
    }

    fn resolved_angle_increasing(&self) -> bool {
        match self {
            SegGeom::Arc {
                center_nm,
                start_clip,
                end_clip,
                mid_clip,
                angle_hint,
                ..
            } => {
                if let Some(h) = angle_hint {
                    return *h;
                }
                infer_arc_direction(*center_nm, *start_clip, *end_clip, *mid_clip)
            }
            SegGeom::Line { .. } => false,
        }
    }
}

fn infer_arc_direction(center: Point, start: Point, end: Point, mid: [f64; 2]) -> bool {
    let a0 = ((start.y - center.y) as f64).atan2((start.x - center.x) as f64);
    let a1 = ((end.y - center.y) as f64).atan2((end.x - center.x) as f64);
    let am = (mid[1] - (center.y as f64)).atan2(mid[0] - (center.x as f64));
    let span_inc = pos_sweep(a0, a1);
    let mid_inc = (am - a0).rem_euclid(TAU);
    mid_inc < span_inc
}

#[derive(Clone, Copy)]
enum OffsetLocus {
    Line { a: [f64; 2], u: [f64; 2] },
    Circle { c: [f64; 2], r: f64 },
}

fn seg_offset_locus(seg: &SegGeom, s_nm: f64) -> Option<OffsetLocus> {
    match seg {
        SegGeom::Line { a_nm, u_nm, .. } => {
            // Material lies to the right in Y-down: `n_mat = (-uy, ux)`.
            let nx = -u_nm[1];
            let ny = u_nm[0];
            Some(OffsetLocus::Line {
                a: [a_nm[0] + s_nm * nx, a_nm[1] + s_nm * ny],
                u: *u_nm,
            })
        }
        SegGeom::Arc {
            center_nm, r_nm, ..
        } => {
            let inc = seg.resolved_angle_increasing();
            let r_off = if inc { *r_nm - s_nm } else { *r_nm + s_nm };
            if r_off <= 100.0 {
                None
            } else {
                Some(OffsetLocus::Circle {
                    c: [center_nm.x as f64, center_nm.y as f64],
                    r: r_off,
                })
            }
        }
    }
}

fn offset_intersections(s1: &SegGeom, s2: &SegGeom, s_nm: f64) -> Vec<[f64; 2]> {
    let (Some(l1), Some(l2)) = (seg_offset_locus(s1, s_nm), seg_offset_locus(s2, s_nm)) else {
        return Vec::new();
    };
    match (l1, l2) {
        (OffsetLocus::Line { a: a1, u: u1 }, OffsetLocus::Line { a: a2, u: u2 }) => {
            intersect_lines_f64(a1, u1, a2, u2).into_iter().collect()
        }
        (OffsetLocus::Line { a, u }, OffsetLocus::Circle { c, r })
        | (OffsetLocus::Circle { c, r }, OffsetLocus::Line { a, u }) => {
            intersect_line_circle_f64(a, u, c, r, 1_000.0)
        }
        (OffsetLocus::Circle { c: c1, r: r1 }, OffsetLocus::Circle { c: c2, r: r2 }) => {
            intersect_circles_f64(c1, r1, c2, r2, 1_000.0)
        }
    }
}

fn intersect_lines_f64(a1: [f64; 2], u1: [f64; 2], a2: [f64; 2], u2: [f64; 2]) -> Option<[f64; 2]> {
    let det = u1[0] * u2[1] - u1[1] * u2[0];
    if det.abs() < 1e-7 {
        return None;
    }
    let dx = a2[0] - a1[0];
    let dy = a2[1] - a1[1];
    let t = (dx * u2[1] - dy * u2[0]) / det;
    Some([a1[0] + t * u1[0], a1[1] + t * u1[1]])
}

fn intersect_line_circle_f64(
    a: [f64; 2],
    u: [f64; 2],
    c: [f64; 2],
    r: f64,
    tangent_slack_nm: f64,
) -> Vec<[f64; 2]> {
    let vx = c[0] - a[0];
    let vy = c[1] - a[1];
    let proj = vx * u[0] + vy * u[1];
    let foot = [a[0] + proj * u[0], a[1] + proj * u[1]];
    let dx = foot[0] - c[0];
    let dy = foot[1] - c[1];
    let d = dx.hypot(dy);
    if d > r + tangent_slack_nm {
        return Vec::new();
    }
    if d >= r {
        if d < 1e-9 {
            return Vec::new();
        }
        return vec![[c[0] + r * (dx / d), c[1] + r * (dy / d)]];
    }
    let h = (r * r - d * d).max(0.0).sqrt();
    if h < 1e-6 {
        vec![foot]
    } else {
        vec![
            [foot[0] - h * u[0], foot[1] - h * u[1]],
            [foot[0] + h * u[0], foot[1] + h * u[1]],
        ]
    }
}

fn intersect_circles_f64(
    c1: [f64; 2],
    r1: f64,
    c2: [f64; 2],
    r2: f64,
    tangent_slack_nm: f64,
) -> Vec<[f64; 2]> {
    let dx = c2[0] - c1[0];
    let dy = c2[1] - c1[1];
    let d = dx.hypot(dy);
    if d < 1.0 {
        return Vec::new();
    }
    let ux = dx / d;
    let uy = dy / d;
    if d > r1 + r2 + tangent_slack_nm || d + r1.min(r2) + tangent_slack_nm < r1.max(r2) {
        return Vec::new();
    }
    if d >= r1 + r2 {
        return vec![[c1[0] + r1 * ux, c1[1] + r1 * uy]];
    }
    if d <= (r1 - r2).abs() {
        let sign = if r1 >= r2 { 1.0 } else { -1.0 };
        return vec![[c1[0] + sign * r1 * ux, c1[1] + sign * r1 * uy]];
    }
    let a = (r1 * r1 - r2 * r2 + d * d) / (2.0 * d);
    let h = (r1 * r1 - a * a).max(0.0).sqrt();
    let mx = c1[0] + a * ux;
    let my = c1[1] + a * uy;
    let px = -uy;
    let py = ux;
    if h < 1e-6 {
        vec![[mx, my]]
    } else {
        vec![[mx + h * px, my + h * py], [mx - h * px, my - h * py]]
    }
}

fn build_loop_from_path(
    path: &Path64,
    prims: &[SourcePrim],
    force_lines: bool,
    post_round: bool,
    fallback_stem: &str,
    approximate: &mut bool,
) -> Loop {
    if force_lines {
        *approximate = true;
        return chord_fallback_loop(path, fallback_stem);
    }

    let mut loop_approx = false;
    let rl = reconstruct_loop_segments(path, prims, false, post_round, &mut loop_approx);
    if let Some((id, center, diameter_nm)) = rl.full_circle {
        let lp = Loop::new(vec![Element::new(
            Some(id),
            ElementKind::Circle {
                center,
                diameter_nm,
            },
        )]);
        if validate_loop_against_path(&lp, path, post_round, loop_approx) {
            *approximate |= loop_approx;
            return lp;
        }
        *approximate = true;
        return chord_fallback_loop(path, fallback_stem);
    }

    let n = rl.segs.len();
    if n < 2 {
        *approximate = true;
        return chord_fallback_loop(path, fallback_stem);
    }

    // Compute exact junction `i`: where `segs[(i + n - 1) % n]` meets `segs[i]`.
    let junctions: Vec<Point> = (0..n)
        .map(|i| {
            let prev = &rl.segs[(i + n - 1) % n];
            let curr = &rl.segs[i];
            solve_junction(prev, curr)
        })
        .collect();

    let mut elements: Vec<Element> = Vec::with_capacity(n);
    for i in 0..n {
        let start = junctions[i];
        let end = junctions[(i + 1) % n];
        if start == end {
            continue;
        }
        match &rl.segs[i] {
            SegGeom::Line { id, .. } => {
                let elem_id = id
                    .clone()
                    .unwrap_or_else(|| format!("{fallback_stem}/e{i}"));
                elements.push(Element::new(Some(elem_id), ElementKind::Line { start, end }));
            }
            SegGeom::Arc {
                id,
                center_nm,
                mid_clip,
                run_len_nm,
                angle_hint,
                exact_arc,
                ..
            } => {
                let angle_increasing = match *exact_arc {
                    Some((es, ee, einc)) if es == start && ee == end => einc,
                    Some((es, ee, einc)) if es == end && ee == start => !einc,
                    _ => angle_hint
                        .unwrap_or_else(|| infer_arc_direction(*center_nm, start, end, *mid_clip)),
                };

                // Verify `board-interface` concentricity, DXF distinct angles, and arc sweep sanity
                // (`arc_len <= 2.2 * run_len + 0.5 mm`, forbidding degenerate / 360-deg wrap arcs).
                let r_s =
                    ((start.x - center_nm.x) as f64).hypot((start.y - center_nm.y) as f64);
                let r_e = ((end.x - center_nm.x) as f64).hypot((end.y - center_nm.y) as f64);
                let r_tol = 9.5_f64.max(r_s * 1e-6);
                let sweep = arc_signed_sweep(*center_nm, start, end, angle_increasing).abs();
                let arc_len_nm = r_s * sweep;
                let chord_nm =
                    ((end.x - start.x) as f64).hypot((end.y - start.y) as f64);
                let dxf_ok = dxf_arc_angles_distinct(*center_nm, start, end);

                if r_s >= 1_000.0
                    && (r_s - r_e).abs() <= r_tol
                    && dxf_ok
                    && (0.005..=TAU - 0.02).contains(&sweep)
                    && chord_nm >= 5_000.0
                    && arc_len_nm <= run_len_nm * 2.2 + 500_000.0
                {
                    elements.push(Element::new(
                        Some(id.clone()),
                        ElementKind::Arc {
                            center: *center_nm,
                            start,
                            end,
                            angle_increasing,
                        },
                    ));
                } else {
                    // Reject invalid/wrap-around arc; bridge `start -> end` cleanly and let the
                    // loop safety net verify (or replace with Clipper2's exact chords).
                    loop_approx = true;
                    elements.push(Element::new(
                        Some(format!("{id}/chord0")),
                        ElementKind::Line { start, end },
                    ));
                }
            }
        }
    }

    // Repair closure if any zero-length segment was skipped (junctions already match otherwise).
    if elements.len() >= 2 {
        let first_start = elements[0].kind.endpoints().map(|(s, _)| s);
        let last_end = elements.last().and_then(|e| e.kind.endpoints().map(|(_, t)| t));
        if let (Some(fs), Some(le)) = (first_start, last_end) {
            if fs != le {
                elements.push(Element::new(
                    Some(format!("{fallback_stem}/close")),
                    ElementKind::Line { start: le, end: fs },
                ));
            }
        }
    }

    let candidate = Loop::new(elements);
    if validate_loop_against_path(&candidate, path, post_round, loop_approx) {
        *approximate |= loop_approx;
        candidate
    } else {
        *approximate = true;
        chord_fallback_loop(path, fallback_stem)
    }
}

fn dxf_arc_angles_distinct(center: Point, start: Point, end: Point) -> bool {
    // `board-interface` writes Y-up DXF `ARC` angles rounded to 12 decimal places and rejects
    // `|end_angle - start_angle| <= 1e-9 deg` on readback.
    let deg_s = (-(start.y - center.y) as f64)
        .atan2((start.x - center.x) as f64)
        .to_degrees()
        .rem_euclid(360.0);
    let deg_e = (-(end.y - center.y) as f64)
        .atan2((end.x - center.x) as f64)
        .to_degrees()
        .rem_euclid(360.0);
    let span = (deg_e - deg_s).rem_euclid(360.0);
    span > 1e-6 && span < 360.0 - 1e-6
}

fn chord_fallback_loop(path: &Path64, fallback_stem: &str) -> Loop {
    let mut pts_nm: Vec<Point> = Vec::with_capacity(path.len());
    for q in path {
        let p = Point::new(q.x * 100, q.y * 100);
        if pts_nm.last().copied() != Some(p) {
            pts_nm.push(p);
        }
    }
    if pts_nm.len() >= 2 && pts_nm.first() == pts_nm.last() {
        pts_nm.pop();
    }
    if pts_nm.len() < 3 {
        return Loop::new(Vec::new());
    }
    let mut verts: Vec<Point> = Vec::with_capacity(pts_nm.len());
    for &p in &pts_nm {
        if verts.len() >= 2 && is_collinear_nm(verts[verts.len() - 2], verts[verts.len() - 1], p) {
            *verts.last_mut().unwrap() = p;
        } else {
            verts.push(p);
        }
    }
    if verts.len() > 3 && is_collinear_nm(verts[verts.len() - 2], verts[verts.len() - 1], verts[0])
    {
        verts.pop();
    }
    if verts.len() < 3 {
        verts = pts_nm;
    }
    let k = verts.len();
    let elements = (0..k)
        .map(|i| {
            Element::new(
                Some(format!("{fallback_stem}/e{i}")),
                ElementKind::Line {
                    start: verts[i],
                    end: verts[(i + 1) % k],
                },
            )
        })
        .collect();
    Loop::new(elements)
}

/// Loop safety net: verify `lp` satisfies all `board-interface` readback invariants, does not
/// self-intersect, has the same orientation as Clipper2's `path`, and stays within chord / round
/// tolerance of `path` in both boundary Hausdorff distance and exact symmetric-difference area.
fn validate_loop_against_path(
    lp: &Loop,
    path: &Path64,
    post_round: bool,
    loop_approx: bool,
) -> bool {
    if !lp_passes_dxf_invariants(lp) {
        return false;
    }
    let path_signed = signed_area(path);
    if path_signed.abs() < 1.0 {
        return false;
    }
    let want_pos = path_signed > 0.0;
    let is_single_circle = matches!(
        lp.elements.as_slice(),
        [Element {
            kind: ElementKind::Circle { .. },
            ..
        }]
    );
    let curve_mm = loop_to_polygon_mm(
        lp,
        0.0005,
        if is_single_circle { Some(want_pos) } else { None },
    );
    if curve_mm.len() < 3 {
        return false;
    }
    let curve_signed = poly_signed_area_mm(&curve_mm);
    if curve_signed.abs() < 1e-6 || (curve_signed > 0.0) != want_pos {
        return false;
    }
    if polygon_self_intersects(&curve_mm) {
        return false;
    }

    let path_mm: Vec<P> = path
        .iter()
        .map(|q| [q.x as f64 / SCALE, q.y as f64 / SCALE])
        .collect();
    if path_mm.len() < 3 {
        return false;
    }

    let hd = boundary_hausdorff_mm(&curve_mm, &path_mm);

    let mut curve_pos = curve_mm;
    if !want_pos {
        curve_pos.reverse();
    }
    let mut path_pos = path.clone();
    if !want_pos {
        path_pos.reverse();
    }
    let curve_p64 = from_mm(&[curve_pos]);
    let path_p64 = vec![path_pos];
    let diff1 = difference_64(&curve_p64, &path_p64, FillRule::NonZero);
    let diff2 = difference_64(&path_p64, &curve_p64, FillRule::NonZero);
    let symdiff_mm2 = (diff1.iter().map(signed_area).sum::<f64>()
        + diff2.iter().map(signed_area).sum::<f64>())
    .max(0.0)
        / (SCALE * SCALE);

    let path_area_mm2 = path_signed.abs() / (SCALE * SCALE);
    let perim_mm = loop_perimeter_mm(lp);

    if loop_approx {
        hd <= 0.015 && symdiff_mm2 <= (0.040_f64).max(1.5e-3 * path_area_mm2)
    } else if post_round {
        let ok_sagitta = hd <= 0.0070 && symdiff_mm2 <= (perim_mm * 0.0055).max(0.045);
        let ok_round = hd <= 0.018 && symdiff_mm2 <= (0.045_f64).max(1.8e-3 * path_area_mm2);
        ok_sagitta || ok_round
    } else {
        let sym_lim = (perim_mm * 0.0055).max(0.045).max(1.8e-3 * path_area_mm2);
        hd <= 0.040 && symdiff_mm2 <= sym_lim
    }
}

fn lp_passes_dxf_invariants(lp: &Loop) -> bool {
    let n = lp.elements.len();
    if n == 0 {
        return false;
    }
    if n == 1 {
        return matches!(
            lp.elements[0].kind,
            ElementKind::Circle { diameter_nm, .. } if diameter_nm > 0
        );
    }
    let mut all_lines = true;
    for i in 0..n {
        let cur = &lp.elements[i].kind;
        let next = &lp.elements[(i + 1) % n].kind;
        let (Some((_, cur_end)), Some((next_start, _))) = (cur.endpoints(), next.endpoints())
        else {
            return false;
        };
        if cur_end != next_start {
            return false;
        }
        match *cur {
            ElementKind::Line { start, end } => {
                if start == end {
                    return false;
                }
            }
            ElementKind::Arc {
                center,
                start,
                end,
                angle_increasing,
            } => {
                all_lines = false;
                if start == end || !dxf_arc_angles_distinct(center, start, end) {
                    return false;
                }
                let r_s = ((start.x - center.x) as f64).hypot((start.y - center.y) as f64);
                let r_e = ((end.x - center.x) as f64).hypot((end.y - center.y) as f64);
                let r_tol = 9.5_f64.max(r_s * 1e-6);
                if r_s < 1_000.0 || (r_s - r_e).abs() > r_tol {
                    return false;
                }
                let sweep = arc_signed_sweep(center, start, end, angle_increasing).abs();
                if !(0.005..=TAU - 0.02).contains(&sweep) {
                    return false;
                }
            }
            ElementKind::Circle { .. } => return false,
        }
    }
    if all_lines && n < 3 {
        return false;
    }
    true
}

fn polygon_self_intersects(raw: &[P]) -> bool {
    let mut pts: Vec<P> = Vec::with_capacity(raw.len());
    for &p in raw {
        if pts
            .last()
            .is_none_or(|q| (p[0] - q[0]).hypot(p[1] - q[1]) > 1e-7)
        {
            pts.push(p);
        }
    }
    if pts.len() >= 2
        && (pts[0][0] - pts[pts.len() - 1][0]).hypot(pts[0][1] - pts[pts.len() - 1][1]) <= 1e-7
    {
        pts.pop();
    }
    let m = pts.len();
    if m < 3 {
        return true;
    }
    // Check adjacent zero-angle foldbacks.
    for i in 0..m {
        let prev = pts[(i + m - 1) % m];
        let cur = pts[i];
        let next = pts[(i + 1) % m];
        let u = [cur[0] - prev[0], cur[1] - prev[1]];
        let v = [next[0] - cur[0], next[1] - cur[1]];
        let lu = u[0].hypot(u[1]);
        let lv = v[0].hypot(v[1]);
        if lu < 1e-8 || lv < 1e-8 {
            return true;
        }
        let cos_t = (u[0] * v[0] + u[1] * v[1]) / (lu * lv);
        let sin_t = (u[0] * v[1] - u[1] * v[0]).abs() / (lu * lv);
        if cos_t < -0.999_99 && sin_t < 1e-4 {
            return true;
        }
    }
    for i in 0..m {
        let a0 = pts[i];
        let a1 = pts[(i + 1) % m];
        let ax0 = a0[0].min(a1[0]) - 1e-7;
        let ax1 = a0[0].max(a1[0]) + 1e-7;
        let ay0 = a0[1].min(a1[1]) - 1e-7;
        let ay1 = a0[1].max(a1[1]) + 1e-7;
        let j_end = if i == 0 { m - 1 } else { m };
        for j in (i + 2)..j_end {
            let b0 = pts[j];
            let b1 = pts[(j + 1) % m];
            if b0[0].max(b1[0]) < ax0
                || b0[0].min(b1[0]) > ax1
                || b0[1].max(b1[1]) < ay0
                || b0[1].min(b1[1]) > ay1
            {
                continue;
            }
            if segments_intersect_mm(a0, a1, b0, b1) {
                return true;
            }
        }
    }
    false
}

fn segments_intersect_mm(a0: P, a1: P, b0: P, b1: P) -> bool {
    let orient =
        |p: P, q: P, r: P| -> f64 { (q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0]) };
    let o1 = orient(a0, a1, b0);
    let o2 = orient(a0, a1, b1);
    let o3 = orient(b0, b1, a0);
    let o4 = orient(b0, b1, a1);
    if ((o1 > 0.0 && o2 < 0.0) || (o1 < 0.0 && o2 > 0.0))
        && ((o3 > 0.0 && o4 < 0.0) || (o3 < 0.0 && o4 > 0.0))
    {
        return true;
    }
    point_seg_dist_mm(b0, a0, a1) <= 1e-7
        || point_seg_dist_mm(b1, a0, a1) <= 1e-7
        || point_seg_dist_mm(a0, b0, b1) <= 1e-7
        || point_seg_dist_mm(a1, b0, b1) <= 1e-7
}

fn boundary_hausdorff_mm(a: &[P], b: &[P]) -> f64 {
    directed_boundary_hd_mm(a, b).max(directed_boundary_hd_mm(b, a))
}

fn directed_boundary_hd_mm(src: &[P], dst: &[P]) -> f64 {
    let mut worst = 0.0_f64;
    let m = src.len();
    let d_len = dst.len();
    for i in 0..m {
        let p0 = src[i];
        let p1 = src[(i + 1) % m];
        let seg_len = (p1[0] - p0[0]).hypot(p1[1] - p0[1]);
        let steps = ((seg_len / 0.25).ceil() as usize).clamp(1, 256);
        for s in 0..steps {
            let t = (s as f64) / (steps as f64);
            let q = [p0[0] + t * (p1[0] - p0[0]), p0[1] + t * (p1[1] - p0[1])];
            let mut best = f64::INFINITY;
            for j in 0..d_len {
                let d = point_seg_dist_mm(q, dst[j], dst[(j + 1) % d_len]);
                if d < best {
                    best = d;
                    if best <= 1e-5 {
                        break;
                    }
                }
            }
            if best > worst {
                worst = best;
            }
        }
    }
    worst
}

/// Exact whole-nanometre junction between `prev` and `curr`, near Clipper2's transition point.
fn solve_junction(prev: &SegGeom, curr: &SegGeom) -> Point {
    let hint_p = curr.start_clip();
    let hint = [hint_p.x as f64, hint_p.y as f64];

    // If both segments came from `Geom::Path` and already share an exact unclipped endpoint near
    // `hint`, reuse that exact `Point` directly.
    if let (Some(p_end), Some(c_start)) = (exact_end_near(prev, hint), exact_start_near(curr, hint))
    {
        if p_end == c_start {
            return p_end;
        }
    }

    // Allow up to 0.5 mm snap along a tangent chord when an inflection chord was merged into
    // `curr` (`curr.start_clip` was one arc-chord step before the analytical tangent point).
    let max_snap_nm = 500_000.0;
    let pick_closest = |cands: Vec<[f64; 2]>| -> Option<Point> {
        cands
            .into_iter()
            .map(|p| (p, (p[0] - hint[0]).hypot(p[1] - hint[1])))
            .filter(|(_, d)| *d <= max_snap_nm)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(p, _)| Point::new(p[0].round() as i64, p[1].round() as i64))
    };

    match (prev, curr) {
        (SegGeom::Line { a_nm: a1, u_nm: u1, .. }, SegGeom::Line { a_nm: a2, u_nm: u2, .. }) => {
            if let Some(p) = intersect_lines_f64(*a1, *u1, *a2, *u2) {
                if (p[0] - hint[0]).hypot(p[1] - hint[1]) <= max_snap_nm {
                    return Point::new(p[0].round() as i64, p[1].round() as i64);
                }
            }
            hint_p
        }
        (
            SegGeom::Line { a_nm, u_nm, .. },
            SegGeom::Arc {
                center_nm, r_nm, ..
            },
        )
        | (
            SegGeom::Arc {
                center_nm, r_nm, ..
            },
            SegGeom::Line { a_nm, u_nm, .. },
        ) => {
            let c = [center_nm.x as f64, center_nm.y as f64];
            let cands = intersect_line_circle_f64(*a_nm, *u_nm, c, *r_nm, 30_000.0);
            if let Some(pt) = pick_closest(cands) {
                return pt;
            }
            // Project `hint` radially onto `Circle(center_nm, r_nm)` so concentricity holds.
            project_onto_circle(*center_nm, *r_nm, hint)
        }
        (
            SegGeom::Arc {
                center_nm: c1_nm,
                r_nm: r1,
                ..
            },
            SegGeom::Arc {
                center_nm: c2_nm,
                r_nm: r2,
                ..
            },
        ) => {
            let c1 = [c1_nm.x as f64, c1_nm.y as f64];
            let c2 = [c2_nm.x as f64, c2_nm.y as f64];
            let cands = intersect_circles_f64(c1, *r1, c2, *r2, 30_000.0);
            if let Some(pt) = pick_closest(cands) {
                return pt;
            }
            project_onto_circle(*c1_nm, *r1, hint)
        }
    }
}

fn exact_start_near(seg: &SegGeom, hint: [f64; 2]) -> Option<Point> {
    let cands: [Option<Point>; 2] = match seg {
        SegGeom::Line {
            exact_nm: Some((s, e)),
            ..
        } => [Some(*s), Some(*e)],
        SegGeom::Arc {
            exact_arc: Some((s, e, _)),
            ..
        } => [Some(*s), Some(*e)],
        _ => [None, None],
    };
    cands
        .into_iter()
        .flatten()
        .find(|p| ((p.x as f64) - hint[0]).hypot((p.y as f64) - hint[1]) <= 15_000.0)
}

fn exact_end_near(seg: &SegGeom, hint: [f64; 2]) -> Option<Point> {
    exact_start_near(seg, hint)
}

fn project_onto_circle(center_nm: Point, r_nm: f64, hint: [f64; 2]) -> Point {
    let dx = hint[0] - (center_nm.x as f64);
    let dy = hint[1] - (center_nm.y as f64);
    let d = dx.hypot(dy);
    if d < 1e-6 {
        Point::new(center_nm.x + (r_nm.round() as i64), center_nm.y)
    } else {
        Point::new(
            ((center_nm.x as f64) + r_nm * dx / d).round() as i64,
            ((center_nm.y as f64) + r_nm * dy / d).round() as i64,
        )
    }
}

/// Ensure every element across `outline`, `cutouts`, `holes` and `keepouts` has a globally unique
/// non-empty ID. When a single source primitive (e.g. `composer:board/right`) was split into `K`
/// disjoint segments by a boolean, sort those segments by canonical geometry so `#1..#K` suffixes
/// are deterministic regardless of Clipper2 vertex start order.
pub(crate) fn ensure_unique_ids(
    outline: &mut [Loop],
    cutouts: &mut [Loop],
    holes: &mut [BiHole],
    keepouts: &mut [BiKeepout],
) {
    let mut used = BTreeSet::new();

    // 1. Reserve hole IDs first (minted & persisted in `shape.json`).
    for (idx, h) in holes.iter_mut().enumerate() {
        let base = h
            .id
            .take()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| format!("composer:hole/{}", idx + 1));
        h.id = Some(claim_unique(&base, &mut used));
    }

    // 2. Group loop element references by their candidate ID across outline, cutouts and keepouts.
    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    enum Loc {
        Outline(usize, usize),
        Cutout(usize, usize),
        Keepout(usize, usize),
    }

    let mut groups: BTreeMap<String, Vec<(ElementKind, Loc)>> = BTreeMap::new();
    for (li, lp) in outline.iter().enumerate() {
        for (ei, el) in lp.elements.iter().enumerate() {
            let id = el
                .id
                .clone()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| format!("composer:outline-{li}/e{ei}"));
            groups
                .entry(id)
                .or_default()
                .push((el.kind.undirected_canonical(), Loc::Outline(li, ei)));
        }
    }
    for (li, lp) in cutouts.iter().enumerate() {
        for (ei, el) in lp.elements.iter().enumerate() {
            let id = el
                .id
                .clone()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| format!("composer:cutout-{li}/e{ei}"));
            groups
                .entry(id)
                .or_default()
                .push((el.kind.undirected_canonical(), Loc::Cutout(li, ei)));
        }
    }
    for (li, ko) in keepouts.iter().enumerate() {
        for (ei, el) in ko.boundary.elements.iter().enumerate() {
            let id = el
                .id
                .clone()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| format!("composer:keepout-{li}/e{ei}"));
            groups
                .entry(id)
                .or_default()
                .push((el.kind.undirected_canonical(), Loc::Keepout(li, ei)));
        }
    }

    for (base_id, mut occs) in groups {
        occs.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
        let multi = occs.len() > 1;
        for (k, (_, loc)) in occs.into_iter().enumerate() {
            let desired = if !multi && !used.contains(&base_id) {
                base_id.clone()
            } else {
                format!("{base_id}#{}", k + 1)
            };
            let assigned = claim_unique(&desired, &mut used);
            match loc {
                Loc::Outline(li, ei) => outline[li].elements[ei].id = Some(assigned),
                Loc::Cutout(li, ei) => cutouts[li].elements[ei].id = Some(assigned),
                Loc::Keepout(li, ei) => keepouts[li].boundary.elements[ei].id = Some(assigned),
            }
        }
    }
}

fn claim_unique(desired: &str, used: &mut BTreeSet<String>) -> String {
    if used.insert(desired.to_string()) {
        return desired.to_string();
    }
    let mut n = 2usize;
    loop {
        let cand = format!("{desired}#{n}");
        if used.insert(cand.clone()) {
            return cand;
        }
        n += 1;
    }
}
