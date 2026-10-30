//! The anneal's cost, kept up to date move by move. `State::cost` rebuilds every pose and box and
//! checks every pair of parts: O(n²) a move. A move changes one movable (or two, in a swap), so
//! here only its parts' boxes, the nets on their pins and their overlaps with the rest are
//! re-scored: O(n) a move. The terms that depend on the whole board (its extent, the edge
//! connectors against it, block separations) are recomputed each move; they are O(n) or less.
//! `State::cost` stays the reference: it makes the metrics, and the anneal re-syncs this cache
//! from it now and then so rounding cannot drift.

use std::collections::BTreeSet;

use super::{bbox_at, gap, overlap, rot, side, Pose, Side, State, P};

/// What a move changed, to undo it.
struct Undo {
    parts: Vec<(usize, Pose, [f64; 4], f64, f64)>,
    nets: Vec<(usize, f64, f64)>,
    blocks: Vec<(usize, (f64, f64, f64), f64)>,
    ov: f64,
    outside: f64,
    near: f64,
    spread: f64,
    wire: f64,
    wire_mm: f64,
}

pub(crate) struct Inc {
    poses: Vec<Pose>,
    boxes: Vec<[f64; 4]>,
    /// Each net's weighted and plain half-perimeter.
    net: Vec<(f64, f64)>,
    wire: f64,
    wire_mm: f64,
    ov: f64,
    out_part: Vec<f64>,
    outside: f64,
    near_part: Vec<f64>,
    near: f64,
    /// Each block's member sums (x, y, count) and spread.
    block_sum: Vec<(f64, f64, f64)>,
    block_spread: Vec<f64>,
    spread: f64,
    // Fixed structure.
    part_nets: Vec<Vec<usize>>,
    near_of: Vec<Vec<usize>>,
    block_of: Vec<Option<usize>>,
    block_members: Vec<Vec<usize>>,
    block_names: Vec<String>,
    movable_parts: Vec<Vec<usize>>,
    edge_parts: Vec<usize>,
    undo: Option<Undo>,
}

impl Inc {
    pub(crate) fn new(st: &State) -> Inc {
        let pr = st.pr;
        let n = pr.parts.len();
        let mut part_nets = vec![Vec::new(); n];
        for (k, net) in pr.nets.iter().enumerate() {
            for (i, _) in &net.pins {
                if !part_nets[*i].contains(&k) {
                    part_nets[*i].push(k);
                }
            }
        }
        let mut near_of = vec![Vec::new(); n];
        for (i, p) in pr.parts.iter().enumerate() {
            if let Some((j, _)) = &p.near {
                near_of[*j].push(i);
            }
        }
        let block_names: Vec<String> = pr.parts.iter().filter_map(|p| p.block.clone()).collect::<BTreeSet<_>>().into_iter().collect();
        let block_of: Vec<Option<usize>> = pr.parts.iter().map(|p| p.block.as_ref().map(|b| block_names.binary_search(b).unwrap())).collect();
        let mut block_members = vec![Vec::new(); block_names.len()];
        for (i, b) in block_of.iter().enumerate() {
            if let Some(b) = b {
                block_members[*b].push(i);
            }
        }
        let mut movable_parts = vec![Vec::new(); st.movables.len()];
        for i in 0..n {
            movable_parts[st.of_part[i].0].push(i);
        }
        let edge_parts = (0..n).filter(|&i| pr.parts[i].edge.is_some()).collect();
        let mut inc = Inc {
            poses: Vec::new(),
            boxes: Vec::new(),
            net: vec![(0.0, 0.0); pr.nets.len()],
            wire: 0.0,
            wire_mm: 0.0,
            ov: 0.0,
            out_part: vec![0.0; n],
            outside: 0.0,
            near_part: vec![0.0; n],
            near: 0.0,
            block_sum: vec![(0.0, 0.0, 0.0); block_names.len()],
            block_spread: vec![0.0; block_names.len()],
            spread: 0.0,
            part_nets,
            near_of,
            block_of,
            block_members,
            block_names,
            movable_parts,
            edge_parts,
            undo: None,
        };
        inc.sync(st);
        inc
    }

    /// Every term from scratch.
    pub(crate) fn sync(&mut self, st: &State) {
        let pr = st.pr;
        let n = pr.parts.len();
        self.poses = (0..n).map(|i| st.part_pose(i)).collect();
        self.boxes = (0..n).map(|i| bbox_at(&pr.parts[i].bbox, &self.poses[i], pr.parts[i].back)).collect();
        for k in 0..pr.nets.len() {
            self.net[k] = self.net_len(st, k);
        }
        self.wire = self.net.iter().map(|v| v.0).sum();
        self.wire_mm = self.net.iter().map(|v| v.1).sum();
        self.ov = 0.0;
        for i in 0..n {
            for j in i + 1..n {
                self.ov += self.pair(st, i, j);
            }
        }
        for i in 0..n {
            self.out_part[i] = self.outside_of(st, i);
            self.near_part[i] = self.near_of_part(st, i);
        }
        self.outside = self.out_part.iter().sum();
        self.near = self.near_part.iter().sum();
        for b in 0..self.block_names.len() {
            self.block_sum[b] = self.block_members[b].iter().fold((0.0, 0.0, 0.0), |s, &i| (s.0 + self.poses[i].x, s.1 + self.poses[i].y, s.2 + 1.0));
            self.block_spread[b] = self.spread_of(b);
        }
        self.spread = self.block_spread.iter().sum();
        self.undo = None;
    }

    /// Re-score after the movables in `moved` changed in `st`; `undo` puts it back.
    pub(crate) fn apply(&mut self, st: &State, moved: &[usize]) {
        let pr = st.pr;
        let parts: Vec<usize> = moved.iter().flat_map(|&m| self.movable_parts[m].iter().copied()).collect();
        let inside: BTreeSet<usize> = parts.iter().copied().collect();
        let mut u = Undo { parts: Vec::new(), nets: Vec::new(), blocks: Vec::new(), ov: self.ov, outside: self.outside, near: self.near, spread: self.spread, wire: self.wire, wire_mm: self.wire_mm };
        // Overlap: what the moved parts had with every other part, before and after.
        let mut before = 0.0;
        for &i in &parts {
            for j in 0..pr.parts.len() {
                // Each pair once: a moved part with every other part, moved pairs by index.
                if j == i || (inside.contains(&j) && j < i) {
                    continue;
                }
                before += self.pair(st, i, j);
            }
        }
        for &i in &parts {
            u.parts.push((i, self.poses[i], self.boxes[i], self.out_part[i], self.near_part[i]));
            self.poses[i] = st.part_pose(i);
            self.boxes[i] = bbox_at(&pr.parts[i].bbox, &self.poses[i], pr.parts[i].back);
        }
        let mut after = 0.0;
        for &i in &parts {
            for j in 0..pr.parts.len() {
                // Each pair once: a moved part with every other part, moved pairs by index.
                if j == i || (inside.contains(&j) && j < i) {
                    continue;
                }
                after += self.pair(st, i, j);
            }
        }
        self.ov += after - before;
        // Nets on the moved parts' pins.
        let nets: BTreeSet<usize> = parts.iter().flat_map(|&i| self.part_nets[i].iter().copied()).collect();
        for k in nets {
            let (w, mm) = self.net_len(st, k);
            u.nets.push((k, self.net[k].0, self.net[k].1));
            self.wire += w - self.net[k].0;
            self.wire_mm += mm - self.net[k].1;
            self.net[k] = (w, mm);
        }
        // Outside the outline, and `near`: the moved parts, and parts placed near them.
        for &i in &parts {
            let o = self.outside_of(st, i);
            self.outside += o - self.out_part[i];
            self.out_part[i] = o;
        }
        let near_touch: BTreeSet<usize> = parts.iter().copied().chain(parts.iter().flat_map(|&i| self.near_of[i].iter().copied())).collect();
        for i in near_touch {
            if !inside.contains(&i) {
                u.parts.push((i, self.poses[i], self.boxes[i], self.out_part[i], self.near_part[i]));
            }
            let v = self.near_of_part(st, i);
            self.near += v - self.near_part[i];
            self.near_part[i] = v;
        }
        // Block cohesion: the moved parts' blocks.
        let blocks: BTreeSet<usize> = parts.iter().filter_map(|&i| self.block_of[i]).collect();
        for b in blocks {
            u.blocks.push((b, self.block_sum[b], self.block_spread[b]));
            self.block_sum[b] = self.block_members[b].iter().fold((0.0, 0.0, 0.0), |s, &i| (s.0 + self.poses[i].x, s.1 + self.poses[i].y, s.2 + 1.0));
            let v = self.spread_of(b);
            self.spread += v - self.block_spread[b];
            self.block_spread[b] = v;
        }
        self.undo = Some(u);
    }

    /// Put back what the last `apply` changed.
    pub(crate) fn undo(&mut self) {
        let Some(u) = self.undo.take() else { return };
        for (i, pose, b, o, nr) in u.parts.into_iter().rev() {
            self.poses[i] = pose;
            self.boxes[i] = b;
            self.out_part[i] = o;
            self.near_part[i] = nr;
        }
        for (k, w, mm) in u.nets {
            self.net[k] = (w, mm);
        }
        for (b, s, v) in u.blocks {
            self.block_sum[b] = s;
            self.block_spread[b] = v;
        }
        self.ov = u.ov;
        self.outside = u.outside;
        self.near = u.near;
        self.spread = u.spread;
        self.wire = u.wire;
        self.wire_mm = u.wire_mm;
    }

    /// The cost as `State::cost` makes it, from the cached terms and the whole-board ones.
    pub(crate) fn cost(&self, st: &State, w_ov: f64) -> f64 {
        let pr = st.pr;
        let o = &pr.options;
        let mut all = [f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY];
        for b in &self.boxes {
            all = [all[0].min(b[0]), all[1].min(b[1]), all[2].max(b[2]), all[3].max(b[3])];
        }
        let mut area = (all[2] - all[0] + 2.0 * o.margin) * (all[3] - all[1] + 2.0 * o.margin);
        if let Some(ol) = pr.outline {
            all = ol;
            area = (ol[2] - ol[0]) * (ol[3] - ol[1]);
        }
        if let (Some(_), Some(poly)) = (&st.sdf, &pr.outline_polygon) {
            let (mut x0, mut y0, mut x1, mut y1) = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
            for p in poly {
                x0 = x0.min(p.0);
                y0 = y0.min(p.1);
                x1 = x1.max(p.0);
                y1 = y1.max(p.1);
            }
            all = [x0, y0, x1, y1];
            area = (x1 - x0) * (y1 - y0);
        }
        let mut edge = 0.0;
        for &i in &self.edge_parts {
            let e = pr.parts[i].edge.as_ref().unwrap();
            let face = rot(e.facing, self.poses[i].rotation);
            let b = self.boxes[i];
            let sides = [(Side::Left, b[0] - all[0], (-1.0, 0.0)), (Side::Right, all[2] - b[2], (1.0, 0.0)), (Side::Top, b[1] - all[1], (0.0, -1.0)), (Side::Bottom, all[3] - b[3], (0.0, 1.0))];
            edge += sides
                .iter()
                .enumerate()
                .filter(|(_, (s, _, _))| e.side.is_none_or(|w| w == *s))
                .map(|(k, (_, d, n))| {
                    let along: f64 = (0..4).filter(|&j| j != k).map(|j| (-sides[j].1).max(0.0)).sum();
                    d.max(-2.0 - d).max(0.0) + 10.0 * (1.0 - (face.0 * n.0 + face.1 * n.1)) + 2.0 * along
                })
                .fold(f64::INFINITY, f64::min);
        }
        let mut sep = 0.0;
        for s in &pr.separations {
            let bb = |blk: &str| {
                let mut out = [f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY];
                if let Ok(b) = self.block_names.binary_search_by(|x| x.as_str().cmp(blk)) {
                    for &i in &self.block_members[b] {
                        let x = self.boxes[i];
                        out = [out[0].min(x[0]), out[1].min(x[1]), out[2].max(x[2]), out[3].max(x[3])];
                    }
                }
                out
            };
            let (a, b) = (bb(&s.a), bb(&s.b));
            if a[0].is_finite() && b[0].is_finite() {
                let g = gap(&a, &b);
                if g < s.min_mm {
                    sep += s.min_mm - g;
                }
            }
        }
        o.w_wire * self.wire + w_ov * (self.ov + self.outside) + o.w_area * area + o.w_near * self.near + o.w_block * self.spread + o.w_edge * edge + o.w_sep * sep
    }

    fn net_len(&self, st: &State, k: usize) -> (f64, f64) {
        let net = &st.pr.nets[k];
        if net.pins.len() < 2 {
            return (0.0, 0.0);
        }
        let mut b = [f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY];
        for (i, pin) in &net.pins {
            let p = self.pin(st, *i, pin);
            b = [b[0].min(p.0), b[1].min(p.1), b[2].max(p.0), b[3].max(p.1)];
        }
        let hp = (b[2] - b[0]) + (b[3] - b[1]);
        (net.weight * hp, hp)
    }

    fn pin(&self, st: &State, i: usize, pin: &str) -> P {
        let part = &st.pr.parts[i];
        let p = side(part.pins.get(pin).copied().unwrap_or((0.0, 0.0)), part.back);
        let q = rot(p, self.poses[i].rotation);
        (self.poses[i].x + q.0, self.poses[i].y + q.1)
    }

    /// Two parts' overlap, as `State::cost` counts it.
    fn pair(&self, st: &State, i: usize, j: usize) -> f64 {
        let pr = st.pr;
        if (st.in_group[i].is_some() && st.in_group[i] == st.in_group[j]) || (pr.parts[i].fixed.is_some() && pr.parts[j].fixed.is_some()) {
            return 0.0;
        }
        let a = if pr.parts[i].back != pr.parts[j].back {
            let thru = |k: usize, other: usize| -> f64 { pr.parts[k].through.iter().map(|t| overlap(&bbox_at(t, &self.poses[k], pr.parts[k].back), &self.boxes[other], 0.0)).sum() };
            thru(i, j) + thru(j, i)
        } else {
            overlap(&self.boxes[i], &self.boxes[j], pr.options.clearance)
        };
        if a > 1e-9 {
            a
        } else {
            0.0
        }
    }

    fn outside_of(&self, st: &State, i: usize) -> f64 {
        let pr = st.pr;
        let o = &pr.options;
        if pr.parts[i].edge.is_some() || pr.parts[i].fixed.is_some() {
            return 0.0;
        }
        let b = self.boxes[i];
        let mut out = 0.0;
        if let Some(ol) = pr.outline {
            let lim = [ol[0] + o.margin, ol[1] + o.margin, ol[2] - o.margin, ol[3] - o.margin];
            let d = (lim[0] - b[0]).max(0.0) + (lim[1] - b[1]).max(0.0) + (b[2] - lim[2]).max(0.0) + (b[3] - lim[3]).max(0.0);
            if d > 1e-6 {
                out += d * ((b[2] - b[0]) + (b[3] - b[1]));
            }
        }
        if let (Some(sdf), Some(_)) = (&st.sdf, &pr.outline_polygon) {
            let (mx, my) = ((b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0);
            let pts = [(b[0], b[1]), (b[2], b[1]), (b[2], b[3]), (b[0], b[3]), (mx, b[1]), (mx, b[3]), (b[0], my), (b[2], my)];
            let d: f64 = pts.iter().map(|p| (o.margin - sdf.at(p.0, p.1)).max(0.0)).sum();
            if d > 1e-6 {
                out += d * ((b[2] - b[0]) + (b[3] - b[1]));
            }
        }
        out
    }

    fn near_of_part(&self, st: &State, i: usize) -> f64 {
        let Some((j, pin)) = &st.pr.parts[i].near else { return 0.0 };
        let t = self.pin(st, *j, pin);
        let b = self.boxes[i];
        let c = ((b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0);
        ((c.0 - t.0).powi(2) + (c.1 - t.1).powi(2)).sqrt()
    }

    fn spread_of(&self, b: usize) -> f64 {
        let c = self.block_sum[b];
        self.block_members[b].iter().map(|&i| ((self.poses[i].x - c.0 / c.2).powi(2) + (self.poses[i].y - c.1 / c.2).powi(2)).sqrt()).sum()
    }
}
