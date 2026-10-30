//! `ccomp floorplan`: a composed board's footprints (courtyards and pads, read from the KiCad
//! board) and its composition (nets, blocks, placement hints) as a floorplanning problem, plus
//! a spec of what the composition cannot say: which connectors go on an edge and which way
//! they face, which generated layouts move as one, which blocks keep apart.

use std::collections::BTreeMap;
use std::path::Path;

use floorplan::{Edge, Net, PartGeom, Pose, Problem, Rigid, Separation};
use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Default, Deserialize)]
pub struct Spec {
    /// Overrides of the floorplanner's options.
    #[serde(default)]
    pub options: Value,
    #[serde(default)]
    pub edges: BTreeMap<String, Edge>,
    #[serde(default)]
    pub fixed: BTreeMap<String, Pose>,
    #[serde(default)]
    pub separations: Vec<Separation>,
    #[serde(default)]
    pub rigid: Vec<RigidSpec>,
    /// Board parts put in a block (`J1` → `usb_in`), so they stay together.
    #[serde(default)]
    pub blocks: BTreeMap<String, String>,
    /// Nets left out of the wirelength; nets with more than `max_net_pins` pins are too.
    #[serde(default)]
    pub skip_nets: Vec<String>,
    #[serde(default = "max_pins")]
    pub max_net_pins: usize,
    /// Parts on the bottom side.
    #[serde(default)]
    pub back: Vec<String>,
    /// The board outline the mechanical design sets (x0, y0, x1, y1).
    #[serde(default)]
    pub outline: Option<[f64; 4]>,
    /// An outline of any shape, as a closed polygon.
    #[serde(default)]
    pub outline_polygon: Option<Vec<(f64, f64)>>,
    /// Parts that stay where the board has them (side and pose), placed by a person or a script.
    #[serde(default)]
    pub keep: Vec<String>,
    /// Parts left out of the floorplan altogether (placed by a later step).
    #[serde(default)]
    pub exclude: Vec<String>,
    /// Areas no part may cover: a reserved via field, a cut-out. `side` is front, back or both.
    #[serde(default)]
    pub keepouts: Vec<Keepout>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Keepout {
    pub name: String,
    pub rect: [f64; 4],
    #[serde(default = "both")]
    pub side: String,
}

fn both() -> String {
    "both".into()
}

fn max_pins() -> usize {
    12
}

/// A generated layout (`layout.json` from `ccomp gen`) and the roles' parts on this board.
#[derive(Debug, Deserialize)]
pub struct RigidSpec {
    pub name: String,
    pub anchor: String,
    pub layout: String,
    /// Layout role → designator (`cin` → `C14`).
    pub roles: BTreeMap<String, String>,
}

pub fn problem(board: &Path, composed: &Value, spec: &Spec, root: &Path) -> Result<Problem, String> {
    let b = pcb::Board::load(board)?;
    let comp_parts = composed["parts"].as_object().ok_or("composed: no parts")?;
    let refs: Vec<String> = comp_parts.keys().filter(|r| !spec.exclude.contains(r)).cloned().collect();
    let index: BTreeMap<&str, usize> = refs.iter().enumerate().map(|(i, r)| (r.as_str(), i)).collect();
    let mut parts = Vec::new();
    for r in &refs {
        let p = b.parts.get(r).ok_or_else(|| format!("{r} is in the composition but not on {}", board.display()))?;
        // The footprint's own frame: undo the rotation, and the mirror of a bottom-side part
        // (KiCad's flip mirrors y in the footprint's frame).
        let local = |q: pcb::geom::P| {
            let l = floorplan::rot((q[0] - p.at[0], q[1] - p.at[1]), -p.rotation);
            if p.back {
                (l.0, -l.1)
            } else {
                l
            }
        };
        let mut pins: BTreeMap<String, (f64, f64)> = BTreeMap::new();
        let mut pad_box = [f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY];
        let mut through = Vec::new();
        for &k in &p.pads {
            let pad = &b.pads[k];
            let c = local(pad.centre);
            pins.entry(pad.pin.clone()).or_insert(c);
            // A plated through-hole pad reaches both outer layers: grown by a 0.2 mm clearance.
            if pad.layers.iter().any(|l| l == "F.Cu") && pad.layers.iter().any(|l| l == "B.Cu") {
                let mut tb = [f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY];
                for q in pad.shape.iter().flatten() {
                    let l = local(*q);
                    tb = [tb[0].min(l.0), tb[1].min(l.1), tb[2].max(l.0), tb[3].max(l.1)];
                }
                if tb[0].is_finite() {
                    through.push([tb[0] - 0.2, tb[1] - 0.2, tb[2] + 0.2, tb[3] + 0.2]);
                }
            }
            for s in &pad.shape {
                for q in s {
                    let l = local(*q);
                    pad_box = [pad_box[0].min(l.0), pad_box[1].min(l.1), pad_box[2].max(l.0), pad_box[3].max(l.1)];
                }
            }
        }
        let bbox = match &p.courtyard {
            Some(h) if !h.is_empty() => {
                let mut bb = [f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY];
                for q in h {
                    let l = local(*q);
                    bb = [bb[0].min(l.0), bb[1].min(l.1), bb[2].max(l.0), bb[3].max(l.1)];
                }
                // Pads past their own courtyard (a USB-C's signal row, a USON's) count too, grown
                // by half the copper clearance so two such parts keep the full rule between pads.
                if pad_box[0].is_finite() {
                    let g = 0.07;
                    bb = [bb[0].min(pad_box[0] - g), bb[1].min(pad_box[1] - g), bb[2].max(pad_box[2] + g), bb[3].max(pad_box[3] + g)];
                }
                bb
            }
            // No courtyard: the pads, grown by KiCad's default courtyard margin.
            _ if pad_box[0].is_finite() => [pad_box[0] - 0.25, pad_box[1] - 0.25, pad_box[2] + 0.25, pad_box[3] + 0.25],
            _ => [-0.5, -0.5, 0.5, 0.5],
        };
        let cp = &comp_parts[r];
        let block = spec.blocks.get(r).cloned().or_else(|| match (cp["instance"].as_str(), cp["block"].as_str()) {
            (Some(i), Some(bk)) => Some(format!("{i}/{bk}")),
            (Some(i), None) => Some(i.to_string()),
            _ => None,
        });
        let near = cp["near"].as_str().and_then(|n| n.split_once('.')).and_then(|(d, pin)| index.get(d).map(|&j| (j, pin.to_string())));
        let kept = spec.keep.contains(r).then_some(floorplan::Pose { x: p.at[0], y: p.at[1], rotation: p.rotation });
        parts.push(PartGeom { reference: r.clone(), bbox, pins, block, near, edge: spec.edges.get(r).cloned(), fixed: spec.fixed.get(r).copied().or(kept), back: spec.back.contains(r) || p.back, through });
    }
    // Keep-outs: fixed parts with no pads, on the side they guard (both: a through-hole area).
    for k in &spec.keepouts {
        let [x0, y0, x1, y1] = k.rect;
        let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
        let local = [x0 - cx, y0 - cy, x1 - cx, y1 - cy];
        let pose = Some(floorplan::Pose { x: cx, y: cy, rotation: 0.0 });
        let through = if k.side == "both" { vec![local] } else { vec![] };
        parts.push(PartGeom { reference: format!("keepout:{}", k.name), bbox: local, pins: BTreeMap::new(), block: None, near: None, edge: None, fixed: pose, back: k.side == "back", through });
    }
    for r in spec.edges.keys().chain(spec.fixed.keys()).chain(spec.blocks.keys()).chain(spec.back.iter()).chain(spec.keep.iter()) {
        if !index.contains_key(r.as_str()) {
            return Err(format!("spec names {r}, which is not on the board"));
        }
    }
    let mut nets = Vec::new();
    for (name, pins) in composed["nets"].as_object().ok_or("composed: no nets")? {
        let ps: Vec<(usize, String)> = pins.as_array().unwrap().iter().filter_map(|p| p.as_str()?.split_once('.').and_then(|(d, pin)| index.get(d).map(|&i| (i, pin.to_string())))).collect();
        if ps.len() < 2 || ps.len() > spec.max_net_pins || spec.skip_nets.contains(name) {
            continue;
        }
        nets.push(Net { name: name.clone(), pins: ps, weight: 1.0 });
    }
    let mut groups = Vec::new();
    for g in &spec.rigid {
        let text = std::fs::read_to_string(root.join(&g.layout)).map_err(|e| format!("{}: {e}", g.layout))?;
        let lay: Value = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", g.layout))?;
        let lp = lay["layout"]["parts"].as_object().ok_or_else(|| format!("{}: no layout.parts", g.layout))?;
        let anchor = *index.get(g.anchor.as_str()).ok_or_else(|| format!("rigid {}: anchor {} not on the board", g.name, g.anchor))?;
        let mut members = Vec::new();
        for (role, place) in lp {
            let des = g.roles.get(role).ok_or_else(|| format!("rigid {}: no part for role {role}", g.name))?;
            let i = *index.get(des.as_str()).ok_or_else(|| format!("rigid {}: {des} not on the board", g.name))?;
            let at = (place["at"][0].as_f64().unwrap_or(0.0), place["at"][1].as_f64().unwrap_or(0.0));
            members.push((i, at, place["rotation"].as_f64().unwrap_or(0.0)));
        }
        groups.push(Rigid { name: g.name.clone(), anchor, members });
    }
    let mut options = serde_json::to_value(floorplan::Options::default()).unwrap();
    if let Value::Object(o) = &spec.options {
        for (k, v) in o {
            options[k] = v.clone();
        }
    }
    let options = serde_json::from_value(options).map_err(|e| format!("spec options: {e}"))?;
    Ok(Problem { parts, nets, groups, separations: spec.separations.clone(), options, outline: spec.outline, outline_polygon: spec.outline_polygon.clone() })
}
