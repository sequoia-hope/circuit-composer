//! The analytical placer: a start for the anneal, found by gradient descent instead of chance.
//!
//! Every free movable's position is a continuous variable, and the cost is made smooth enough to
//! follow downhill (after ePlace and Cypress, without their density grid: at a few hundred parts
//! every pair can be checked):
//! - **wire**: each net's weighted-average wirelength, which tends to the half-perimeter as its
//!   smoothing `gamma` shrinks through the run;
//! - **overlap**: for two courtyards on one side, their overlap in x times their overlap in y,
//!   measured from their centres so that a part wholly inside another is still pushed out; its
//!   weight rises through the run, as the anneal's does;
//! - **outline**: how far a courtyard pokes out of the outline (or, on a shaped board, of its
//!   signed-distance field), squared;
//! - **area** with no outline: the courtyards' extent, smoothed like a net;
//! - **near**, **block**, **edge** and **separation** as the anneal scores them.
//!
//! Steps are Adam's, which keeps each movable's step near the learning rate whatever the terms'
//! scales. Rotations are discrete: now and then every movable tries its four quarter turns
//! against the anneal's own cost (`Inc`) and keeps the best. What comes out still overlaps a
//! little; `solve_with` takes it as a warm start: legalised, repaired and annealed cool.

use std::collections::BTreeMap;

use super::{bbox_at, edge_slides, fixed_pose, gap, inc::Inc, layout, poly_box, rot, span_of, w_ov_at, Pose, Problem, Rng, Sdf, Side, State, P};

/// Gradient steps in a run.
const ITERS: usize = 1200;

/// Every part's pose, in `Problem::parts` order: the analytical placer's placement.
pub(crate) fn global(pr: &Problem) -> Vec<Pose> {
    let n = pr.parts.len();
    let o = &pr.options;
    let (movables, of_part, in_group) = layout(pr);
    let span = span_of(pr);
    let board = pr.outline.or(poly_box(pr));
    let (cx, cy, sx, sy) = match board {
        Some(b) => ((b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0, b[2] - b[0], b[3] - b[1]),
        None => (0.0, 0.0, span, span),
    };
    // Everything starts in a small cloud at the middle; the overlap term spreads it.
    let mut rng = Rng(o.seed.wrapping_mul(0x9E3779B97F4A7C15) | 1);
    let mv: Vec<Pose> = movables.iter().map(|m| fixed_pose(pr, m).unwrap_or(Pose { x: cx + rng.normal() * sx * 0.05, y: cy + rng.normal() * sy * 0.05, rotation: 0.0 })).collect();
    let sdf = pr.outline_polygon.as_ref().map(|p| Sdf::new(p, 0.1));
    let mut st = State { pr, sdf, mv, movables, of_part, in_group };
    let free: Vec<usize> = (0..st.movables.len()).filter(|&m| fixed_pose(pr, &st.movables[m]).is_none()).collect();
    let slides = edge_slides(&mut st, &free, true);
    let mut adam = vec![[0.0f64; 4]; st.movables.len()];
    let (g0, g1) = (span * 0.1, 0.25);
    let (lr0, lr1) = (span * 0.02, 0.005);
    let rot_from = ITERS * 3 / 10;
    for t in 0..ITERS {
        let prog = t as f64 / ITERS as f64;
        let gamma = g0 * (g1 / g0).powf(prog);
        let lr = lr0 * (lr1 / lr0).powf(prog);
        // Overlap weighs little at first, so connected parts gather, and much at the end.
        let w_ov = 0.02 * (2.0e4f64).powf(prog);
        let g = gradient(&st, gamma, w_ov, board);
        let mut gm = vec![(0.0, 0.0); st.movables.len()];
        for i in 0..n {
            let m = st.of_part[i].0;
            gm[m].0 += g[i].0;
            gm[m].1 += g[i].1;
        }
        let k = (t + 1) as f64;
        for &m in &free {
            let (mut gx, mut gy) = gm[m];
            match slides.get(&m) {
                Some(true) => gy = 0.0,
                Some(false) => gx = 0.0,
                None => {}
            }
            let a = &mut adam[m];
            a[0] = 0.9 * a[0] + 0.1 * gx;
            a[1] = 0.9 * a[1] + 0.1 * gy;
            a[2] = 0.999 * a[2] + 0.001 * gx * gx;
            a[3] = 0.999 * a[3] + 0.001 * gy * gy;
            let (b1, b2) = (1.0 - 0.9f64.powf(k), 1.0 - 0.999f64.powf(k));
            st.mv[m].x -= lr * (a[0] / b1) / ((a[2] / b2).sqrt() + 1e-9);
            st.mv[m].y -= lr * (a[1] / b1) / ((a[3] / b2).sqrt() + 1e-9);
        }
        if t >= rot_from && (t - rot_from) % (ITERS / 12) == 0 {
            turn(&mut st, &free, &slides, w_ov_at(o, prog));
        }
    }
    (0..n).map(|i| st.part_pose(i)).collect()
}

/// Each movable in turn tries its other three quarter turns and keeps whichever costs least.
fn turn(st: &mut State, free: &[usize], slides: &BTreeMap<usize, bool>, w_ov: f64) {
    let mut inc = Inc::new(st);
    for &m in free {
        if slides.contains_key(&m) {
            continue;
        }
        let home = st.mv[m].rotation;
        let mut best = (inc.cost(st, w_ov), home);
        for q in 1..4 {
            st.mv[m].rotation = (home + 90.0 * q as f64).rem_euclid(360.0);
            inc.apply(st, &[m]);
            let c = inc.cost(st, w_ov);
            if c < best.0 {
                best = (c, st.mv[m].rotation);
            }
            inc.undo();
        }
        st.mv[m].rotation = home;
        if best.1 != home {
            st.mv[m].rotation = best.1;
            inc.apply(st, &[m]);
        }
    }
}

/// The weighted-average extent of `xs` (smooth max less smooth min), and its gradient into `d`.
fn wa(xs: &[f64], gamma: f64, d: &mut Vec<f64>) -> f64 {
    d.clear();
    d.resize(xs.len(), 0.0);
    let hi = xs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let lo = xs.iter().copied().fold(f64::INFINITY, f64::min);
    let (mut sa, mut sxa, mut sb, mut sxb) = (0.0, 0.0, 0.0, 0.0);
    for &x in xs {
        let a = ((x - hi) / gamma).exp();
        let b = ((lo - x) / gamma).exp();
        sa += a;
        sxa += x * a;
        sb += b;
        sxb += x * b;
    }
    let (wmax, wmin) = (sxa / sa, sxb / sb);
    for (k, &x) in xs.iter().enumerate() {
        let a = ((x - hi) / gamma).exp();
        let b = ((lo - x) / gamma).exp();
        d[k] = a / sa * (1.0 + (x - wmax) / gamma) - b / sb * (1.0 - (x - wmin) / gamma);
    }
    wmax - wmin
}

/// The smoothed cost's gradient for every part's position.
fn gradient(st: &State, gamma: f64, w_ov: f64, board: Option<[f64; 4]>) -> Vec<P> {
    let pr = st.pr;
    let o = &pr.options;
    let n = pr.parts.len();
    let poses: Vec<Pose> = (0..n).map(|i| st.part_pose(i)).collect();
    let boxes: Vec<[f64; 4]> = (0..n).map(|i| bbox_at(&pr.parts[i].bbox, &poses[i], pr.parts[i].back)).collect();
    let mut g = vec![(0.0, 0.0); n];
    let mut xs = Vec::new();
    let mut ys = Vec::new();
    let mut d = Vec::new();

    // Wire.
    for net in &pr.nets {
        if net.pins.len() < 2 {
            continue;
        }
        xs.clear();
        ys.clear();
        for (i, pin) in &net.pins {
            let p = st.pin(&poses, *i, pin);
            xs.push(p.0);
            ys.push(p.1);
        }
        let w = o.w_wire * net.weight;
        wa(&xs, gamma, &mut d);
        for (k, (i, _)) in net.pins.iter().enumerate() {
            g[*i].0 += w * d[k];
        }
        wa(&ys, gamma, &mut d);
        for (k, (i, _)) in net.pins.iter().enumerate() {
            g[*i].1 += w * d[k];
        }
    }

    // Overlap: pairs found by sweeping boxes sorted by their left side.
    let grow = o.clearance;
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| boxes[a][0].total_cmp(&boxes[b][0]));
    for (k, &i) in order.iter().enumerate() {
        for &j in &order[k + 1..] {
            if boxes[j][0] > boxes[i][2] + grow {
                break;
            }
            let (pi, pj) = (&pr.parts[i], &pr.parts[j]);
            if pi.back != pj.back || (st.in_group[i].is_some() && st.in_group[i] == st.in_group[j]) || (pi.fixed.is_some() && pj.fixed.is_some()) {
                continue;
            }
            let (a, b) = (&boxes[i], &boxes[j]);
            let dx = (a[0] + a[2] - b[0] - b[2]) / 2.0;
            let dy = (a[1] + a[3] - b[1] - b[3]) / 2.0;
            let ox = (a[2] - a[0] + b[2] - b[0]) / 2.0 + grow - dx.abs();
            let oy = (a[3] - a[1] + b[3] - b[1]) / 2.0 + grow - dy.abs();
            if ox <= 0.0 || oy <= 0.0 {
                continue;
            }
            // Apart along both axes, a tie broken by index.
            let sx = if dx != 0.0 { dx.signum() } else if i < j { 1.0 } else { -1.0 };
            let sy = if dy != 0.0 { dy.signum() } else if i < j { 1.0 } else { -1.0 };
            let (fx, fy) = (w_ov * sx * oy, w_ov * sy * ox);
            g[i].0 -= fx;
            g[i].1 -= fy;
            g[j].0 += fx;
            g[j].1 += fy;
        }
    }

    // The outline.
    let w_out = 10.0 * w_ov + 1.0;
    for (i, p) in pr.parts.iter().enumerate() {
        if p.edge.is_some() || p.fixed.is_some() {
            continue;
        }
        let b = boxes[i];
        if let Some(ol) = pr.outline {
            let m = o.margin;
            g[i].0 -= 2.0 * w_out * (ol[0] + m - b[0]).max(0.0);
            g[i].1 -= 2.0 * w_out * (ol[1] + m - b[1]).max(0.0);
            g[i].0 += 2.0 * w_out * (b[2] - ol[2] + m).max(0.0);
            g[i].1 += 2.0 * w_out * (b[3] - ol[3] + m).max(0.0);
        }
        if let Some(sdf) = &st.sdf {
            let (mx, my) = ((b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0);
            for q in [(b[0], b[1]), (b[2], b[1]), (b[2], b[3]), (b[0], b[3]), (mx, b[1]), (mx, b[3]), (b[0], my), (b[2], my)] {
                let short = o.margin - sdf.at(q.0, q.1);
                if short > 0.0 {
                    let h = 0.05;
                    let nx = (sdf.at(q.0 + h, q.1) - sdf.at(q.0 - h, q.1)) / (2.0 * h);
                    let ny = (sdf.at(q.0, q.1 + h) - sdf.at(q.0, q.1 - h)) / (2.0 * h);
                    // Into the board is up the distance field.
                    g[i].0 -= 2.0 * w_out * short * nx;
                    g[i].1 -= 2.0 * w_out * short * ny;
                }
            }
        }
    }

    // The board's extent when no outline is set: the smoothed width times the smoothed height.
    let extent = boxes.iter().fold([f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY], |a, b| [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])]);
    if board.is_none() {
        xs.clear();
        ys.clear();
        for b in &boxes {
            xs.extend([b[0], b[2]]);
            ys.extend([b[1], b[3]]);
        }
        let mut dy = Vec::new();
        let w = wa(&xs, gamma, &mut d);
        let h = wa(&ys, gamma, &mut dy);
        for i in 0..n {
            g[i].0 += o.w_area * h * (d[2 * i] + d[2 * i + 1]);
            g[i].1 += o.w_area * w * (dy[2 * i] + dy[2 * i + 1]);
        }
    }

    // Near: a part's centre to the pad it serves.
    for (i, p) in pr.parts.iter().enumerate() {
        let Some((j, pin)) = &p.near else { continue };
        let t = st.pin(&poses, *j, pin);
        let c = ((boxes[i][0] + boxes[i][2]) / 2.0, (boxes[i][1] + boxes[i][3]) / 2.0);
        let (ex, ey) = (c.0 - t.0, c.1 - t.1);
        let l = (ex * ex + ey * ey + 0.01).sqrt();
        let (fx, fy) = (o.w_near * ex / l, o.w_near * ey / l);
        g[i].0 += fx;
        g[i].1 += fy;
        g[*j].0 -= fx;
        g[*j].1 -= fy;
    }

    // Blocks: each part towards its block's centre, and blocks that must stay apart pushed apart.
    let mut centres: BTreeMap<&str, (f64, f64, f64)> = BTreeMap::new();
    for (i, p) in pr.parts.iter().enumerate() {
        if let Some(b) = &p.block {
            let e = centres.entry(b.as_str()).or_insert((0.0, 0.0, 0.0));
            *e = (e.0 + poses[i].x, e.1 + poses[i].y, e.2 + 1.0);
        }
    }
    for (i, p) in pr.parts.iter().enumerate() {
        let Some(b) = &p.block else { continue };
        let c = centres[b.as_str()];
        let (ex, ey) = (poses[i].x - c.0 / c.2, poses[i].y - c.1 / c.2);
        let l = (ex * ex + ey * ey + 0.01).sqrt();
        g[i].0 += o.w_block * ex / l;
        g[i].1 += o.w_block * ey / l;
    }
    for s in &pr.separations {
        let bb = |blk: &str| {
            (0..n).filter(|&i| pr.parts[i].block.as_deref() == Some(blk)).fold([f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY], |a, i| {
                let b = boxes[i];
                [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])]
            })
        };
        let (a, b) = (bb(&s.a), bb(&s.b));
        if !a[0].is_finite() || !b[0].is_finite() || gap(&a, &b) >= s.min_mm {
            continue;
        }
        let (ex, ey) = ((a[0] + a[2] - b[0] - b[2]) / 2.0, (a[1] + a[3] - b[1] - b[3]) / 2.0);
        let l = (ex * ex + ey * ey + 0.01).sqrt();
        for (i, p) in pr.parts.iter().enumerate() {
            let k = match p.block.as_deref() {
                Some(x) if x == s.a => -1.0,
                Some(x) if x == s.b => 1.0,
                _ => continue,
            };
            g[i].0 += k * o.w_sep * ex / l;
            g[i].1 += k * o.w_sep * ey / l;
        }
    }

    // Edge parts that are not sliding along a set side: to whichever side suits them best.
    let all = board.unwrap_or(extent);
    for (i, p) in pr.parts.iter().enumerate() {
        let Some(e) = &p.edge else { continue };
        if p.fixed.is_some() {
            continue;
        }
        let b = boxes[i];
        let face = rot(e.facing, poses[i].rotation);
        let sides = [(Side::Left, b[0] - all[0], (-1.0, 0.0)), (Side::Right, all[2] - b[2], (1.0, 0.0)), (Side::Top, b[1] - all[1], (0.0, -1.0)), (Side::Bottom, all[3] - b[3], (0.0, 1.0))];
        let score = |k: usize| {
            let (_, d, nrm) = sides[k];
            let along: f64 = (0..4).filter(|&j| j != k).map(|j| (-sides[j].1).max(0.0)).sum();
            d.max(-2.0 - d).max(0.0) + 10.0 * (1.0 - (face.0 * nrm.0 + face.1 * nrm.1)) + 2.0 * along
        };
        let Some(k) = (0..4).filter(|&k| e.side.is_none_or(|w| w == sides[k].0)).min_by(|&a, &b| score(a).total_cmp(&score(b))) else { continue };
        let (_, d, nrm) = sides[k];
        // Out towards the side when short of it, back in when past it by more than 2 mm.
        let push = if d > 0.0 { 1.0 } else if d < -2.0 { -1.0 } else { 0.0 };
        g[i].0 -= o.w_edge * push * nrm.0;
        g[i].1 -= o.w_edge * push * nrm.1;
        for j in (0..4).filter(|&j| j != k) {
            if sides[j].1 < 0.0 {
                g[i].0 += 2.0 * o.w_edge * sides[j].2 .0;
                g[i].1 += 2.0 * o.w_edge * sides[j].2 .1;
            }
        }
    }
    g
}
