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

    // A track that runs through a pad joins it in the middle: the pad reaches both ends along
    // the track.
    for &(a, b, w) in &track_nodes {
        let (pa, pb, l) = (nodes[a].p, nodes[b].p, nodes[a].layer);
        for (pi, ids) in &pad_nodes {
            let pad = &board.pads[*pi];
            let Some(&id) = ids.iter().find(|&&id| nodes[id].layer == l) else { continue };
            let ends_inside = pad.shape.iter().any(|s| geom::inside(s, pa) || geom::inside(s, pb));
            if !ends_inside && geom::point_seg_dist(pad.centre, pa, pb) <= w / 2.0 {
                link(&mut adj, id, a, geom::dist(pad.centre, pa));
                link(&mut adj, id, b, geom::dist(pad.centre, pb));
            }
        }
    }

    // A via inside a pad (or touching it) joins the pad on that layer: via-in-pad, with the
    // track leaving on another layer.
    for (vids, size) in &via_nodes {
        for &v in vids {
            let (p, l) = (nodes[v].p, nodes[v].layer);
            for (pi, ids) in &pad_nodes {
                let pad = &board.pads[*pi];
                let Some(&id) = ids.iter().find(|&&id| nodes[id].layer == l) else { continue };
                if pad.shape.iter().any(|s| geom::inside(s, p) || geom::polyline_poly_dist(&[p], s) <= size / 2.0) {
                    link(&mut adj, v, id, geom::dist(p, pad.centre));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Pad, Part, Track};
    use std::collections::BTreeMap;

    fn pad(part: &str, at: P) -> Pad {
        let sq = vec![[at[0] - 0.3, at[1] - 0.3], [at[0] + 0.3, at[1] - 0.3], [at[0] + 0.3, at[1] + 0.3], [at[0] - 0.3, at[1] + 0.3]];
        Pad { part: part.into(), pin: "1".into(), net: "N".into(), layers: vec!["F.Cu".into()], centre: at, shape: vec![sq] }
    }

    #[test]
    fn track_through_a_pad_joins_it() {
        // One track from A to B runs straight over P's pad; P has no track end of its own.
        let pads = vec![pad("A", [0.0, 0.0]), pad("P", [4.0, 0.0]), pad("B", [10.0, 0.0])];
        let mut parts = BTreeMap::new();
        for (i, r) in ["A", "P", "B"].iter().enumerate() {
            parts.insert(
                r.to_string(),
                Part { reference: r.to_string(), value: String::new(), footprint: String::new(), at: pads[i].centre, rotation: 0.0, back: false, locked: false, courtyard: None, pads: vec![i] },
            );
        }
        let board = Board {
            layers: vec!["F.Cu".into(), "B.Cu".into()],
            parts,
            pads,
            tracks: vec![Track { a: [0.0, 0.0], b: [10.0, 0.0], width: 0.2, layer: "F.Cu".into(), net: "N".into(), locked: false }],
            vias: vec![],
            fills: vec![],
            rules: None,
            outline: None,
        };
        let p = copper_path(&board, "N", "P.1", "B.1").expect("P joins the track");
        assert!((p.length - 6.0).abs() < 1e-9, "{}", p.length);
        let p = copper_path(&board, "N", "A.1", "P.1").unwrap();
        assert!((p.length - 4.0).abs() < 1e-9, "{}", p.length);
    }

    #[test]
    fn via_in_pad_joins_it() {
        // A's pad holds a via; the track leaves it on B.Cu and comes up through a second via
        // inside B's pad. No track end touches either pad on F.Cu.
        let pads = vec![pad("A", [0.0, 0.0]), pad("B", [5.0, 0.0])];
        let mut parts = BTreeMap::new();
        for (i, r) in ["A", "B"].iter().enumerate() {
            parts.insert(
                r.to_string(),
                Part { reference: r.to_string(), value: String::new(), footprint: String::new(), at: pads[i].centre, rotation: 0.0, back: false, locked: false, courtyard: None, pads: vec![i] },
            );
        }
        let via = |x: f64| crate::Via { at: [x, 0.1], size: 0.4, drill: 0.2, layers: vec!["F.Cu".into(), "B.Cu".into()], net: "N".into() };
        let board = Board {
            layers: vec!["F.Cu".into(), "B.Cu".into()],
            parts,
            pads,
            tracks: vec![Track { a: [0.0, 0.1], b: [5.0, 0.1], width: 0.2, layer: "B.Cu".into(), net: "N".into(), locked: false }],
            vias: vec![via(0.0), via(5.0)],
            fills: vec![],
            rules: None,
            outline: None,
        };
        let p = copper_path(&board, "N", "A.1", "B.1").expect("the vias join the pads");
        assert!((p.length - 5.2).abs() < 1e-9, "{}", p.length);
    }
}
