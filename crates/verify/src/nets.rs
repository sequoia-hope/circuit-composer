//! Net-level measurements for interface blocks: copper path between two pads, the share of a
//! net's track with no ground under it, and how far each power pin is from its decoupling.

use std::collections::BTreeMap;

use pcb::geom::{self, P};
use pcb::graph::copper_path;
use pcb::Board;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct PathReport {
    pub from: String,
    pub to: String,
    pub net: String,
    /// Along the copper, pad centre to pad centre, mm.
    pub length_mm: f64,
    /// Layer changes along the path.
    pub vias: usize,
    pub layers: Vec<String>,
}

/// The copper path between two pads on the same net, or `Err` when they are on different nets
/// or not joined.
pub fn path(board: &Board, from: &str, to: &str) -> Result<PathReport, String> {
    let a = board.pad(from).ok_or_else(|| format!("no pad {from}"))?;
    let b = board.pad(to).ok_or_else(|| format!("no pad {to}"))?;
    if a.net != b.net {
        return Err(format!("{from} is on {} and {to} on {}", a.net, b.net));
    }
    let p = copper_path(board, &a.net, from, to).ok_or_else(|| format!("{from} and {to} are not joined by copper on {}", a.net))?;
    let vias = p.layers.windows(2).filter(|w| w[0] != w[1]).count();
    let mut layers: Vec<String> = Vec::new();
    for l in &p.layers {
        if !layers.contains(l) {
            layers.push(l.clone());
        }
    }
    Ok(PathReport { from: from.into(), to: to.into(), net: a.net.clone(), length_mm: p.length, vias, layers })
}

#[derive(Debug, Clone, Serialize)]
pub struct ReturnPathReport {
    pub net: String,
    pub ground: String,
    pub track_mm: f64,
    /// Track length with no `ground` fill on either neighbouring copper layer, mm.
    pub unreferenced_mm: f64,
    /// Where the longest unreferenced stretch starts, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worst_at: Option<P>,
    pub worst_mm: f64,
    /// `[track_mm, unreferenced_mm]` per copper layer.
    pub by_layer: BTreeMap<String, [f64; 2]>,
    /// Other nets' fills on a neighbouring layer under the unreferenced track (a power plane
    /// standing in for ground), with the length over each, mm.
    pub over_other: BTreeMap<String, f64>,
}

/// Walks every track of `net` in `step` mm steps and asks whether a fill of `ground` lies under
/// (or over) the centreline on an adjacent copper layer: the return current's path. Uses the
/// fills as KiCad last computed them, so refill a board before measuring it.
pub fn return_path(board: &Board, net: &str, ground: &str, step: f64) -> ReturnPathReport {
    let li = |name: &str| board.layers.iter().position(|l| l == name);
    let mut fills_by_layer: BTreeMap<usize, Vec<&Vec<P>>> = BTreeMap::new();
    let mut others_by_layer: BTreeMap<usize, Vec<(&str, &Vec<P>)>> = BTreeMap::new();
    for f in board.fills.iter() {
        if let Some(l) = li(&f.layer) {
            if f.net == ground {
                fills_by_layer.entry(l).or_default().push(&f.polygon);
            } else if f.net != net && !f.net.is_empty() {
                others_by_layer.entry(l).or_default().push((&f.net, &f.polygon));
            }
        }
    }
    let neighbours = |layer: usize| [layer.checked_sub(1), Some(layer + 1)].into_iter().flatten();
    let referenced = |layer: usize, p: P| -> bool {
        neighbours(layer)
            .filter_map(|n| fills_by_layer.get(&n))
            .any(|polys| polys.iter().any(|poly| geom::inside(poly, p)))
    };
    let (mut track_mm, mut unreferenced_mm, mut worst_mm, mut worst_at) = (0.0, 0.0, 0.0, None);
    let mut by_layer: BTreeMap<String, [f64; 2]> = BTreeMap::new();
    let mut over_other: BTreeMap<String, f64> = BTreeMap::new();
    for t in board.tracks.iter().filter(|t| t.net == net) {
        let Some(l) = li(&t.layer) else { continue };
        let len = geom::dist(t.a, t.b);
        track_mm += len;
        let layer = by_layer.entry(t.layer.clone()).or_default();
        layer[0] += len;
        let n = ((len / step).ceil() as usize).max(1);
        let (mut run, mut run_start) = (0.0, None);
        for i in 0..n {
            // Sample the middle of each step.
            let p = geom::add(t.a, geom::scale(geom::sub(t.b, t.a), (i as f64 + 0.5) / n as f64));
            if referenced(l, p) {
                run = 0.0;
                run_start = None;
            } else {
                unreferenced_mm += len / n as f64;
                layer[1] += len / n as f64;
                let under = neighbours(l).filter_map(|n| others_by_layer.get(&n)).flatten().find(|(_, poly)| geom::inside(poly, p));
                if let Some((other, _)) = under {
                    *over_other.entry(other.to_string()).or_default() += len / n as f64;
                }
                run += len / n as f64;
                run_start.get_or_insert(p);
                if run > worst_mm {
                    worst_mm = run;
                    worst_at = run_start;
                }
            }
        }
    }
    ReturnPathReport { net: net.into(), ground: ground.into(), track_mm, unreferenced_mm, worst_at, worst_mm, by_layer, over_other }
}

#[derive(Debug, Clone, Serialize)]
pub struct DecouplingReport {
    pub pin: String,
    pub net: String,
    /// The capacitor nearest along the copper, and that distance.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cap: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub length_mm: Option<f64>,
    /// Other pins whose nearest capacitor is the same one.
    pub shares_with: Vec<String>,
}

/// For each pad of `part` on one of `nets`, the nearest capacitor (a two-pad part whose
/// reference starts with `C`, with one pad on that net and the other on `ground`) along the
/// copper.
pub fn decoupling(board: &Board, part: &str, nets: &[String], ground: &str) -> Result<Vec<DecouplingReport>, String> {
    let ic = board.parts.get(part).ok_or_else(|| format!("no part {part}"))?;
    let caps: Vec<(&str, BTreeMap<&str, &str>)> = board
        .parts
        .values()
        .filter(|p| p.reference.starts_with('C') && p.pads.len() == 2)
        .map(|p| (p.reference.as_str(), p.pads.iter().map(|&i| (board.pads[i].net.as_str(), board.pads[i].pin.as_str())).collect()))
        .filter(|(_, by_net): &(&str, BTreeMap<&str, &str>)| by_net.contains_key(ground))
        .collect();
    let mut out = Vec::new();
    for &i in &ic.pads {
        let pad = &board.pads[i];
        if !nets.contains(&pad.net) {
            continue;
        }
        let from = format!("{part}.{}", pad.pin);
        let best = caps
            .iter()
            .filter_map(|(c, by_net)| by_net.get(pad.net.as_str()).map(|pin| (*c, format!("{c}.{pin}"))))
            .filter_map(|(c, to)| copper_path(board, &pad.net, &from, &to).map(|p| (c, p.length)))
            .min_by(|a, b| a.1.total_cmp(&b.1));
        out.push(DecouplingReport {
            pin: pad.pin.clone(),
            net: pad.net.clone(),
            cap: best.map(|b| b.0.to_string()),
            length_mm: best.map(|b| b.1),
            shares_with: Vec::new(),
        });
    }
    out.sort_by_key(|o| natural(&o.pin));
    // Several pads of one pin number (an exposed pad split in pieces) count once.
    out.dedup_by(|a, b| a.pin == b.pin);
    for k in 0..out.len() {
        let shares: Vec<String> = out.iter().filter(|o| o.pin != out[k].pin && o.cap.is_some() && o.cap == out[k].cap).map(|o| o.pin.clone()).collect();
        out[k].shares_with = shares;
    }
    Ok(out)
}

fn natural(pin: &str) -> (u64, String) {
    (pin.parse().unwrap_or(u64::MAX), pin.to_string())
}
