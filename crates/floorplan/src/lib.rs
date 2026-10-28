//! The floorplanner: every part's position and quarter turn, found by simulated annealing.
//!
//! The cost of a placement is:
//! - **wire**: each net's half-perimeter over its pads, weighted (power and ground nets are left
//!   out by the caller; they reach everything);
//! - **overlap**: the area where courtyards, grown by half the clearance each, overlap; its weight
//!   rises through the run, so parts pass through each other early and settle apart late;
//! - **area**: the bounding box of every courtyard, which becomes the board outline;
//! - **near**: a part's distance from the pad it serves (a decoupling capacitor and its pin);
//! - **block**: each part's distance from its block's centre, so blocks stay together;
//! - **edge**: a connector's distance from the board edge, and whether it faces outward;
//! - **separation**: how far two blocks that must stay apart (an aggressor and a victim) fall
//!   short of their distance.
//!
//! A rigid group (a generated block layout, in its anchor's frame) moves and turns as one.
//! Rotations follow KiCad: positive is counter-clockwise on screen, with y pointing down.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub type P = (f64, f64);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartGeom {
    pub reference: String,
    /// Courtyard bounding box at rotation 0, relative to the footprint origin.
    pub bbox: [f64; 4],
    /// Pad centres by pin, relative to the footprint origin at rotation 0.
    pub pins: BTreeMap<String, P>,
    #[serde(default)]
    pub block: Option<String>,
    /// The pad this part should sit next to, as (part index, pin).
    #[serde(default)]
    pub near: Option<(usize, String)>,
    #[serde(default)]
    pub edge: Option<Edge>,
    /// A part that does not move (a mounting hole, a connector placed by the mechanical design).
    #[serde(default)]
    pub fixed: Option<Pose>,
    /// On the bottom side: its courtyard meets only other bottom-side parts.
    #[serde(default)]
    pub back: bool,
    /// Through-hole pads (local frame, grown by their clearance): they block the other side too.
    #[serde(default)]
    pub through: Vec<[f64; 4]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Left,
    Right,
    Top,
    Bottom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    /// The side it must be on; any side when none.
    #[serde(default)]
    pub side: Option<Side>,
    /// The direction the mating face points at rotation 0 (`[0, 1]`: toward +y, down the
    /// screen). Placed, it must point out of the board.
    pub facing: P,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Net {
    pub name: String,
    pub pins: Vec<(usize, String)>,
    #[serde(default = "one")]
    pub weight: f64,
}

fn one() -> f64 {
    1.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rigid {
    pub name: String,
    pub anchor: usize,
    /// Members other than the anchor: position and rotation in the anchor's frame.
    pub members: Vec<(usize, P, f64)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Separation {
    pub a: String,
    pub b: String,
    pub min_mm: f64,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Options {
    pub seed: u64,
    pub moves: usize,
    /// Courtyard to courtyard, mm. KiCad's rule is that courtyards do not overlap: 0.
    pub clearance: f64,
    /// Courtyard to board edge, mm; an edge connector sits flush.
    pub margin: f64,
    pub w_wire: f64,
    pub w_area: f64,
    pub w_near: f64,
    pub w_block: f64,
    pub w_edge: f64,
    pub w_sep: f64,
    pub w_overlap: f64,
}

impl Default for Options {
    fn default() -> Self {
        Options { seed: 1, moves: 400_000, clearance: 0.0, margin: 0.5, w_wire: 1.0, w_area: 0.05, w_near: 3.0, w_block: 0.3, w_edge: 5.0, w_sep: 5.0, w_overlap: 20.0 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Problem {
    pub parts: Vec<PartGeom>,
    pub nets: Vec<Net>,
    #[serde(default)]
    pub groups: Vec<Rigid>,
    #[serde(default)]
    pub separations: Vec<Separation>,
    #[serde(default)]
    pub options: Options,
    /// A board outline set by the mechanical design (x0, y0, x1, y1): every courtyard stays
    /// inside it, less the margin, and edge parts sit flush to its sides. Without one, the
    /// outline is whatever the parts need.
    #[serde(default)]
    pub outline: Option<[f64; 4]>,
    /// A board outline of any shape, as a closed polygon: every courtyard stays inside it, less
    /// the margin. Checked against a signed-distance grid built once.
    #[serde(default)]
    pub outline_polygon: Option<Vec<P>>,
}

/// Signed distance to a polygon's edge on a grid (positive inside), looked up bilinearly.
struct Sdf {
    x0: f64,
    y0: f64,
    step: f64,
    nx: usize,
    ny: usize,
    d: Vec<f64>,
}

impl Sdf {
    fn new(poly: &[P], step: f64) -> Sdf {
        let (mut x0, mut y0, mut x1, mut y1) = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
        for p in poly {
            x0 = x0.min(p.0);
            y0 = y0.min(p.1);
            x1 = x1.max(p.0);
            y1 = y1.max(p.1);
        }
        let pad = 3.0;
        let (x0, y0) = (x0 - pad, y0 - pad);
        let nx = ((x1 + pad - x0) / step) as usize + 2;
        let ny = ((y1 + pad - y0) / step) as usize + 2;
        let mut d = vec![0.0; nx * ny];
        for j in 0..ny {
            for i in 0..nx {
                let (x, y) = (x0 + i as f64 * step, y0 + j as f64 * step);
                let mut inside = false;
                let mut best = f64::INFINITY;
                for k in 0..poly.len() {
                    let (a, b) = (poly[k], poly[(k + 1) % poly.len()]);
                    if (a.1 > y) != (b.1 > y) && x < a.0 + (y - a.1) * (b.0 - a.0) / (b.1 - a.1) {
                        inside = !inside;
                    }
                    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
                    let t = (((x - a.0) * dx + (y - a.1) * dy) / (dx * dx + dy * dy).max(1e-12)).clamp(0.0, 1.0);
                    best = best.min(((x - a.0 - t * dx).powi(2) + (y - a.1 - t * dy).powi(2)).sqrt());
                }
                d[j * nx + i] = if inside { best } else { -best };
            }
        }
        Sdf { x0, y0, step, nx, ny, d }
    }

    fn at(&self, x: f64, y: f64) -> f64 {
        let fx = ((x - self.x0) / self.step).clamp(0.0, (self.nx - 2) as f64);
        let fy = ((y - self.y0) / self.step).clamp(0.0, (self.ny - 2) as f64);
        let (i, j) = (fx as usize, fy as usize);
        let (u, v) = (fx - i as f64, fy - j as f64);
        let g = |a: usize, b: usize| self.d[b * self.nx + a];
        g(i, j) * (1.0 - u) * (1.0 - v) + g(i + 1, j) * u * (1.0 - v) + g(i, j + 1) * (1.0 - u) * v + g(i + 1, j + 1) * u * v
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Pose {
    pub x: f64,
    pub y: f64,
    /// Degrees.
    pub rotation: f64,
}

/// A point in a part's frame, in the board's, for a part rotated `deg`.
pub fn rot(p: P, deg: f64) -> P {
    let (s, c) = deg.to_radians().sin_cos();
    (p.0 * c + p.1 * s, -p.0 * s + p.1 * c)
}

/// A bottom-side footprint is its top-side self mirrored in y, then turned: as KiCad flips.
fn side(p: P, back: bool) -> P {
    if back {
        (p.0, -p.1)
    } else {
        p
    }
}

fn bbox_at(b: &[f64; 4], pose: &Pose, back: bool) -> [f64; 4] {
    let corners = [(b[0], b[1]), (b[2], b[1]), (b[2], b[3]), (b[0], b[3])].map(|c| side(c, back));
    let mut out = [f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY];
    for c in corners {
        let q = rot(c, pose.rotation);
        out[0] = out[0].min(pose.x + q.0);
        out[1] = out[1].min(pose.y + q.1);
        out[2] = out[2].max(pose.x + q.0);
        out[3] = out[3].max(pose.y + q.1);
    }
    out
}

fn overlap(a: &[f64; 4], b: &[f64; 4], grow: f64) -> f64 {
    let w = (a[2].min(b[2]) - a[0].max(b[0]) + grow).max(0.0);
    let h = (a[3].min(b[3]) - a[1].max(b[1]) + grow).max(0.0);
    w * h
}

fn gap(a: &[f64; 4], b: &[f64; 4]) -> f64 {
    let dx = (a[0] - b[2]).max(b[0] - a[2]).max(0.0);
    let dy = (a[1] - b[3]).max(b[1] - a[3]).max(0.0);
    (dx * dx + dy * dy).sqrt()
}

/// What moves: one part, or a rigid group by its anchor.
#[derive(Debug, Clone)]
enum Movable {
    Part(usize),
    Group(usize),
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn f(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
    fn normal(&mut self) -> f64 {
        let (u, v) = (self.f().max(1e-12), self.f());
        (-2.0 * u.ln()).sqrt() * (2.0 * std::f64::consts::PI * v).cos()
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Metrics {
    pub wire_mm: f64,
    pub board_mm: [f64; 2],
    pub area_mm2: f64,
    pub overlaps: usize,
    pub overlap_mm2: f64,
    /// The overlapping pairs (and parts outside a set outline), worst first, at most 20.
    pub overlap_pairs: Vec<String>,
    pub near_mm: f64,
    pub edge_misses: Vec<String>,
    pub separation_misses: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Placement {
    pub parts: BTreeMap<String, Pose>,
    /// Board outline, mm: x0, y0, x1, y1.
    pub outline: [f64; 4],
    /// Each block's courtyard bounding box.
    pub blocks: BTreeMap<String, [f64; 4]>,
    pub metrics: Metrics,
    pub cost: f64,
}

struct State<'a> {
    pr: &'a Problem,
    sdf: Option<Sdf>,
    /// Movable poses.
    mv: Vec<Pose>,
    movables: Vec<Movable>,
    /// Part index -> (movable, member offset, member rotation).
    of_part: Vec<(usize, P, f64)>,
    in_group: Vec<Option<usize>>,
}

impl<'a> State<'a> {
    fn part_pose(&self, i: usize) -> Pose {
        let (m, off, r) = self.of_part[i];
        let base = self.mv[m];
        let q = rot(off, base.rotation);
        Pose { x: base.x + q.0, y: base.y + q.1, rotation: (base.rotation + r).rem_euclid(360.0) }
    }

    fn pin(&self, poses: &[Pose], i: usize, pin: &str) -> P {
        let part = &self.pr.parts[i];
        let p = side(part.pins.get(pin).copied().unwrap_or((0.0, 0.0)), part.back);
        let q = rot(p, poses[i].rotation);
        (poses[i].x + q.0, poses[i].y + q.1)
    }

    /// The cost, and the metrics behind it, with the overlap weighted `w_ov`.
    fn cost(&self, w_ov: f64) -> (f64, Metrics) {
        let pr = self.pr;
        let o = &pr.options;
        let n = pr.parts.len();
        let poses: Vec<Pose> = (0..n).map(|i| self.part_pose(i)).collect();
        let boxes: Vec<[f64; 4]> = (0..n).map(|i| bbox_at(&pr.parts[i].bbox, &poses[i], pr.parts[i].back)).collect();
        let mut m = Metrics::default();
        let mut wire = 0.0;
        for net in &pr.nets {
            let mut b = [f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY];
            for (i, pin) in &net.pins {
                let p = self.pin(&poses, *i, pin);
                b = [b[0].min(p.0), b[1].min(p.1), b[2].max(p.0), b[3].max(p.1)];
            }
            if net.pins.len() > 1 {
                wire += net.weight * ((b[2] - b[0]) + (b[3] - b[1]));
                m.wire_mm += (b[2] - b[0]) + (b[3] - b[1]);
            }
        }
        let mut ov = 0.0;
        for i in 0..n {
            for j in i + 1..n {
                // Within a rigid group, or between two parts nothing here can move: not ours.
                if (self.in_group[i].is_some() && self.in_group[i] == self.in_group[j]) || (pr.parts[i].fixed.is_some() && pr.parts[j].fixed.is_some()) {
                    continue;
                }
                let a = if pr.parts[i].back != pr.parts[j].back {
                    // Opposite sides meet only where a through-hole pad comes through.
                    let thru = |k: usize, other: usize| -> f64 { pr.parts[k].through.iter().map(|t| overlap(&bbox_at(t, &poses[k], pr.parts[k].back), &boxes[other], 0.0)).sum() };
                    thru(i, j) + thru(j, i)
                } else {
                    overlap(&boxes[i], &boxes[j], o.clearance)
                };
                if a > 1e-9 {
                    ov += a;
                    m.overlaps += 1;
                    m.overlap_pairs.push(format!("{} {} {a:.3}", pr.parts[i].reference, pr.parts[j].reference));
                }
            }
        }
        m.overlap_mm2 = ov;
        let mut all = [f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY];
        for b in &boxes {
            all = [all[0].min(b[0]), all[1].min(b[1]), all[2].max(b[2]), all[3].max(b[3])];
        }
        let (bw, bh) = (all[2] - all[0] + 2.0 * o.margin, all[3] - all[1] + 2.0 * o.margin);
        m.board_mm = [bw, bh];
        m.area_mm2 = bw * bh;
        // With a set outline, what pokes out of it costs like an overlap, and the edges are its.
        let mut outside = 0.0;
        if let Some(ol) = pr.outline {
            let inner = [ol[0] + o.margin, ol[1] + o.margin, ol[2] - o.margin, ol[3] - o.margin];
            for (i, b) in boxes.iter().enumerate() {
                // An edge connector may overhang its edge (its edge term places it); a fixed part
                // is where the mechanical design put it.
                if pr.parts[i].edge.is_some() || pr.parts[i].fixed.is_some() {
                    continue;
                }
                let lim = inner;
                let d = (lim[0] - b[0]).max(0.0) + (lim[1] - b[1]).max(0.0) + (b[2] - lim[2]).max(0.0) + (b[3] - lim[3]).max(0.0);
                if d > 1e-6 {
                    outside += d * ((b[2] - b[0]) + (b[3] - b[1]));
                    m.overlaps += 1;
                    m.overlap_pairs.push(format!("{} outside {d:.3}", pr.parts[i].reference));
                }
            }
            all = ol;
            m.board_mm = [ol[2] - ol[0], ol[3] - ol[1]];
            m.area_mm2 = m.board_mm[0] * m.board_mm[1];
        }
        if let (Some(sdf), Some(poly)) = (&self.sdf, &pr.outline_polygon) {
            for (i, b) in boxes.iter().enumerate() {
                if pr.parts[i].edge.is_some() || pr.parts[i].fixed.is_some() {
                    continue;
                }
                // Corners and side midpoints, each at least the margin inside the edge.
                let (mx, my) = ((b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0);
                let pts = [(b[0], b[1]), (b[2], b[1]), (b[2], b[3]), (b[0], b[3]), (mx, b[1]), (mx, b[3]), (b[0], my), (b[2], my)];
                let d: f64 = pts.iter().map(|p| (o.margin - sdf.at(p.0, p.1)).max(0.0)).sum();
                if d > 1e-6 {
                    outside += d * ((b[2] - b[0]) + (b[3] - b[1]));
                    m.overlaps += 1;
                    m.overlap_pairs.push(format!("{} outside {d:.3}", pr.parts[i].reference));
                }
            }
            let (mut x0, mut y0, mut x1, mut y1) = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
            for p in poly {
                x0 = x0.min(p.0);
                y0 = y0.min(p.1);
                x1 = x1.max(p.0);
                y1 = y1.max(p.1);
            }
            all = [x0, y0, x1, y1];
            m.board_mm = [x1 - x0, y1 - y0];
            m.area_mm2 = m.board_mm[0] * m.board_mm[1];
        }
        let mut near = 0.0;
        for (i, p) in pr.parts.iter().enumerate() {
            if let Some((j, pin)) = &p.near {
                let t = self.pin(&poses, *j, pin);
                let c = ((boxes[i][0] + boxes[i][2]) / 2.0, (boxes[i][1] + boxes[i][3]) / 2.0);
                near += ((c.0 - t.0).powi(2) + (c.1 - t.1).powi(2)).sqrt();
            }
        }
        m.near_mm = near;
        let mut centres: BTreeMap<&str, (f64, f64, f64)> = BTreeMap::new();
        for (i, p) in pr.parts.iter().enumerate() {
            if let Some(b) = &p.block {
                let e = centres.entry(b.as_str()).or_insert((0.0, 0.0, 0.0));
                e.0 += poses[i].x;
                e.1 += poses[i].y;
                e.2 += 1.0;
            }
        }
        let mut spread = 0.0;
        for (i, p) in pr.parts.iter().enumerate() {
            if let Some(b) = &p.block {
                let c = centres[b.as_str()];
                spread += ((poses[i].x - c.0 / c.2).powi(2) + (poses[i].y - c.1 / c.2).powi(2)).sqrt();
            }
        }
        let mut edge = 0.0;
        for (i, p) in pr.parts.iter().enumerate() {
            let Some(e) = &p.edge else { continue };
            let face = rot(e.facing, poses[i].rotation);
            let b = boxes[i];
            let sides = [(Side::Left, b[0] - all[0], (-1.0, 0.0)), (Side::Right, all[2] - b[2], (1.0, 0.0)), (Side::Top, b[1] - all[1], (0.0, -1.0)), (Side::Bottom, all[3] - b[3], (0.0, 1.0))];
            // On its side: flush, or overhanging by up to 2 mm, costs nothing; facing inward costs
            // 10. Along that side it must stay within the board, like any other part.
            let best = sides
                .iter()
                .enumerate()
                .filter(|(_, (s, _, _))| e.side.is_none_or(|w| w == *s))
                .map(|(k, (_, d, n))| {
                    let along: f64 = (0..4).filter(|&j| j != k).map(|j| (-sides[j].1).max(0.0)).sum();
                    d.max(-2.0 - d).max(0.0) + 10.0 * (1.0 - (face.0 * n.0 + face.1 * n.1)) + 2.0 * along
                })
                .fold(f64::INFINITY, f64::min);
            edge += best;
            if best > 0.05 {
                m.edge_misses.push(format!("{} is {best:.2} from its edge or faces inward", p.reference));
            }
        }
        let mut sep = 0.0;
        for s in &pr.separations {
            let bb = |blk: &str| {
                let mut out = [f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY];
                for (i, p) in pr.parts.iter().enumerate() {
                    if p.block.as_deref() == Some(blk) {
                        out = [out[0].min(boxes[i][0]), out[1].min(boxes[i][1]), out[2].max(boxes[i][2]), out[3].max(boxes[i][3])];
                    }
                }
                out
            };
            let (a, b) = (bb(&s.a), bb(&s.b));
            if a[0].is_finite() && b[0].is_finite() {
                let g = gap(&a, &b);
                if g < s.min_mm {
                    sep += s.min_mm - g;
                    m.separation_misses.push(format!("{} and {} are {g:.2} mm apart, need {}", s.a, s.b, s.min_mm));
                }
            }
        }
        m.overlap_pairs.sort_by(|a, b| {
            let v = |s: &str| s.rsplit(' ').next().and_then(|x| x.parse::<f64>().ok()).unwrap_or(0.0);
            v(b).total_cmp(&v(a))
        });
        m.overlap_pairs.truncate(20);
        let cost = o.w_wire * wire + w_ov * (ov + outside) + o.w_area * m.area_mm2 + o.w_near * near + o.w_block * spread + o.w_edge * edge + o.w_sep * sep;
        (cost, m)
    }
}

pub fn solve(pr: &Problem) -> Placement {
    let n = pr.parts.len();
    let o = &pr.options;
    let mut in_group = vec![None; n];
    let mut movables = Vec::new();
    let mut of_part = vec![(usize::MAX, (0.0, 0.0), 0.0); n];
    for (g, r) in pr.groups.iter().enumerate() {
        let m = movables.len();
        movables.push(Movable::Group(g));
        in_group[r.anchor] = Some(g);
        of_part[r.anchor] = (m, (0.0, 0.0), 0.0);
        for (i, off, rr) in &r.members {
            in_group[*i] = Some(g);
            of_part[*i] = (m, *off, *rr);
        }
    }
    for i in 0..n {
        if in_group[i].is_none() {
            of_part[i] = (movables.len(), (0.0, 0.0), 0.0);
            movables.push(Movable::Part(i));
        }
    }
    let fixed = |m: &Movable| -> Option<Pose> {
        match m {
            Movable::Part(i) => pr.parts[*i].fixed,
            Movable::Group(g) => pr.parts[pr.groups[*g].anchor].fixed,
        }
    };
    let total: f64 = pr.parts.iter().map(|p| (p.bbox[2] - p.bbox[0] + o.clearance) * (p.bbox[3] - p.bbox[1] + o.clearance)).sum();
    let span = (total * 2.0).sqrt();
    let mut rng = Rng(o.seed.wrapping_mul(0x9E3779B97F4A7C15) | 1);
    let poly_box = pr.outline_polygon.as_ref().map(|p| {
        let xs = p.iter().map(|q| q.0);
        let ys = p.iter().map(|q| q.1);
        [xs.clone().fold(f64::INFINITY, f64::min), ys.clone().fold(f64::INFINITY, f64::min), xs.fold(f64::NEG_INFINITY, f64::max), ys.fold(f64::NEG_INFINITY, f64::max)]
    });
    let (cx, cy, sx, sy) = match pr.outline.or(poly_box) {
        // Start inside the middle half, so parts begin on the board.
        Some(o) => ((o[0] + o[2]) / 2.0, (o[1] + o[3]) / 2.0, (o[2] - o[0]) * 0.5, (o[3] - o[1]) * 0.5),
        None => (0.0, 0.0, span, span),
    };
    let mv: Vec<Pose> = movables.iter().map(|m| fixed(m).unwrap_or(Pose { x: cx + (rng.f() - 0.5) * sx, y: cy + (rng.f() - 0.5) * sy, rotation: 0.0 })).collect();
    let sdf = pr.outline_polygon.as_ref().map(|p| Sdf::new(p, 0.1));
    let mut st = State { pr, sdf, mv, movables, of_part, in_group };
    let free: Vec<usize> = (0..st.movables.len()).filter(|&m| fixed(&st.movables[m]).is_none()).collect();
    // An edge part with its side named, on a set outline, starts flush on that side facing out
    // and only slides along it: true for x on top and bottom, for y on left and right.
    let mut slides: BTreeMap<usize, bool> = BTreeMap::new();
    if let Some(ol) = pr.outline {
        for &m in &free {
            let Movable::Part(i) = st.movables[m] else { continue };
            let Some(e) = &pr.parts[i].edge else { continue };
            let Some(side) = e.side else { continue };
            let out = match side {
                Side::Left => (-1.0, 0.0),
                Side::Right => (1.0, 0.0),
                Side::Top => (0.0, -1.0),
                Side::Bottom => (0.0, 1.0),
            };
            let r = (0..4).map(|q| 90.0 * q as f64).find(|&r| {
                let f = rot(e.facing, r);
                (f.0 - out.0).abs() < 1e-6 && (f.1 - out.1).abs() < 1e-6
            });
            let Some(r) = r else { continue };
            let mut pose = Pose { x: (ol[0] + ol[2]) / 2.0, y: (ol[1] + ol[3]) / 2.0, rotation: r };
            let b = bbox_at(&pr.parts[i].bbox, &pose, pr.parts[i].back);
            match side {
                Side::Left => pose.x += ol[0] - b[0],
                Side::Right => pose.x += ol[2] - b[2],
                Side::Top => pose.y += ol[1] - b[1],
                Side::Bottom => pose.y += ol[3] - b[3],
            }
            st.mv[m] = pose;
            slides.insert(m, matches!(side, Side::Top | Side::Bottom));
        }
    }

    // Temperature from the spread of costs of random moves at the start.
    let (mut cur, _) = st.cost(o.w_overlap);
    let mut deltas = Vec::new();
    for _ in 0..200.min(free.len() * 10) {
        let m = free[(rng.next() as usize) % free.len()];
        let old = st.mv[m];
        st.mv[m].x += rng.normal() * span * 0.1;
        st.mv[m].y += rng.normal() * span * 0.1;
        let (c, _) = st.cost(o.w_overlap);
        deltas.push((c - cur).abs());
        st.mv[m] = old;
    }
    let t0 = if deltas.is_empty() { 1.0 } else { deltas.iter().sum::<f64>() / deltas.len() as f64 };
    let mut best = (cur, st.mv.clone());
    if !free.is_empty() {
        for k in 0..o.moves {
            let prog = k as f64 / o.moves as f64;
            let t = t0 * (1e-4f64).powf(prog);
            let w_ov = o.w_overlap * (1.0 + 200.0 * prog * prog);
            if k % 2000 == 0 {
                cur = st.cost(w_ov).0;
                best.0 = f64::INFINITY;
            }
            let m = free[(rng.next() as usize) % free.len()];
            let saved: Vec<(usize, Pose)>;
            let r = rng.f();
            if let Some(&along_x) = slides.get(&m) {
                saved = vec![(m, st.mv[m])];
                let step = (span * 0.3 * (1.0 - prog) + 0.05).max(0.05) * rng.normal();
                if along_x {
                    st.mv[m].x += step;
                } else {
                    st.mv[m].y += step;
                }
            } else if r < 0.12 {
                saved = vec![(m, st.mv[m])];
                st.mv[m].rotation = (st.mv[m].rotation + 90.0 * (1 + rng.next() % 3) as f64).rem_euclid(360.0);
            } else if r < 0.2 && free.len() > 1 && !slides.contains_key(&free[k % free.len()]) {
                let m2 = free[k % free.len()];
                saved = vec![(m, st.mv[m]), (m2, st.mv[m2])];
                let (a, b) = (st.mv[m], st.mv[m2]);
                st.mv[m].x = b.x;
                st.mv[m].y = b.y;
                st.mv[m2].x = a.x;
                st.mv[m2].y = a.y;
            } else {
                saved = vec![(m, st.mv[m])];
                let step = (span * 0.3 * (1.0 - prog) + 0.05).max(0.05);
                st.mv[m].x += rng.normal() * step;
                st.mv[m].y += rng.normal() * step;
            }
            let (c, _) = st.cost(w_ov);
            if c <= cur || rng.f() < (-(c - cur) / t).exp() {
                cur = c;
                if prog > 0.9 && c < best.0 {
                    best = (c, st.mv.clone());
                }
            } else {
                for (i, p) in saved {
                    st.mv[i] = p;
                }
            }
        }
        if best.0.is_finite() {
            st.mv = best.1.clone();
        }
    }
    // A part on its edge stays there; what overlaps it moves.
    let pushable: Vec<usize> = free.iter().copied().filter(|m| !slides.contains_key(m)).collect();
    legalise(&mut st, &pushable);

    let (cost, metrics) = st.cost(o.w_overlap * 201.0);
    let poses: Vec<Pose> = (0..n).map(|i| st.part_pose(i)).collect();
    let boxes: Vec<[f64; 4]> = (0..n).map(|i| bbox_at(&pr.parts[i].bbox, &poses[i], pr.parts[i].back)).collect();
    let mut all = [f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY];
    let mut blocks: BTreeMap<String, [f64; 4]> = BTreeMap::new();
    for (i, b) in boxes.iter().enumerate() {
        all = [all[0].min(b[0]), all[1].min(b[1]), all[2].max(b[2]), all[3].max(b[3])];
        if let Some(k) = &pr.parts[i].block {
            let e = blocks.entry(k.clone()).or_insert([f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY]);
            *e = [e[0].min(b[0]), e[1].min(b[1]), e[2].max(b[2]), e[3].max(b[3])];
        }
    }
    // Edge connectors sit flush: the outline is the courtyards' box grown by the margin, except
    // on a side an edge part touches.
    let mut outline = [all[0] - o.margin, all[1] - o.margin, all[2] + o.margin, all[3] + o.margin];
    for (i, p) in pr.parts.iter().enumerate().filter(|_| pr.outline.is_none()) {
        if p.edge.is_some() {
            let b = boxes[i];
            for (k, (v, lim)) in [(b[0], all[0]), (b[1], all[1]), (b[2], all[2]), (b[3], all[3])].into_iter().enumerate() {
                if (v - lim).abs() < 1e-6 {
                    outline[k] = lim;
                }
            }
        }
    }
    if let Some(ol) = pr.outline.or(poly_box) {
        outline = ol;
    }
    let parts = pr.parts.iter().enumerate().map(|(i, p)| (p.reference.clone(), round_pose(poses[i]))).collect();
    let mut metrics = metrics;
    metrics.board_mm = [outline[2] - outline[0], outline[3] - outline[1]];
    metrics.area_mm2 = metrics.board_mm[0] * metrics.board_mm[1];
    Placement { parts, outline, blocks, metrics, cost }
}

/// The metrics of a given placement (a person's, to compare with), every part where `poses`
/// puts it; rigid groups are not enforced.
pub fn evaluate(pr: &Problem, poses: &BTreeMap<String, Pose>) -> Result<Metrics, String> {
    let n = pr.parts.len();
    let mut mv = Vec::with_capacity(n);
    for p in &pr.parts {
        mv.push(*poses.get(&p.reference).ok_or_else(|| format!("{} has no pose", p.reference))?);
    }
    let sdf = pr.outline_polygon.as_ref().map(|p| Sdf::new(p, 0.1));
    let st = State { pr, sdf, mv, movables: (0..n).map(Movable::Part).collect(), of_part: (0..n).map(|i| (i, (0.0, 0.0), 0.0)).collect(), in_group: vec![None; n] };
    Ok(st.cost(pr.options.w_overlap).1)
}

fn round_pose(p: Pose) -> Pose {
    let r = |v: f64| (v * 1000.0).round() / 1000.0;
    Pose { x: r(p.x), y: r(p.y), rotation: r(p.rotation) }
}

/// Push overlapping movables apart along their shallower axis until none overlap.
fn legalise(st: &mut State, free: &[usize]) {
    let pr = st.pr;
    let n = pr.parts.len();
    let grow = pr.options.clearance;
    for _ in 0..500 {
        let poses: Vec<Pose> = (0..n).map(|i| st.part_pose(i)).collect();
        let boxes: Vec<[f64; 4]> = (0..n).map(|i| bbox_at(&pr.parts[i].bbox, &poses[i], pr.parts[i].back)).collect();
        let mut moved = false;
        for i in 0..n {
            for j in i + 1..n {
                if st.in_group[i].is_some() && st.in_group[i] == st.in_group[j] {
                    continue;
                }
                // Opposite sides: the through-hole pad that collides stands in for its part.
                let (bi, bj) = if pr.parts[i].back != pr.parts[j].back {
                    let hit = |k: usize, other: usize| pr.parts[k].through.iter().map(|t| bbox_at(t, &poses[k], pr.parts[k].back)).find(|t| overlap(t, &boxes[other], 0.0) > 1e-9);
                    match (hit(i, j), hit(j, i)) {
                        (Some(t), _) => (t, boxes[j]),
                        (_, Some(t)) => (boxes[i], t),
                        _ => continue,
                    }
                } else if overlap(&boxes[i], &boxes[j], grow) > 1e-9 {
                    (boxes[i], boxes[j])
                } else {
                    continue;
                };
                let (a, b) = (&bi, &bj);
                let px = (a[2].min(b[2]) - a[0].max(b[0])) + grow + 1e-3;
                let py = (a[3].min(b[3]) - a[1].max(b[1])) + grow + 1e-3;
                let (mi, mj) = (st.of_part[i].0, st.of_part[j].0);
                // Move the free one, the smaller when both are.
                let area = |k: usize| (boxes[k][2] - boxes[k][0]) * (boxes[k][3] - boxes[k][1]);
                let (m, me_is_i) = match (free.contains(&mi), free.contains(&mj)) {
                    (true, true) if area(i) < area(j) => (mi, true),
                    (true, _) if !free.contains(&mj) => (mi, true),
                    (_, true) => (mj, false),
                    _ => continue,
                };
                let (bme, bot) = if me_is_i { (bi, bj) } else { (bj, bi) };
                let (ca, cb) = (((bme[0] + bme[2]) / 2.0, (bme[1] + bme[3]) / 2.0), ((bot[0] + bot[2]) / 2.0, (bot[1] + bot[3]) / 2.0));
                if px < py {
                    st.mv[m].x += if ca.0 >= cb.0 { px } else { -px };
                } else {
                    st.mv[m].y += if ca.1 >= cb.1 { py } else { -py };
                }
                moved = true;
                break;
            }
            if moved {
                break;
            }
        }
        if !moved {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part(r: &str, w: f64, h: f64) -> PartGeom {
        let pins = [("1".to_string(), (-w / 4.0, 0.0)), ("2".to_string(), (w / 4.0, 0.0))].into();
        PartGeom { reference: r.into(), bbox: [-w / 2.0, -h / 2.0, w / 2.0, h / 2.0], pins, block: None, near: None, edge: None, fixed: None, back: false, through: vec![] }
    }

    #[test]
    fn a_chain_packs_without_overlap() {
        let parts: Vec<_> = (0..12).map(|i| part(&format!("R{i}"), 1.6, 0.8)).collect();
        let nets = (0..11).map(|i| Net { name: format!("N{i}"), pins: vec![(i, "2".into()), (i + 1, "1".into())], weight: 1.0 }).collect();
        let pr = Problem { parts, nets, groups: vec![], separations: vec![], options: Options { moves: 60_000, ..Default::default() }, outline: None, outline_polygon: None };
        let p = solve(&pr);
        assert_eq!(p.metrics.overlaps, 0, "{:?}", p.metrics);
        // Twelve 1.6 x 0.8 parts (with clearance, 1.8 x 1.0 each) take about 22 mm²; a sane
        // placement stays within a few times that.
        assert!(p.metrics.area_mm2 < 70.0, "{:?}", p.metrics);
    }

    #[test]
    fn rigid_groups_keep_their_shape_and_edges_face_out() {
        let mut parts = vec![part("U1", 3.0, 3.0), part("C1", 1.0, 0.5), part("J1", 4.0, 2.0)];
        parts[2].edge = Some(Edge { side: None, facing: (0.0, 1.0) });
        let nets = vec![Net { name: "A".into(), pins: vec![(0, "1".into()), (2, "1".into())], weight: 1.0 }];
        let groups = vec![Rigid { name: "core".into(), anchor: 0, members: vec![(1, (0.0, -2.5), 0.0)] }];
        let pr = Problem { parts, nets, groups, separations: vec![], options: Options { moves: 40_000, ..Default::default() }, outline: None, outline_polygon: None };
        let p = solve(&pr);
        let (u, c) = (p.parts["U1"], p.parts["C1"]);
        let off = rot((0.0, -2.5), u.rotation);
        assert!((c.x - u.x - off.0).abs() < 1e-3 && (c.y - u.y - off.1).abs() < 1e-3, "{u:?} {c:?}");
        assert!(p.metrics.edge_misses.is_empty(), "{:?}", p.metrics);
        assert_eq!(p.metrics.overlaps, 0);
    }

    #[test]
    fn a_polygon_outline_holds_the_parts() {
        // A triangle: twelve small parts must all end up inside it.
        let parts: Vec<_> = (0..12).map(|i| part(&format!("R{i}"), 1.0, 0.5)).collect();
        let nets = (0..11).map(|i| Net { name: format!("N{i}"), pins: vec![(i, "2".into()), (i + 1, "1".into())], weight: 1.0 }).collect();
        let tri = vec![(0.0, 0.0), (20.0, 0.0), (10.0, 15.0)];
        let pr = Problem { parts, nets, groups: vec![], separations: vec![], options: Options { moves: 60_000, ..Default::default() }, outline: None, outline_polygon: Some(tri) };
        let p = solve(&pr);
        assert_eq!(p.metrics.overlaps, 0, "{:?}", p.metrics);
    }

    #[test]
    fn separated_blocks_stay_apart() {
        let mut parts: Vec<_> = (0..4).map(|i| part(&format!("P{i}"), 1.0, 1.0)).collect();
        for (i, p) in parts.iter_mut().enumerate() {
            p.block = Some(if i < 2 { "sw".into() } else { "xtal".into() });
        }
        let nets = vec![Net { name: "A".into(), pins: vec![(0, "1".into()), (3, "1".into())], weight: 1.0 }];
        let seps = vec![Separation { a: "sw".into(), b: "xtal".into(), min_mm: 5.0, note: None }];
        let pr = Problem { parts, nets, groups: vec![], separations: seps, options: Options { moves: 40_000, ..Default::default() }, outline: None, outline_polygon: None };
        let p = solve(&pr);
        assert!(p.metrics.separation_misses.is_empty(), "{:?}", p.metrics);
    }
}
