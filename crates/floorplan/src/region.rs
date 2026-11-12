//! Regions: polygons a block's parts must stay inside (`must`), or out of (`never`). A brief's
//! isolation barrier, an amplifier over its cooling plate, a converter's secondary. Both are hard,
//! like overlap: a member on the wrong side of its region's edge is a miss.
//!
//! A courtyard's miss is the area of it on the wrong side, plus, times its half-perimeter, how far
//! its corners, side midpoints and centre lie on the wrong side. The area is exact for any polygon
//! (the polygon clipped to the courtyard); the distances give a part far from its region a slope
//! to follow back, where the area alone is flat.

use serde::{Deserialize, Serialize};

use super::{Problem, P};

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RegionKind {
    /// Its members stay inside it.
    #[default]
    Must,
    /// Its members stay out of it; with no members, every part that moves does.
    Never,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Sides {
    Front,
    Back,
    #[default]
    Both,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Region {
    pub name: String,
    /// A closed polygon, mm, in the board's frame.
    pub polygon: Vec<P>,
    #[serde(default)]
    pub kind: RegionKind,
    /// The parts it binds, by index into `Problem::parts`.
    #[serde(default)]
    pub members: Vec<usize>,
    /// Which side's parts it binds.
    #[serde(default)]
    pub side: Sides,
}

/// Each part's regions, by index into `Problem::regions`.
pub(crate) fn binds(pr: &Problem) -> Vec<Vec<usize>> {
    let mut out = vec![Vec::new(); pr.parts.len()];
    for (r, reg) in pr.regions.iter().enumerate() {
        let on_side = |i: usize| match reg.side {
            Sides::Both => true,
            Sides::Front => !pr.parts[i].back,
            Sides::Back => pr.parts[i].back,
        };
        if reg.kind == RegionKind::Never && reg.members.is_empty() {
            for (i, p) in pr.parts.iter().enumerate() {
                if p.fixed.is_none() && on_side(i) {
                    out[i].push(r);
                }
            }
        } else {
            for &i in &reg.members {
                if on_side(i) && !out[i].contains(&r) {
                    out[i].push(r);
                }
            }
        }
    }
    out
}

/// Distance from `p` to the polygon's edge: positive inside, negative outside.
pub(crate) fn signed_distance(poly: &[P], p: P) -> f64 {
    let mut inside = false;
    let mut best = f64::INFINITY;
    for k in 0..poly.len() {
        let (a, b) = (poly[k], poly[(k + 1) % poly.len()]);
        if (a.1 > p.1) != (b.1 > p.1) && p.0 < a.0 + (p.1 - a.1) * (b.0 - a.0) / (b.1 - a.1) {
            inside = !inside;
        }
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let t = (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / (dx * dx + dy * dy).max(1e-12)).clamp(0.0, 1.0);
        best = best.min((p.0 - a.0 - t * dx).hypot(p.1 - a.1 - t * dy));
    }
    if inside {
        best
    } else {
        -best
    }
}

/// The area of the polygon inside the box (the polygon clipped to it, Sutherland-Hodgman: exact
/// for any simple polygon, the box being convex).
fn area_within(poly: &[P], b: &[f64; 4]) -> f64 {
    let mut pts: Vec<P> = poly.to_vec();
    // Each side of the box: (axis, bound, keep below it).
    for (axis, bound, below) in [(0, b[0], false), (0, b[2], true), (1, b[1], false), (1, b[3], true)] {
        if pts.is_empty() {
            return 0.0;
        }
        let v = |p: &P| if axis == 0 { p.0 } else { p.1 };
        let keep = |p: &P| if below { v(p) <= bound } else { v(p) >= bound };
        let mut next = Vec::with_capacity(pts.len() + 4);
        for k in 0..pts.len() {
            let (a, c) = (pts[k], pts[(k + 1) % pts.len()]);
            if keep(&a) {
                next.push(a);
            }
            if keep(&a) != keep(&c) {
                let t = (bound - v(&a)) / (v(&c) - v(&a));
                next.push((a.0 + t * (c.0 - a.0), a.1 + t * (c.1 - a.1)));
            }
        }
        pts = next;
    }
    let mut s = 0.0;
    for k in 0..pts.len() {
        let (a, c) = (pts[k], pts[(k + 1) % pts.len()]);
        s += a.0 * c.1 - c.0 * a.1;
    }
    (s / 2.0).abs()
}

/// The polygon's bounding box.
pub(crate) fn extent(poly: &[P]) -> [f64; 4] {
    poly.iter().fold([f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY], |e, p| [e[0].min(p.0), e[1].min(p.1), e[2].max(p.0), e[3].max(p.1)])
}

/// Corners, side midpoints and centre of a box.
pub(crate) fn samples(b: &[f64; 4]) -> [P; 9] {
    let (mx, my) = ((b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0);
    [(b[0], b[1]), (b[2], b[1]), (b[2], b[3]), (b[0], b[3]), (mx, b[1]), (mx, b[3]), (b[0], my), (b[2], my), (mx, my)]
}

/// How much a courtyard misses its region (0 where it keeps it).
pub(crate) fn miss(r: &Region, b: &[f64; 4]) -> f64 {
    let e = extent(&r.polygon);
    let apart = b[2] <= e[0] || b[0] >= e[2] || b[3] <= e[1] || b[1] >= e[3];
    let hp = (b[2] - b[0]) + (b[3] - b[1]);
    match r.kind {
        RegionKind::Never => {
            if apart {
                return 0.0;
            }
            let deep: f64 = samples(b).iter().map(|&p| signed_distance(&r.polygon, p).max(0.0)).sum();
            area_within(&r.polygon, b) + deep * hp
        }
        RegionKind::Must => {
            let short: f64 = samples(b).iter().map(|&p| (-signed_distance(&r.polygon, p)).max(0.0)).sum();
            let inside = if apart { 0.0 } else { area_within(&r.polygon, b) };
            let out = ((b[2] - b[0]) * (b[3] - b[1]) - inside).max(0.0);
            // Rounding leaves a sliver on a courtyard flush with the edge.
            let out = if out < 1e-9 { 0.0 } else { out };
            out + short * hp
        }
    }
}

/// A point inside the polygon, by chance (its centre of box when chance finds none).
pub(crate) fn point_in(poly: &[P], mut f: impl FnMut() -> f64) -> P {
    let e = extent(poly);
    for _ in 0..200 {
        let p = (e[0] + f() * (e[2] - e[0]), e[1] + f() * (e[3] - e[1]));
        if signed_distance(poly, p) > 0.0 {
            return p;
        }
    }
    ((e[0] + e[2]) / 2.0, (e[1] + e[3]) / 2.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<P> {
        vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
    }

    #[test]
    fn a_must_miss_is_zero_inside_and_grows_with_distance() {
        let r = Region { name: "a".into(), polygon: square(0.0, 0.0, 10.0, 10.0), kind: RegionKind::Must, members: vec![], side: Sides::Both };
        assert_eq!(miss(&r, &[1.0, 1.0, 3.0, 3.0]), 0.0);
        assert_eq!(miss(&r, &[0.0, 0.0, 10.0, 10.0]), 0.0);
        let half = miss(&r, &[9.0, 1.0, 11.0, 3.0]);
        let far = miss(&r, &[20.0, 1.0, 22.0, 3.0]);
        let farther = miss(&r, &[30.0, 1.0, 32.0, 3.0]);
        assert!(half > 0.0 && far > half && farther > far, "{half} {far} {farther}");
    }

    #[test]
    fn a_never_region_catches_a_part_straddling_a_thin_strip() {
        // A 0.5 mm strip no sample point of the 3 mm part lands in.
        let r = Region { name: "barrier".into(), polygon: square(4.75, -10.0, 5.25, 10.0), kind: RegionKind::Never, members: vec![], side: Sides::Both };
        let m = miss(&r, &[3.0, -1.0, 6.0, 1.0]);
        assert!((m - 1.0).abs() < 1e-9, "{m}");
        assert_eq!(miss(&r, &[5.25, -1.0, 9.0, 1.0]), 0.0);
    }

    #[test]
    fn concave_regions_are_measured_exactly() {
        // An L: the courtyard sits in its notch.
        let l = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 4.0), (4.0, 4.0), (4.0, 10.0), (0.0, 10.0)];
        let r = Region { name: "l".into(), polygon: l, kind: RegionKind::Must, members: vec![], side: Sides::Both };
        assert_eq!(miss(&r, &[1.0, 1.0, 3.0, 9.0]), 0.0);
        assert!(miss(&r, &[5.0, 5.0, 7.0, 7.0]) > 4.0);
        assert!((area_within(&r.polygon, &[2.0, 2.0, 6.0, 6.0]) - 12.0).abs() < 1e-9);
    }
}
