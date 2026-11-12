//! `ccomp floorplan`: a composed board's footprints (courtyards and pads, read from the KiCad
//! board) and its composition (nets, blocks, placement hints) as a floorplanning problem, plus
//! a spec of what the composition cannot say: which connectors go on an edge and which way
//! they face, which generated layouts move as one, which blocks keep apart.

use std::collections::BTreeMap;
use std::path::Path;

use floorplan::{Edge, Net, PartGeom, Pose, Problem, Region, RegionKind, Rigid, Separation, Sides};
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
    /// A footprint locked in the board stays too, listed here or not.
    #[serde(default)]
    pub keep: Vec<String>,
    /// Parts left out of the floorplan altogether (placed by a later step).
    #[serde(default)]
    pub exclude: Vec<String>,
    /// Areas no part may cover: a reserved via field, a cut-out. `side` is front, back or both.
    #[serde(default)]
    pub keepouts: Vec<Keepout>,
    /// Areas blocks and parts must stay inside (`must`) or out of (`never`).
    #[serde(default)]
    pub regions: Vec<RegionSpec>,
}

/// A region, by `polygon` or `rect` (x0, y0, x1, y1). Its members are its `parts` and every part
/// of its `blocks` (a block named `iso_dcdc` holds `iso_dcdc/pri` too); a rigid layout with a part
/// among them comes with all of it. A `never` region with no members keeps every moving part out
/// but those it `except`s (designators or blocks: a transformer straddling a barrier).
#[derive(Debug, Clone, Deserialize)]
pub struct RegionSpec {
    pub name: String,
    #[serde(default)]
    pub polygon: Option<Vec<(f64, f64)>>,
    #[serde(default)]
    pub rect: Option<[f64; 4]>,
    #[serde(default)]
    pub kind: RegionKind,
    #[serde(default)]
    pub side: Sides,
    #[serde(default)]
    pub blocks: Vec<String>,
    #[serde(default)]
    pub parts: Vec<String>,
    #[serde(default)]
    pub except: Vec<String>,
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

/// A generated layout (`layout.json` from `ccomp gen`, or a cluster `lift_layout.py` lifted from
/// a board) and the roles' parts on this board.
#[derive(Debug, Deserialize)]
pub struct RigidSpec {
    pub name: String,
    pub anchor: String,
    pub layout: String,
    /// Layout role → designator (`cin` → `C14`); a role not named here is a designator.
    #[serde(default)]
    pub roles: BTreeMap<String, String>,
}

pub fn problem(board: &Path, composed: &Value, spec: &Spec, root: &Path) -> Result<Problem, String> {
    let b = pcb::Board::load(board)?;
    let comp_parts = composed["parts"].as_object().ok_or("composed: no parts")?;
    let refs: Vec<String> = comp_parts.keys().filter(|r| !spec.exclude.contains(r)).cloned().collect();
    let index: BTreeMap<&str, usize> = refs.iter().enumerate().map(|(i, r)| (r.as_str(), i)).collect();
    let net_of: BTreeMap<String, String> = composed["nets"].as_object().into_iter().flatten().flat_map(|(n, ps)| ps.as_array().into_iter().flatten().filter_map(move |p| Some((p.as_str()?.to_string(), n.clone())))).collect();
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
        // The part's own pin on the near pin's net: the pad that should face it.
        let near_pin = cp["near"].as_str().and_then(|n| net_of.get(n)).and_then(|net| composed["nets"][net.as_str()].as_array()).and_then(|ps| {
            ps.iter().filter_map(|x| x.as_str()?.split_once('.')).find(|(d, pin)| d == r && pins.contains_key(*pin)).map(|(_, pin)| pin.to_string())
        });
        // Kept by the spec, or locked in the board (KiCad's lock, and Composer's when it saves):
        // where the board has it.
        let kept = (spec.keep.contains(r) || p.locked).then_some(floorplan::Pose { x: p.at[0], y: p.at[1], rotation: p.rotation });
        parts.push(PartGeom { reference: r.clone(), bbox, pins, block, near, near_pin, edge: spec.edges.get(r).cloned(), fixed: spec.fixed.get(r).copied().or(kept), back: spec.back.contains(r) || p.back, through });
    }
    // Keep-outs: fixed parts with no pads, on the side they guard (both: a through-hole area).
    for k in &spec.keepouts {
        let [x0, y0, x1, y1] = k.rect;
        let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
        let local = [x0 - cx, y0 - cy, x1 - cx, y1 - cy];
        let pose = Some(floorplan::Pose { x: cx, y: cy, rotation: 0.0 });
        let through = if k.side == "both" { vec![local] } else { vec![] };
        parts.push(PartGeom { reference: format!("keepout:{}", k.name), bbox: local, pins: BTreeMap::new(), block: None, near: None, near_pin: None, edge: None, fixed: pose, back: k.side == "back", through });
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
        nets.push(Net { name: name.clone(), pins: ps, weight: 1.0, max_mm: None, planar: false });
    }
    let mut groups = Vec::new();
    for g in &spec.rigid {
        let text = std::fs::read_to_string(root.join(&g.layout)).map_err(|e| format!("{}: {e}", g.layout))?;
        let lay: Value = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", g.layout))?;
        // `ccomp gen` wraps its layout; `extract_layout.py` and `lift_layout.py` write it bare.
        let lay = if lay.get("layout").is_some() { lay["layout"].clone() } else { lay };
        let lp = lay["parts"].as_object().ok_or_else(|| format!("{}: no parts", g.layout))?;
        let anchor = *index.get(g.anchor.as_str()).ok_or_else(|| format!("rigid {}: anchor {} not on the board", g.name, g.anchor))?;
        let mut members = Vec::new();
        for (role, place) in lp {
            let des = g.roles.get(role).unwrap_or(role);
            let i = *index.get(des.as_str()).ok_or_else(|| format!("rigid {}: no part for role {role} ({des} is not on the board)", g.name))?;
            let at = (place["at"][0].as_f64().unwrap_or(0.0), place["at"][1].as_f64().unwrap_or(0.0));
            // A member on the anchor's other side (a decoupler under a BGA) is on that side here.
            parts[i].back = parts[anchor].back != place["flipped"].as_bool().unwrap_or(false);
            members.push((i, at, place["rotation"].as_f64().unwrap_or(0.0)));
        }
        // The layout's own copper rides with it, as pad-less parts no other courtyard may cover:
        // a track blocks its side, a via both. Inner-layer copper blocks no part.
        let num = |v: &Value| v.as_f64().unwrap_or(0.0);
        for (k, t) in lay["tracks"].as_array().into_iter().flatten().enumerate() {
            let back = match t["layer"].as_str() {
                Some("F.Cu") => false,
                Some("B.Cu") => true,
                _ => continue,
            };
            let (a, z, hw) = ((num(&t["a"][0]), num(&t["a"][1])), (num(&t["b"][0]), num(&t["b"][1])), num(&t["width"]) / 2.0);
            let c = ((a.0 + z.0) / 2.0, (a.1 + z.1) / 2.0);
            let (dx, dy) = ((a.0 - z.0).abs() / 2.0 + hw, (a.1 - z.1).abs() / 2.0 + hw);
            members.push((parts.len(), c, 0.0));
            parts.push(PartGeom { reference: format!("copper:{}:track{k}", g.name), bbox: [-dx, -dy, dx, dy], pins: BTreeMap::new(), block: None, near: None, near_pin: None, edge: None, fixed: None, back, through: vec![] });
        }
        for (k, v) in lay["vias"].as_array().into_iter().flatten().enumerate() {
            let r = num(&v["size"]) / 2.0;
            let bbox = [-r, -r, r, r];
            members.push((parts.len(), (num(&v["at"][0]), num(&v["at"][1])), 0.0));
            parts.push(PartGeom { reference: format!("copper:{}:via{k}", g.name), bbox, pins: BTreeMap::new(), block: None, near: None, near_pin: None, edge: None, fixed: None, back: false, through: vec![bbox] });
        }
        groups.push(Rigid { name: g.name.clone(), anchor, members });
    }
    let mut regions = Vec::new();
    for r in &spec.regions {
        let polygon = match (&r.polygon, r.rect) {
            (Some(p), None) if p.len() >= 3 => p.clone(),
            (None, Some([x0, y0, x1, y1])) => vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1)],
            _ => return Err(format!("region {}: give a polygon of three or more points, or a rect", r.name)),
        };
        // Designators and blocks as part indices, a rigid layout with a part among them whole.
        let resolve = |parts_named: &[String], blocks: &[String]| -> Result<Vec<usize>, String> {
            let mut out: Vec<usize> = Vec::new();
            for d in parts_named {
                out.push(*index.get(d.as_str()).ok_or_else(|| format!("region {}: {d} is not on the board", r.name))?);
            }
            for blk in blocks {
                let had = out.len();
                let prefix = format!("{blk}/");
                out.extend(parts.iter().enumerate().filter(|(_, p)| p.block.as_deref().is_some_and(|b| b == blk || b.starts_with(&prefix))).map(|(i, _)| i));
                if out.len() == had {
                    return Err(format!("region {}: no part is in block {blk}", r.name));
                }
            }
            for g in &groups {
                let all: Vec<usize> = std::iter::once(g.anchor).chain(g.members.iter().map(|m| m.0)).collect();
                if all.iter().any(|i| out.contains(i)) {
                    out.extend(all);
                }
            }
            out.sort_unstable();
            out.dedup();
            Ok(out)
        };
        let mut members = resolve(&r.parts, &r.blocks)?;
        if !r.except.is_empty() {
            if r.kind != RegionKind::Never || !members.is_empty() {
                return Err(format!("region {}: `except` is for a never region with no parts or blocks", r.name));
            }
            let (named, blocks): (Vec<String>, Vec<String>) = r.except.iter().cloned().partition(|e| index.contains_key(e.as_str()));
            let spared = resolve(&named, &blocks)?;
            members = (0..parts.len()).filter(|i| parts[*i].fixed.is_none() && !spared.contains(i)).collect();
        }
        regions.push(Region { name: r.name.clone(), polygon, kind: r.kind, members, side: r.side });
    }
    let mut options = serde_json::to_value(floorplan::Options::default()).unwrap();
    if let Value::Object(o) = &spec.options {
        for (k, v) in o {
            options[k] = v.clone();
        }
    }
    let options = serde_json::from_value(options).map_err(|e| format!("spec options: {e}"))?;
    Ok(Problem { parts, nets, groups, separations: spec.separations.clone(), options, outline: spec.outline, outline_polygon: spec.outline_polygon.clone(), regions })
}

/// A placement promises a net's pins' box, not its route, and routes run longer than the box:
/// a length limit becomes a box of this fraction of it.
pub const LIMIT_BOX: f64 = 0.75;

/// What the board's intent asks of its nets' placement. The intent names nets as KiCad does; the
/// composition as composed (`/mcu/XIN`, `/mcu_XIN`).
/// - Each net's `max_length` (hard or soft), as a limit on its pins' box.
/// - The nets a block keeps over ground (`unreferenced_length_mm <= 0`): they should route on one
///   layer, so they are `planar` and their pins' lines are kept from crossing.
pub fn limit_nets(pr: &mut Problem, intent: &intent::IntentFile) {
    let over_ground: std::collections::BTreeSet<&str> = intent
        .blocks
        .values()
        .filter_map(|b| b.placement_goal.as_ref())
        .flat_map(|g| &g.constraints)
        .filter(|c| c.value.metric == intent::Metric::UnreferencedLengthMm)
        .flat_map(|c| c.value.nets.iter().map(String::as_str))
        .collect();
    for net in &mut pr.nets {
        let kicad = compose::board_intent::kicad_net_name(&net.name);
        let n = intent.nets.get(&net.name).or_else(|| intent.nets.get(&kicad));
        if let Some(l) = n.and_then(|n| n.max_length.as_ref()) {
            net.max_mm = Some(LIMIT_BOX * l.value.mm);
        }
        net.planar = over_ground.contains(net.name.as_str()) || over_ground.contains(kicad.as_str());
    }
}
