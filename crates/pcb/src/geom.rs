//! Plane geometry in millimetres, KiCad's frame (Y down).

pub type P = [f64; 2];

pub fn sub(a: P, b: P) -> P {
    [a[0] - b[0], a[1] - b[1]]
}

pub fn add(a: P, b: P) -> P {
    [a[0] + b[0], a[1] + b[1]]
}

pub fn scale(a: P, k: f64) -> P {
    [a[0] * k, a[1] * k]
}

pub fn dot(a: P, b: P) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

pub fn cross(a: P, b: P) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}

pub fn dist(a: P, b: P) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

/// `p` rotated by `deg` about the origin, as KiCad rotates: positive angles turn
/// counter-clockwise on screen, which with Y down is clockwise in the maths frame.
pub fn rotate(p: P, deg: f64) -> P {
    let (s, c) = (-deg.to_radians()).sin_cos();
    [p[0] * c - p[1] * s, p[0] * s + p[1] * c]
}

/// Signed area by the shoelace formula; the sign depends on winding.
pub fn signed_area(poly: &[P]) -> f64 {
    let n = poly.len();
    (0..n).map(|i| cross(poly[i], poly[(i + 1) % n])).sum::<f64>() / 2.0
}

pub fn area(poly: &[P]) -> f64 {
    signed_area(poly).abs()
}

/// Winding number of a closed polyline around `p`.
pub fn winding(poly: &[P], p: P) -> i32 {
    let n = poly.len();
    let mut w = 0;
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        let side = cross(sub(b, a), sub(p, a));
        if a[1] <= p[1] {
            if b[1] > p[1] && side > 0.0 {
                w += 1;
            }
        } else if b[1] <= p[1] && side < 0.0 {
            w -= 1;
        }
    }
    w
}

/// The area a current loop encloses: the integral of |winding number|, so a loop that
/// crosses itself counts both lobes instead of cancelling them as the shoelace sum does.
/// Sampled on a grid of about `cells` cells along the longer side.
pub fn loop_area(poly: &[P], cells: usize) -> f64 {
    let Some((lo, hi)) = bbox(poly.iter().copied()) else { return 0.0 };
    let step = (hi[0] - lo[0]).max(hi[1] - lo[1]) / cells as f64;
    if step <= 0.0 {
        return 0.0;
    }
    let (nx, ny) = (((hi[0] - lo[0]) / step).ceil() as usize, ((hi[1] - lo[1]) / step).ceil() as usize);
    let mut sum = 0i64;
    for j in 0..ny {
        for i in 0..nx {
            sum += winding(poly, [lo[0] + (i as f64 + 0.5) * step, lo[1] + (j as f64 + 0.5) * step]).abs() as i64;
        }
    }
    sum as f64 * step * step
}

pub fn centroid(poly: &[P]) -> P {
    let n = poly.len().max(1) as f64;
    let s = poly.iter().fold([0.0, 0.0], |acc, p| add(acc, *p));
    scale(s, 1.0 / n)
}

pub fn bbox(points: impl IntoIterator<Item = P>) -> Option<(P, P)> {
    let mut it = points.into_iter();
    let first = it.next()?;
    Some(it.fold((first, first), |(lo, hi), p| ([lo[0].min(p[0]), lo[1].min(p[1])], [hi[0].max(p[0]), hi[1].max(p[1])])))
}

/// Even-odd point in polygon.
pub fn inside(poly: &[P], p: P) -> bool {
    let n = poly.len();
    let mut c = false;
    let mut j = n.wrapping_sub(1);
    for i in 0..n {
        let (a, b) = (poly[i], poly[j]);
        if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0] {
            c = !c;
        }
        j = i;
    }
    c
}

pub fn point_seg_dist(p: P, a: P, b: P) -> f64 {
    let ab = sub(b, a);
    let len2 = dot(ab, ab);
    let t = if len2 == 0.0 { 0.0 } else { (dot(sub(p, a), ab) / len2).clamp(0.0, 1.0) };
    dist(p, add(a, scale(ab, t)))
}

fn segs_cross(a: P, b: P, c: P, d: P) -> bool {
    let d1 = cross(sub(b, a), sub(c, a));
    let d2 = cross(sub(b, a), sub(d, a));
    let d3 = cross(sub(d, c), sub(a, c));
    let d4 = cross(sub(d, c), sub(b, c));
    (d1 > 0.0) != (d2 > 0.0) && (d3 > 0.0) != (d4 > 0.0)
}

pub fn seg_seg_dist(a: P, b: P, c: P, d: P) -> f64 {
    if segs_cross(a, b, c, d) {
        return 0.0;
    }
    point_seg_dist(a, c, d).min(point_seg_dist(b, c, d)).min(point_seg_dist(c, a, b)).min(point_seg_dist(d, a, b))
}

fn edges(poly: &[P]) -> impl Iterator<Item = (P, P)> + '_ {
    (0..poly.len()).map(move |i| (poly[i], poly[(i + 1) % poly.len()]))
}

/// Gap between two filled polygons: 0 when they touch, overlap or one holds the other.
pub fn poly_poly_dist(a: &[P], b: &[P]) -> f64 {
    if a.is_empty() || b.is_empty() {
        return f64::INFINITY;
    }
    if inside(a, b[0]) || inside(b, a[0]) {
        return 0.0;
    }
    let mut best = f64::INFINITY;
    for (p, q) in edges(a) {
        for (r, s) in edges(b) {
            best = best.min(seg_seg_dist(p, q, r, s));
            if best == 0.0 {
                return 0.0;
            }
        }
    }
    best
}

/// Gap between a point and a filled polygon: 0 inside.
pub fn point_poly_dist(p: P, poly: &[P]) -> f64 {
    if inside(poly, p) {
        return 0.0;
    }
    edges(poly).map(|(a, b)| point_seg_dist(p, a, b)).fold(f64::INFINITY, f64::min)
}

/// Gap between a polyline (zero width) and a filled polygon. A single point is measured too.
pub fn polyline_poly_dist(line: &[P], poly: &[P]) -> f64 {
    if line.iter().any(|p| inside(poly, *p)) {
        return 0.0;
    }
    if line.len() == 1 {
        return point_poly_dist(line[0], poly);
    }
    let mut best = f64::INFINITY;
    for w in line.windows(2) {
        for (r, s) in edges(poly) {
            best = best.min(seg_seg_dist(w[0], w[1], r, s));
        }
    }
    best
}

pub fn circle(c: P, r: f64, n: usize) -> Vec<P> {
    (0..n)
        .map(|i| {
            let t = i as f64 / n as f64 * std::f64::consts::TAU;
            [c[0] + r * t.cos(), c[1] + r * t.sin()]
        })
        .collect()
}

/// A track's copper: the segment swept by a disc of diameter `w`.
pub fn capsule(a: P, b: P, w: f64) -> Vec<P> {
    let r = w / 2.0;
    let d = sub(b, a);
    let len = dot(d, d).sqrt();
    if len < 1e-9 {
        return circle(a, r, 16);
    }
    let base = d[1].atan2(d[0]);
    let mut out = Vec::with_capacity(18);
    for i in 0..=8 {
        let t = base - std::f64::consts::FRAC_PI_2 + std::f64::consts::PI * i as f64 / 8.0;
        out.push([b[0] + r * t.cos(), b[1] + r * t.sin()]);
    }
    for i in 0..=8 {
        let t = base + std::f64::consts::FRAC_PI_2 + std::f64::consts::PI * i as f64 / 8.0;
        out.push([a[0] + r * t.cos(), a[1] + r * t.sin()]);
    }
    out
}

/// Area of the union of polygons, by sampling on a `step` grid. Exact enough for copper areas
/// of a few mm² at 10 µm.
pub fn union_area(polys: &[Vec<P>], step: f64) -> f64 {
    overlap_area(polys, None, step)
}

/// Area covered by the union of `a`, and also by the union of `b` when given.
pub fn overlap_area(a: &[Vec<P>], b: Option<&[Vec<P>]>, step: f64) -> f64 {
    let Some((lo, hi)) = bbox(a.iter().flatten().copied()) else { return 0.0 };
    let boxes = |ps: &[Vec<P>]| -> Vec<(P, P)> { ps.iter().map(|p| bbox(p.iter().copied()).unwrap_or(([0.0; 2], [0.0; 2]))).collect() };
    let (ba, bb) = (boxes(a), b.map(boxes).unwrap_or_default());
    let hit = |ps: &[Vec<P>], bs: &[(P, P)], q: P| {
        ps.iter().zip(bs).any(|(p, (l, h))| l[0] <= q[0] && q[0] <= h[0] && l[1] <= q[1] && q[1] <= h[1] && inside(p, q))
    };
    let (nx, ny) = (((hi[0] - lo[0]) / step).ceil() as usize, ((hi[1] - lo[1]) / step).ceil() as usize);
    let mut count = 0usize;
    for j in 0..ny {
        let y = lo[1] + (j as f64 + 0.5) * step;
        for i in 0..nx {
            let q = [lo[0] + (i as f64 + 0.5) * step, y];
            if hit(a, &ba, q) && b.is_none_or(|b| hit(b, &bb, q)) {
                count += 1;
            }
        }
    }
    count as f64 * step * step
}

/// Convex hull, counter-clockwise in the maths frame (monotone chain).
pub fn hull(points: &[P]) -> Vec<P> {
    let mut pts = points.to_vec();
    pts.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    pts.dedup();
    if pts.len() < 3 {
        return pts;
    }
    let mut lower: Vec<P> = Vec::new();
    for &p in &pts {
        while lower.len() >= 2 && cross(sub(lower[lower.len() - 1], lower[lower.len() - 2]), sub(p, lower[lower.len() - 2])) <= 0.0 {
            lower.pop();
        }
        lower.push(p);
    }
    let mut upper: Vec<P> = Vec::new();
    for &p in pts.iter().rev() {
        while upper.len() >= 2 && cross(sub(upper[upper.len() - 1], upper[upper.len() - 2]), sub(p, upper[upper.len() - 2])) <= 0.0 {
            upper.pop();
        }
        upper.push(p);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

/// Whether two convex polygons overlap by more than `eps` (touching is not overlap), by the
/// separating axis theorem.
pub fn convex_overlap(a: &[P], b: &[P], eps: f64) -> bool {
    for poly in [a, b] {
        let n = poly.len();
        for i in 0..n {
            let e = sub(poly[(i + 1) % n], poly[i]);
            let axis = [-e[1], e[0]];
            let len = dot(axis, axis).sqrt();
            if len < 1e-12 {
                continue;
            }
            let axis = scale(axis, 1.0 / len);
            let span = |p: &[P]| p.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), q| (lo.min(dot(*q, axis)), hi.max(dot(*q, axis))));
            let (alo, ahi) = span(a);
            let (blo, bhi) = span(b);
            if ahi - blo <= eps || bhi - alo <= eps {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn square_area() {
        let sq = vec![[0.0, 0.0], [2.0, 0.0], [2.0, 1.0], [0.0, 1.0]];
        assert!((area(&sq) - 2.0).abs() < 1e-12);
        let both = union_area(&[sq.clone(), vec![[1.0, 0.0], [3.0, 0.0], [3.0, 1.0], [1.0, 1.0]]], 0.01);
        assert!((both - 3.0).abs() < 0.02, "{both}");
    }

    #[test]
    fn distances() {
        let a = vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        let b: Vec<P> = a.iter().map(|p| [p[0] + 3.0, p[1]]).collect();
        assert!((poly_poly_dist(&a, &b) - 2.0).abs() < 1e-12);
        assert_eq!(poly_poly_dist(&a, &a), 0.0);
        assert!((polyline_poly_dist(&[[0.5, 3.0], [0.5, 5.0]], &a) - 2.0).abs() < 1e-12);
    }

    #[test]
    fn figure_eight() {
        // Two unit squares joined at a corner, traversed as one crossing loop.
        let eight = vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [2.0, 1.0], [2.0, 2.0], [1.0, 2.0], [1.0, 1.0], [0.0, 1.0]];
        let bow = vec![[0.0, 0.0], [2.0, 2.0], [2.0, 0.0], [0.0, 2.0]];
        assert!(area(&bow) < 1e-9);
        assert!((loop_area(&bow, 400) - 2.0).abs() < 0.03, "{}", loop_area(&bow, 400));
        assert!((loop_area(&eight, 400) - 2.0).abs() < 0.03);
    }

    #[test]
    fn sat() {
        let a = vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        let touching: Vec<P> = a.iter().map(|p| [p[0] + 1.0, p[1]]).collect();
        let over: Vec<P> = a.iter().map(|p| [p[0] + 0.5, p[1] + 0.5]).collect();
        assert!(!convex_overlap(&a, &touching, 1e-9));
        assert!(convex_overlap(&a, &over, 1e-9));
    }

    #[test]
    fn kicad_rotation() {
        // KiCad: a footprint at 90° puts its local +X pad above the centre (−Y on the board).
        let p = rotate([1.0, 0.0], 90.0);
        assert!((p[0]).abs() < 1e-12 && (p[1] + 1.0).abs() < 1e-12, "{p:?}");
    }

    #[test]
    fn capsule_area() {
        let c = capsule([0.0, 0.0], [2.0, 0.0], 0.2);
        let expect = 2.0 * 0.2 + std::f64::consts::PI * 0.01;
        assert!((area(&c) - expect).abs() < 0.002, "{}", area(&c));
    }
}
