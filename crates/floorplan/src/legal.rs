//! The legaliser for crowded boards. `legalise` pushes overlapping parts apart one pair at a time,
//! which is enough where there is room. Where there isn't (an earring's back, scattered with
//! reserved keep-outs for rail vias and bypass-cap slots), no part in trouble has a clear spot
//! anywhere on the board on its own: the room it needs is where other movable parts sit. So here
//! a part in trouble takes the spot that costs least, counting its distance from where it was and
//! the movable parts it would evict, but never a spot on a fixed part or off the board. The
//! evicted parts take their turn. Each eviction raises the price of evicting that part again (as
//! PathFinder's history cost does for congested wires), so parts can't keep trading places.
//!
//! Candidate spots are tried nearest first, in a window about where the part was that doubles
//! until nothing outside it could do better. Its rows are where the part abuts a neighbour above
//! or below, and its columns, in each row, where it abuts a neighbour in that row: a part slid
//! toward where it was stops against one of those, so it can fill a gap exactly as wide as it
//! is. A grid, coarser as the window grows, covers the spots the board's edge bounds instead.

use std::collections::VecDeque;

use super::{bbox_at, gap, overlap, rot, Pose, State};

/// A part's courtyard and through-hole pads, in the board's frame.
#[derive(Clone)]
struct Shape {
    part: usize,
    bbox: [f64; 4],
    through: Vec<[f64; 4]>,
}

fn shift(b: &[f64; 4], dx: f64, dy: f64) -> [f64; 4] {
    [b[0] + dx, b[1] + dy, b[2] + dx, b[3] + dy]
}

/// A movable's parts with its anchor at `pose`.
fn shapes_at(st: &State, parts: &[usize], pose: Pose) -> Vec<Shape> {
    let pr = st.pr;
    parts
        .iter()
        .map(|&i| {
            let (_, off, r) = st.of_part[i];
            let q = rot(off, pose.rotation);
            let p = Pose { x: pose.x + q.0, y: pose.y + q.1, rotation: (pose.rotation + r).rem_euclid(360.0) };
            let part = &pr.parts[i];
            Shape { part: i, bbox: bbox_at(&part.bbox, &p, part.back), through: part.through.iter().map(|t| bbox_at(t, &p, part.back)).collect() }
        })
        .collect()
}

/// How much two parts collide, as `State::cost` counts it: courtyards on the same side, grown by
/// the clearance; through-hole pads against the other side's courtyards.
fn collide(st: &State, a: &Shape, b: &Shape) -> f64 {
    let pr = st.pr;
    if pr.parts[a.part].back != pr.parts[b.part].back {
        a.through.iter().map(|t| overlap(t, &b.bbox, 0.0)).sum::<f64>() + b.through.iter().map(|t| overlap(t, &a.bbox, 0.0)).sum::<f64>()
    } else {
        overlap(&a.bbox, &b.bbox, pr.options.clearance)
    }
}

/// How far a part pokes off the board (past the margin), as `trouble` measured it.
fn off_board(st: &State, b: &[f64; 4]) -> f64 {
    let pr = st.pr;
    let mg = pr.options.margin;
    let mut t = 0.0;
    if let Some(sdf) = &st.sdf {
        let (mx, my) = ((b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0);
        // Well inside: the edge is farther from its centre than any corner is (the grid's
        // interpolation is good to a step).
        if sdf.at(mx, my) > mg + 0.5 * ((b[2] - b[0]).hypot(b[3] - b[1])) + sdf.step {
            return 0.0;
        }
        for p in [(b[0], b[1]), (b[2], b[1]), (b[2], b[3]), (b[0], b[3]), (mx, b[1]), (mx, b[3]), (b[0], my), (b[2], my)] {
            t += (mg - sdf.at(p.0, p.1)).max(0.0);
        }
    }
    if let Some(ol) = pr.outline {
        t += (ol[0] + mg - b[0]).max(0.0) + (ol[1] + mg - b[1]).max(0.0) + (b[2] - ol[2] + mg).max(0.0) + (b[3] - ol[3] + mg).max(0.0);
    }
    t
}

const EPS: f64 = 1e-9;

/// The parts near a window, bucketed in 1 mm cells, so a candidate spot checks only its cells.
struct Cells {
    x0: f64,
    y0: f64,
    nx: usize,
    ny: usize,
    cell: Vec<Vec<usize>>,
}

impl Cells {
    const STEP: f64 = 1.0;

    fn new(near: &[usize], shapes: &[Shape], span: [f64; 4], grow: f64) -> Cells {
        let (x0, y0) = (span[0] - grow, span[1] - grow);
        let nx = ((span[2] - span[0] + 2.0 * grow) / Self::STEP).ceil() as usize + 1;
        let ny = ((span[3] - span[1] + 2.0 * grow) / Self::STEP).ceil() as usize + 1;
        let mut c = Cells { x0, y0, nx, ny, cell: vec![Vec::new(); nx * ny] };
        for &j in near {
            // Its courtyard and its through-hole pads, which reach the other side.
            let mut b = shapes[j].bbox;
            for t in &shapes[j].through {
                b = [b[0].min(t[0]), b[1].min(t[1]), b[2].max(t[2]), b[3].max(t[3])];
            }
            let (i0, j0, i1, j1) = c.range(&b, grow);
            for jj in j0..=j1 {
                for ii in i0..=i1 {
                    c.cell[jj * nx + ii].push(j);
                }
            }
        }
        c
    }

    fn range(&self, b: &[f64; 4], grow: f64) -> (usize, usize, usize, usize) {
        let ix = |v: f64| (((v - self.x0) / Self::STEP).floor().max(0.0) as usize).min(self.nx - 1);
        let iy = |v: f64| (((v - self.y0) / Self::STEP).floor().max(0.0) as usize).min(self.ny - 1);
        (ix(b[0] - grow), iy(b[1] - grow), ix(b[2] + grow), iy(b[3] + grow))
    }

    /// Every part bucketed in a cell `b` (grown by `grow`) touches; a part may come more than once.
    fn about<'a>(&'a self, b: &[f64; 4], grow: f64) -> impl Iterator<Item = usize> + 'a {
        let (i0, j0, i1, j1) = self.range(b, grow);
        (j0..=j1).flat_map(move |jj| (i0..=i1).flat_map(move |ii| self.cell[jj * self.nx + ii].iter().copied()))
    }
}

/// Clear every overlap the movables in `free` are part of, moving as little as it can. Movables
/// not in `free`, and any with an edge part, stay put: they are walls. If it can't clear them all
/// within its budget, it keeps whichever of before and after scores better.
pub(crate) fn negotiate(st: &mut State, free: &[usize]) {
    let pr = st.pr;
    let n = pr.parts.len();
    let nm = st.movables.len();
    let mut parts_of: Vec<Vec<usize>> = vec![Vec::new(); nm];
    for i in 0..n {
        parts_of[st.of_part[i].0].push(i);
    }
    let mut can_move = vec![false; nm];
    for &m in free {
        can_move[m] = parts_of[m].iter().all(|&i| pr.parts[i].edge.is_none());
    }
    let mut shapes: Vec<Shape> = (0..n)
        .map(|i| {
            let (p, part) = (st.part_pose(i), &pr.parts[i]);
            Shape { part: i, bbox: bbox_at(&part.bbox, &p, part.back), through: part.through.iter().map(|t| bbox_at(t, &p, part.back)).collect() }
        })
        .collect();

    let mut queue: VecDeque<usize> = VecDeque::new();
    let mut queued = vec![false; nm];
    let size = |m: usize, shapes: &[Shape]| -> f64 { parts_of[m].iter().map(|&i| (shapes[i].bbox[2] - shapes[i].bbox[0]) * (shapes[i].bbox[3] - shapes[i].bbox[1])).sum() };
    let trouble = |st: &State, m: usize, shapes: &[Shape]| -> f64 {
        let mut t = 0.0;
        for &i in &parts_of[m] {
            for j in 0..n {
                if st.of_part[j].0 != m {
                    t += collide(st, &shapes[i], &shapes[j]);
                }
            }
            t += off_board(st, &shapes[i].bbox);
        }
        t
    };
    let mut start: Vec<usize> = (0..nm).filter(|&m| can_move[m] && trouble(st, m, &shapes) > EPS).collect();
    if start.is_empty() {
        return;
    }
    // The largest first: they have the fewest places to go.
    start.sort_by(|&a, &b| size(b, &shapes).total_cmp(&size(a, &shapes)));
    for m in start {
        queued[m] = true;
        queue.push_back(m);
    }

    // Where the search may look: the board, or around the parts when it has no outline.
    let mut area = [f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY];
    for s in &shapes {
        area = [area[0].min(s.bbox[0]), area[1].min(s.bbox[1]), area[2].max(s.bbox[2]), area[3].max(s.bbox[3])];
    }
    if let Some(ol) = pr.outline.or_else(|| super::poly_box(pr)) {
        area = ol;
    } else {
        area = [area[0] - 5.0, area[1] - 5.0, area[2] + 5.0, area[3] + 5.0];
    }

    let before = (st.cost(pr.options.w_overlap * 201.0).0, st.mv.clone());
    let home: Vec<Pose> = st.mv.clone();
    let mut history = vec![0.0f64; nm];
    let budget = 60 * queue.len() + 400;
    let grow = pr.options.clearance;
    let mut steps = 0;
    let mut seen = vec![0usize; n];
    let mut stamp = 0;
    while let Some(m) = queue.pop_front() {
        queued[m] = false;
        if steps >= budget {
            break;
        }
        steps += 1;
        if trouble(st, m, &shapes) <= EPS {
            continue;
        }
        let mine = &parts_of[m];
        let h = home[m];
        // Block separations it must keep: (its block, the box of the rest of that block, the
        // other block's box, the distance). One already too close without it is not its to keep.
        let boxed = |blk: &str| {
            let mut b = [f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY];
            for i in (0..n).filter(|&i| st.of_part[i].0 != m && pr.parts[i].block.as_deref() == Some(blk)) {
                let s = &shapes[i].bbox;
                b = [b[0].min(s[0]), b[1].min(s[1]), b[2].max(s[2]), b[3].max(s[3])];
            }
            b
        };
        let mut keep: Vec<(&str, [f64; 4], [f64; 4], f64)> = Vec::new();
        for sep in &pr.separations {
            for (mine_blk, other) in [(&sep.a, &sep.b), (&sep.b, &sep.a)] {
                if mine.iter().any(|&i| pr.parts[i].block.as_ref() == Some(mine_blk)) {
                    let (rest, far) = (boxed(mine_blk), boxed(other));
                    if far[0].is_finite() && !(rest[0].is_finite() && gap(&rest, &far) < sep.min_mm) {
                        keep.push((mine_blk.as_str(), rest, far, sep.min_mm));
                    }
                }
            }
        }
        // (cost, pose, the movables it evicts)
        let mut best: Option<(f64, Pose, Vec<usize>)> = None;
        let mut radius = 1.5;
        let mut tried = 0.0;
        loop {
            // The window about where it was, within the board (a push may have left it off it).
            let win = [(h.x - radius).max(area[0]), (h.y - radius).max(area[1]), (h.x + radius).min(area[2]), (h.y + radius).min(area[3])];
            let whole = h.x - radius <= area[0] && h.y - radius <= area[1] && h.x + radius >= area[2] && h.y + radius >= area[3];
            for q in 0..4 {
                let rotation = (h.rotation + 90.0 * q as f64).rem_euclid(360.0);
                let at0 = shapes_at(st, mine, Pose { x: 0.0, y: 0.0, rotation });
                let mut ext = [f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY];
                for s in &at0 {
                    ext = [ext[0].min(s.bbox[0]), ext[1].min(s.bbox[1]), ext[2].max(s.bbox[2]), ext[3].max(s.bbox[3])];
                }
                // The parts it could touch anywhere in the window.
                let near: Vec<usize> = (0..n)
                    .filter(|&j| st.of_part[j].0 != m)
                    .filter(|&j| {
                        let b = &shapes[j].bbox;
                        b[2] + grow >= win[0] + ext[0] && b[0] - grow <= win[2] + ext[2] && b[3] + grow >= win[1] + ext[1] && b[1] - grow <= win[3] + ext[3]
                    })
                    .collect();
                // Rows: abutting each neighbour above or below, where it was, and a grid that
                // coarsens as the window grows (for spots the board's edge bounds).
                let step = (radius / 24.0).max(0.25);
                let grid = |lo: f64, hi: f64| {
                    let mut v = Vec::new();
                    let mut t = lo;
                    while t <= hi {
                        v.push(t);
                        t += step;
                    }
                    v
                };
                let clear = |j: usize| if pr.parts[j].back == pr.parts[mine[0]].back { grow + 1e-3 } else { 1e-3 };
                // Each neighbour's courtyard with its through-hole pads.
                let reach: Vec<[f64; 4]> = near
                    .iter()
                    .map(|&j| {
                        let mut b = shapes[j].bbox;
                        for t in &shapes[j].through {
                            b = [b[0].min(t[0]), b[1].min(t[1]), b[2].max(t[2]), b[3].max(t[3])];
                        }
                        b
                    })
                    .collect();
                let mut ys = grid(win[1], win[3]);
                ys.push(h.y);
                for (k, &j) in near.iter().enumerate() {
                    ys.push(reach[k][3] + clear(j) - ext[1]);
                    ys.push(reach[k][1] - clear(j) - ext[3]);
                }
                ys.retain(|y| (win[1]..=win[3]).contains(y));
                ys.sort_by(f64::total_cmp);
                ys.dedup_by(|a, b| (*a - *b).abs() < 1e-4);
                let gx = grid(win[0], win[2]);
                // Turning a part costs a little: it moves its pins.
                let turn = if q == 0 { 0.0 } else { 0.3 };
                // Nearest first, leaving out what the last window already tried.
                let mut cands: Vec<(f64, f64, f64)> = Vec::new();
                let mut xs: Vec<f64> = Vec::new();
                for &y in &ys {
                    // Sliding toward where it was, a part stops against a neighbour in its row,
                    // or where it was: those are the columns worth trying.
                    xs.clear();
                    xs.extend_from_slice(&gx);
                    xs.push(h.x);
                    for (k, &j) in near.iter().enumerate() {
                        let b = &reach[k];
                        if b[3] + grow >= y + ext[1] && b[1] - grow <= y + ext[3] {
                            xs.push(b[2] + clear(j) - ext[0]);
                            xs.push(b[0] - clear(j) - ext[2]);
                        }
                    }
                    for &x in &xs {
                        if x < win[0] || x > win[2] || (x - h.x).abs().max((y - h.y).abs()) < tried {
                            continue;
                        }
                        let dist = ((x - h.x).powi(2) + (y - h.y).powi(2)).sqrt() + turn;
                        if best.as_ref().is_none_or(|b| dist < b.0) {
                            cands.push((dist, x, y));
                        }
                    }
                }
                cands.sort_by(|a, b| a.0.total_cmp(&b.0));
                let cells = Cells::new(&near, &shapes, [win[0] + ext[0], win[1] + ext[1], win[2] + ext[2], win[3] + ext[3]], grow);
                let mut cand = at0.clone();
                for (dist, x, y) in cands {
                    if best.as_ref().is_some_and(|b| dist >= b.0) {
                        break;
                    }
                    for (c, s) in cand.iter_mut().zip(&at0) {
                        c.bbox = shift(&s.bbox, x, y);
                        for (ct, t) in c.through.iter_mut().zip(&s.through) {
                            *ct = shift(t, x, y);
                        }
                    }
                    if cand.iter().any(|s| off_board(st, &s.bbox) > EPS) {
                        continue;
                    }
                    let too_close = keep.iter().any(|(blk, rest, far, min)| {
                        let mut b = *rest;
                        for s in cand.iter().filter(|s| pr.parts[s.part].block.as_deref() == Some(*blk)) {
                            b = [b[0].min(s.bbox[0]), b[1].min(s.bbox[1]), b[2].max(s.bbox[2]), b[3].max(s.bbox[3])];
                        }
                        gap(&b, far) < *min
                    });
                    if too_close {
                        continue;
                    }
                    let mut cost = dist;
                    let mut evict: Vec<usize> = Vec::new();
                    let mut blocked = false;
                    for s in &cand {
                        stamp += 1;
                        for j in cells.about(&s.bbox, grow) {
                            if seen[j] == stamp || collide(st, s, &shapes[j]) <= EPS {
                                continue;
                            }
                            seen[j] = stamp;
                            let o = st.of_part[j].0;
                            if !can_move[o] {
                                blocked = true;
                                break;
                            }
                            if !evict.contains(&o) {
                                evict.push(o);
                                // Evicting a larger part, or one evicted often, costs more.
                                cost += 1.0 + size(o, &shapes).sqrt() + 2.0 * history[o];
                            }
                        }
                        if blocked || best.as_ref().is_some_and(|b| cost >= b.0) {
                            blocked = true;
                            break;
                        }
                    }
                    if !blocked {
                        best = Some((cost, Pose { x, y, rotation }, evict));
                    }
                }
            }
            // Nothing farther than the window can beat what it holds.
            if best.as_ref().is_some_and(|b| b.0 <= radius) || whole {
                break;
            }
            tried = radius;
            radius *= 2.0;
        }
        let Some((_, pose, evict)) = best else { continue };
        st.mv[m] = pose;
        for s in shapes_at(st, mine, pose) {
            let i = s.part;
            shapes[i] = s;
        }
        for o in evict {
            history[o] += 1.0;
            if !queued[o] {
                queued[o] = true;
                queue.push_back(o);
            }
        }
    }
    if before.0 < st.cost(pr.options.w_overlap * 201.0).0 {
        st.mv = before.1;
    }
}
