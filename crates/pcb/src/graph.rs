//! The path current takes through a net's copper between two pads: shortest, over track
//! centrelines, via barrels, and straight lines across a zone fill between the points where
//! other copper meets it. A fast proxy for a current path, not a field solution: a straight line
//! across a fill may cut a corner the fill does not have.

use std::cmp::Ordering;
use std::collections::BinaryHeap;

use crate::geom::{self, P};
use crate::Board;

#[derive(Debug, Clone)]
pub struct Path {
    /// Points from the first pad's centre to the second's.
    pub points: Vec<P>,
    /// The layer of each point.
    pub layers: Vec<String>,
    pub length: f64,
}

struct Node {
    p: P,
    layer: usize,
}

#[derive(PartialEq)]
struct Item(f64, usize);
impl Eq for Item {}
impl PartialOrd for Item {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Item {
    fn cmp(&self, o: &Self) -> Ordering {
        o.0.total_cmp(&self.0)
    }
}

/// The shortest copper path on `net` from pad `from` to pad `to` (`REF.PIN`), or `None` when
/// the copper does not join them.
pub fn copper_path(board: &Board, net: &str, from: &str, to: &str) -> Option<Path> {
    let li = |name: &str| board.layers.iter().position(|l| l == name);
    let mut nodes: Vec<Node> = Vec::new();
    let mut adj: Vec<Vec<(usize, f64)>> = Vec::new();
    let add = |nodes: &mut Vec<Node>, adj: &mut Vec<Vec<(usize, f64)>>, p: P, layer: usize| {
        nodes.push(Node { p, layer });
        adj.push(Vec::new());
        nodes.len() - 1
    };
    let link = |adj: &mut Vec<Vec<(usize, f64)>>, a: usize, b: usize, w: f64| {
        adj[a].push((b, w));
        adj[b].push((a, w));
    };

    // Pads: a node per layer, joined at no cost.
    let mut pad_nodes: Vec<(usize, Vec<usize>)> = Vec::new();
    for (i, pad) in board.pads.iter().enumerate().filter(|(_, p)| p.net == net) {
        let ids: Vec<usize> = pad.layers.iter().filter_map(|l| li(l)).map(|l| add(&mut nodes, &mut adj, pad.centre, l)).collect();
        for w in ids.windows(2) {
            link(&mut adj, w[0], w[1], 0.0);
        }
        pad_nodes.push((i, ids));
    }
    // Tracks: both ends.
    let mut track_nodes = Vec::new();
    for t in board.tracks.iter().filter(|t| t.net == net) {
        let Some(l) = li(&t.layer) else { continue };
        let a = add(&mut nodes, &mut adj, t.a, l);
        let b = add(&mut nodes, &mut adj, t.b, l);
        link(&mut adj, a, b, geom::dist(t.a, t.b));
        track_nodes.push((a, b, t.width));
    }
    // Vias: a node per layer, joined at no cost.
    let mut via_nodes = Vec::new();
    for v in board.vias.iter().filter(|v| v.net == net) {
        let ids: Vec<usize> = v.layers.iter().filter_map(|l| li(l)).map(|l| add(&mut nodes, &mut adj, v.at, l)).collect();
        for w in ids.windows(2) {
            link(&mut adj, w[0], w[1], 0.0);
        }
        via_nodes.push((ids, v.size));
    }

    // Track ends meet pads, vias, other tracks' ends and middles.
    for &(a, b, w) in &track_nodes {
        for end in [a, b] {
            let (p, l) = (nodes[end].p, nodes[end].layer);
            for (pi, ids) in &pad_nodes {
                let pad = &board.pads[*pi];
                if let Some(&id) = ids.iter().find(|&&id| nodes[id].layer == l) {
                    if pad.shape.iter().any(|s| geom::inside(s, p) || geom::polyline_poly_dist(&[p], s) <= w / 2.0) {
                        link(&mut adj, end, id, geom::dist(p, pad.centre));
                    }
                }
            }
            for (ids, size) in &via_nodes {
                if let Some(&id) = ids.iter().find(|&&id| nodes[id].layer == l) {
                    if geom::dist(p, nodes[id].p) <= size / 2.0 {
                        link(&mut adj, end, id, geom::dist(p, nodes[id].p));
                    }
                }
            }
            for &(c, d, w2) in &track_nodes {
                if c == a || nodes[c].layer != l {
                    continue;
                }
                let (pc, pd) = (nodes[c].p, nodes[d].p);
                if geom::point_seg_dist(p, pc, pd) <= (w.max(w2)) / 2.0 {
                    link(&mut adj, end, c, geom::dist(p, pc));
                    link(&mut adj, end, d, geom::dist(p, pd));
                }
            }
        }
    }

    // Fills: every node inside joins every other inside, in a straight line. A pad that
    // overlaps the fill joins at its centre.
    for f in board.fills.iter().filter(|f| f.net == net) {
        let Some(l) = li(&f.layer) else { continue };
        let mut members: Vec<usize> = (0..nodes.len()).filter(|&i| nodes[i].layer == l && geom::inside(&f.polygon, nodes[i].p)).collect();
        for (pi, ids) in &pad_nodes {
            if let Some(&id) = ids.iter().find(|&&id| nodes[id].layer == l) {
                if !members.contains(&id) && board.pads[*pi].shape.iter().any(|s| s.iter().any(|q| geom::inside(&f.polygon, *q))) {
                    members.push(id);
                }
            }
        }
        for (k, &i) in members.iter().enumerate() {
            for &j in &members[k + 1..] {
                let w = geom::dist(nodes[i].p, nodes[j].p);
                link(&mut adj, i, j, w);
            }
        }
    }

    let start_pad = board.pad(from)?;
    let end_pad = board.pad(to)?;
    let find = |pad: &crate::Pad| -> Vec<usize> {
        pad_nodes
            .iter()
            .find(|(pi, _)| std::ptr::eq(&board.pads[*pi], pad))
            .map(|(_, ids)| ids.clone())
            .unwrap_or_default()
    };
    let (starts, ends) = (find(start_pad), find(end_pad));

    let mut best = vec![f64::INFINITY; nodes.len()];
    let mut prev = vec![usize::MAX; nodes.len()];
    let mut heap = BinaryHeap::new();
    for &s in &starts {
        best[s] = 0.0;
        heap.push(Item(0.0, s));
    }
    while let Some(Item(d, u)) = heap.pop() {
        if d > best[u] {
            continue;
        }
        for &(v, w) in &adj[u] {
            if d + w < best[v] {
                best[v] = d + w;
                prev[v] = u;
                heap.push(Item(d + w, v));
            }
        }
    }
    let &end = ends.iter().min_by(|a, b| best[**a].total_cmp(&best[**b]))?;
    if !best[end].is_finite() {
        return None;
    }
    let mut chain = vec![end];
    while prev[*chain.last().unwrap()] != usize::MAX {
        chain.push(prev[*chain.last().unwrap()]);
    }
    chain.reverse();
    let mut points: Vec<P> = Vec::new();
    let mut layers = Vec::new();
    for i in chain {
        if points.last().is_some_and(|q| geom::dist(*q, nodes[i].p) < 1e-9) {
            continue;
        }
        points.push(nodes[i].p);
        layers.push(board.layers[nodes[i].layer].clone());
    }
    Some(Path { points, layers, length: best[end] })
}
