//! Checks for a MOSFET half-bridge with its gate driver, and for a current-sense amplifier
//! across its shunt. Parts are named by role; which pad of a MOSFET is drain, source or gate is
//! read from the nets (the drain of the high side is on the bus capacitor, the two switches
//! share the phase node, the source of the low side is on the bus capacitor's other side).
//!
//! Half-bridge:
//! - `hot_loop_area_mm2`: the commutation loop, CBUS+ → high-side drain, through the switch to
//!   its source, the phase node to the low-side drain, through it, and its source back to CBUS−.
//!   Copper paths, joined straight across each part. `hot_loop_area_placed_mm2`: pad centres.
//! - `phase_path_mm`: copper from the high-side source to the low-side drain, the link whose
//!   inductance rings with every edge.
//! - `hs_gate_loop_area_mm2`, `ls_gate_loop_area_mm2`: driver output → gate resistor → gate,
//!   back from the source to the driver's return pin (VS, COM). `hs_gate_path_mm`,
//!   `ls_gate_path_mm`: the forward copper alone.
//! - `bootstrap_loop_area_mm2`, `bootstrap_path_mm`: VB → bootstrap capacitor → VS.
//! - `driver_supply_path_mm`: the driver's VCC pin to its nearest capacitor along the copper.
//!
//! Current sense:
//! - `sense_loop_area_mm2`: the amplifier's + input, its copper to the shunt, across the shunt,
//!   and back to the − input: the loop a changing phase current induces a voltage in.
//! - `sense_path_pos_mm`, `sense_path_neg_mm`: each input's copper to the shunt.
//! - `kelvin_offset_pos_mm`, `kelvin_offset_neg_mm`: where each sense path first meets copper
//!   that carries the load current (a fill of its net, or another part's pad), how far that
//!   tap is from the shunt's pad. 0 is a Kelvin connection: the tap is on the pad, and no
//!   load-carrying copper's drop adds to the reading.

use std::collections::BTreeMap;

use pcb::geom::{self, P};
use pcb::graph::copper_path;
use pcb::{Board, Pad};
use serde::{Deserialize, Serialize};

use crate::{FixStage, Limit, Measurement};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BridgeRoles {
    /// The switches and the bus capacitor across them (parts).
    pub hs: String,
    pub ls: String,
    pub cbus: String,
    /// Driver pins as `REF.PIN`.
    pub ho: String,
    pub lo: String,
    pub vs: String,
    pub vb: String,
    pub com: String,
    pub vcc: String,
    /// Gate resistors and the bootstrap capacitor: found from the nets when not given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rgh: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rgl: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cboot: Option<String>,
}

impl BridgeRoles {
    pub fn from_map(m: &BTreeMap<String, String>) -> Result<BridgeRoles, String> {
        let get = |k: &str| m.get(k).cloned().ok_or_else(|| format!("half-bridge role `{k}` is not given"));
        let known = ["hs", "ls", "cbus", "ho", "lo", "vs", "vb", "com", "vcc", "rgh", "rgl", "cboot"];
        if let Some(k) = m.keys().find(|k| !known.contains(&k.as_str())) {
            return Err(format!("`{k}` is not a half-bridge role"));
        }
        Ok(BridgeRoles {
            hs: get("hs")?,
            ls: get("ls")?,
            cbus: get("cbus")?,
            ho: get("ho")?,
            lo: get("lo")?,
            vs: get("vs")?,
            vb: get("vb")?,
            com: get("com")?,
            vcc: get("vcc")?,
            rgh: m.get("rgh").cloned(),
            rgl: m.get("rgl").cloned(),
            cboot: m.get("cboot").cloned(),
        })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct BridgeNets {
    pub bus: String,
    pub phase: String,
    pub gnd: String,
    pub hs_gate: String,
    pub ls_gate: String,
}

/// The parts found from the nets, and the shapes behind the numbers.
#[derive(Debug, Clone, Default, Serialize)]
pub struct BridgeGeometry {
    pub rgh: String,
    pub rgl: String,
    pub cboot: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cvcc: Option<String>,
    pub hot_loop: Vec<P>,
    pub hs_gate_loop: Vec<P>,
    pub ls_gate_loop: Vec<P>,
    pub bootstrap_loop: Vec<P>,
    pub routed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct BridgeReport {
    pub block: String,
    pub roles: BridgeRoles,
    pub nets: BridgeNets,
    pub measurements: Vec<Measurement>,
    pub geometry: BridgeGeometry,
}

fn pad<'a>(b: &'a Board, ref_pin: &str) -> Result<&'a Pad, String> {
    b.pad(ref_pin).ok_or_else(|| format!("pad {ref_pin} is not on the board"))
}

fn pads<'a>(b: &'a Board, part: &str) -> Result<Vec<&'a Pad>, String> {
    let p = b.parts.get(part).ok_or_else(|| format!("part {part} is not on the board"))?;
    Ok(p.pads.iter().map(|&i| &b.pads[i]).collect())
}

/// `part`'s pin on `net`, as `REF.PIN`.
fn pin_on(b: &Board, part: &str, net: &str) -> Result<String, String> {
    pads(b, part)?.into_iter().find(|p| p.net == net).map(|p| format!("{part}.{}", p.pin)).ok_or_else(|| format!("{part} has no pad on net `{net}`"))
}

/// The two-pad part joining nets `a` and `b` whose reference starts with `prefix`; an error
/// unless there is exactly one.
fn bridging(b: &Board, a: &str, c: &str, prefix: &str, what: &str) -> Result<String, String> {
    let found: Vec<&str> = b
        .parts
        .values()
        .filter(|p| p.reference.starts_with(prefix) && p.pads.len() == 2)
        .filter(|p| {
            let n: Vec<&str> = p.pads.iter().map(|&i| b.pads[i].net.as_str()).collect();
            (n[0] == a && n[1] == c) || (n[0] == c && n[1] == a)
        })
        .map(|p| p.reference.as_str())
        .collect();
    match found[..] {
        [one] => Ok(one.to_string()),
        [] => Err(format!("no {what} joins `{a}` and `{c}`; name it")),
        _ => Err(format!("{} could each be the {what} between `{a}` and `{c}`; name one", found.join(", "))),
    }
}

/// The mean of a pin's pad centres: where a part's terminal is, for the placed loop.
fn centre(b: &Board, ref_pin: &str) -> P {
    let (r, pin) = ref_pin.split_once('.').unwrap_or_default();
    let cs: Vec<P> = b.parts.get(r).map(|p| p.pads.iter().map(|&i| &b.pads[i]).filter(|p| p.pin == pin).map(|p| p.centre).collect()).unwrap_or_default();
    let n = cs.len().max(1) as f64;
    cs.iter().fold([0.0, 0.0], |s, c| geom::add(s, geom::scale(*c, 1.0 / n)))
}

/// A loop of copper legs `(net, from, to)`, each the shortest copper path, consecutive legs
/// joined straight (through a part). Unrouted legs are pad centres. Returns the points, whether
/// every leg was routed, and the legs' total length.
fn legs_loop(b: &Board, legs: &[(&str, &str, &str)]) -> (Vec<P>, bool, f64) {
    let (mut pts, mut routed, mut length) = (Vec::new(), true, 0.0);
    for (net, from, to) in legs {
        match copper_path(b, net, from, to) {
            Some(p) => {
                length += p.length;
                pts.extend(p.points);
            }
            None => {
                routed = false;
                let (a, c) = (centre(b, from), centre(b, to));
                length += geom::dist(a, c);
                pts.extend([a, c]);
            }
        }
    }
    (pts, routed, length)
}

#[allow(clippy::too_many_arguments)]
fn measurement(block: &str, check: &'static str, nets: &[&str], location: P, measured: f64, unit: &'static str, fix: FixStage, method: &str) -> Measurement {
    Measurement {
        check,
        block: block.to_string(),
        nets: nets.iter().map(|s| s.to_string()).collect(),
        location,
        measured,
        unit,
        limit: None,
        passed: None,
        fix_stage: fix,
        method: Some(method.to_string()),
    }
}

fn routed_note(routed: bool, how: &str) -> String {
    if routed { how.to_string() } else { "pad centres: a leg is unrouted".to_string() }
}

pub fn check(board: &Board, block: &str, roles: &BridgeRoles, limits: &BTreeMap<String, Limit>) -> Result<BridgeReport, String> {
    let (hs, ls, cbus) = (pads(board, &roles.hs)?, pads(board, &roles.ls)?, pads(board, &roles.cbus)?);
    if cbus.len() != 2 {
        return Err(format!("bus capacitor {} has {} pads", roles.cbus, cbus.len()));
    }
    // The bus is the capacitor's net the high side is on; ground is its other.
    let (bus, gnd) = match (hs.iter().any(|p| p.net == cbus[0].net), hs.iter().any(|p| p.net == cbus[1].net)) {
        (true, false) => (cbus[0].net.clone(), cbus[1].net.clone()),
        (false, true) => (cbus[1].net.clone(), cbus[0].net.clone()),
        _ => return Err(format!("{} is not across {}'s drain and {}'s source", roles.cbus, roles.hs, roles.ls)),
    };
    if !ls.iter().any(|p| p.net == gnd) {
        return Err(format!("{}'s source is not on {gnd}, the bus capacitor's other side", roles.ls));
    }
    let phase = hs.iter().map(|p| p.net.clone()).find(|n| *n != bus && ls.iter().any(|q| q.net == *n)).ok_or_else(|| format!("{} and {} share no phase node", roles.hs, roles.ls))?;
    let gate_of = |fet: &[&Pad], skip: [&str; 2], who: &str| fet.iter().map(|p| p.net.clone()).find(|n| !skip.contains(&n.as_str())).ok_or_else(|| format!("{who} has no gate pad"));
    let hs_gate = gate_of(&hs, [&bus, &phase], &roles.hs)?;
    let ls_gate = gate_of(&ls, [&gnd, &phase], &roles.ls)?;
    let nets = BridgeNets { bus: bus.clone(), phase: phase.clone(), gnd: gnd.clone(), hs_gate: hs_gate.clone(), ls_gate: ls_gate.clone() };

    let (ho, lo, vs, vb, com, vcc) =
        (pad(board, &roles.ho)?, pad(board, &roles.lo)?, pad(board, &roles.vs)?, pad(board, &roles.vb)?, pad(board, &roles.com)?, pad(board, &roles.vcc)?);
    if vs.net != phase {
        return Err(format!("{} (VS) is on {}, not the phase node {phase}", roles.vs, vs.net));
    }
    let rgh = match &roles.rgh {
        Some(r) => r.clone(),
        None => bridging(board, &ho.net, &hs_gate, "R", "high-side gate resistor")?,
    };
    let rgl = match &roles.rgl {
        Some(r) => r.clone(),
        None => bridging(board, &lo.net, &ls_gate, "R", "low-side gate resistor")?,
    };
    let cboot = match &roles.cboot {
        Some(c) => c.clone(),
        None => bridging(board, &vb.net, &phase, "C", "bootstrap capacitor")?,
    };

    let p = |part: &str, net: &str| pin_on(board, part, net);
    let (hs_d, hs_s, hs_g) = (p(&roles.hs, &bus)?, p(&roles.hs, &phase)?, p(&roles.hs, &hs_gate)?);
    let (ls_d, ls_s, ls_g) = (p(&roles.ls, &phase)?, p(&roles.ls, &gnd)?, p(&roles.ls, &ls_gate)?);
    let (cb_p, cb_n) = (p(&roles.cbus, &bus)?, p(&roles.cbus, &gnd)?);

    let mut out = Vec::new();
    let mut geo = BridgeGeometry { rgh: rgh.clone(), rgl: rgl.clone(), cboot: cboot.clone(), ..Default::default() };

    // Commutation loop.
    let (hot, routed, _) = legs_loop(board, &[(&bus, &cb_p, &hs_d), (&phase, &hs_s, &ls_d), (&gnd, &ls_s, &cb_n)]);
    let how = routed_note(routed, "copper paths on the bus, phase and ground nets, joined through the bus capacitor and each switch");
    out.push(measurement(block, "hot_loop_area_mm2", &[&bus, &phase, &gnd], geom::centroid(&hot), geom::loop_area(&hot, 400), "mm2", FixStage::Block, &how));
    let placed: Vec<P> = [&cb_p, &hs_d, &hs_s, &ls_d, &ls_s, &cb_n].iter().map(|r| centre(board, r)).collect();
    out.push(measurement(block, "hot_loop_area_placed_mm2", &[&bus, &phase, &gnd], geom::centroid(&placed), geom::loop_area(&placed, 400), "mm2", FixStage::Floorplan, "pad centres (a pin's pads averaged)"));
    geo.hot_loop = hot;
    geo.routed = routed;

    let (phase_link, routed_p, phase_len) = legs_loop(board, &[(&phase, &hs_s, &ls_d)]);
    out.push(measurement(block, "phase_path_mm", &[&phase], geom::centroid(&phase_link), phase_len, "mm", FixStage::Block, &routed_note(routed_p, "shortest copper path, high-side source pads to low-side drain pads")));

    // Gate loops: out through the resistor, back from the source to the driver's return.
    for (check, path_check, out_pin, r, gate_net, gate_pin, src, ret_net, ret_pin, is_hs) in [
        ("hs_gate_loop_area_mm2", "hs_gate_path_mm", &roles.ho, &rgh, &hs_gate, &hs_g, &hs_s, &phase, &roles.vs, true),
        ("ls_gate_loop_area_mm2", "ls_gate_path_mm", &roles.lo, &rgl, &ls_gate, &ls_g, &ls_s, &gnd, &roles.com, false),
    ] {
        let drive_net = &pad(board, out_pin)?.net;
        let (r_in, r_out) = (p(r, drive_net)?, p(r, gate_net)?);
        let (fwd, routed_f, fwd_len) = legs_loop(board, &[(drive_net, out_pin, &r_in), (gate_net, &r_out, gate_pin)]);
        let (ret, _, _) = legs_loop(board, &[(ret_net, src, ret_pin)]);
        let lp: Vec<P> = fwd.iter().chain(ret.iter()).copied().collect();
        let how = routed_note(routed_f, "copper from the driver pin through the gate resistor to the gate, back from the source to the driver's return pin");
        out.push(measurement(block, check, &[drive_net, gate_net, ret_net], geom::centroid(&lp), geom::loop_area(&lp, 400), "mm2", FixStage::Block, &how));
        out.push(measurement(block, path_check, &[drive_net, gate_net], centre(board, gate_pin), fwd_len, "mm", FixStage::Block, &routed_note(routed_f, "copper, driver pin to gate pad, through the gate resistor")));
        if is_hs {
            geo.hs_gate_loop = lp;
        } else {
            geo.ls_gate_loop = lp;
        }
    }

    // Bootstrap.
    let (cb_vb, cb_vs) = (p(&cboot, &vb.net)?, p(&cboot, &phase)?);
    let (boot, routed_b, boot_len) = legs_loop(board, &[(&vb.net, &roles.vb, &cb_vb), (&phase, &cb_vs, &roles.vs)]);
    let how = routed_note(routed_b, "copper VB → bootstrap capacitor → VS");
    out.push(measurement(block, "bootstrap_loop_area_mm2", &[&vb.net, &phase], geom::centroid(&boot), geom::loop_area(&boot, 400), "mm2", FixStage::Block, &how));
    out.push(measurement(block, "bootstrap_path_mm", &[&vb.net, &phase], vb.centre, boot_len, "mm", FixStage::Block, &how));
    geo.bootstrap_loop = boot;

    // Driver supply.
    let driver = roles.vcc.split_once('.').map_or("", |(r, _)| r);
    let dec = crate::nets::decoupling(board, driver, std::slice::from_ref(&vcc.net), &com.net)?;
    if let Some(d) = dec.iter().find(|d| d.pin == roles.vcc.split_once('.').map_or("", |(_, p)| p)) {
        if let (Some(c), Some(len)) = (&d.cap, d.length_mm) {
            geo.cvcc = Some(c.clone());
            out.push(measurement(block, "driver_supply_path_mm", &[&vcc.net], vcc.centre, len, "mm", FixStage::Block, &format!("copper, VCC pin to {c}, the nearest capacitor to {} along it", com.net)));
        }
    }

    for x in &mut out {
        x.judge(limits.get(x.check).copied());
    }
    Ok(BridgeReport { block: block.into(), roles: roles.clone(), nets, measurements: out, geometry: geo })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SenseRoles {
    /// The amplifier's inputs, `REF.PIN`.
    pub inp: String,
    pub inn: String,
    /// The shunt, or shunts in parallel (`R20+R23`).
    pub shunt: Vec<String>,
}

impl SenseRoles {
    pub fn from_map(m: &BTreeMap<String, String>) -> Result<SenseRoles, String> {
        let get = |k: &str| m.get(k).cloned().ok_or_else(|| format!("current-sense role `{k}` is not given"));
        if let Some(k) = m.keys().find(|k| !["inp", "inn", "shunt"].contains(&k.as_str())) {
            return Err(format!("`{k}` is not a current-sense role"));
        }
        Ok(SenseRoles { inp: get("inp")?, inn: get("inn")?, shunt: get("shunt")?.split('+').map(str::to_string).collect() })
    }

    pub fn amp(&self) -> &str {
        self.inp.split_once('.').map_or("", |(r, _)| r)
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct SenseGeometry {
    pub loop_: Vec<P>,
    /// Where each input's path first meets load-carrying copper.
    pub taps: Vec<P>,
    /// The shunt pad each input's path reaches.
    pub ends: Vec<String>,
    pub routed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SenseReport {
    pub block: String,
    pub roles: SenseRoles,
    pub nets: [String; 2],
    pub measurements: Vec<Measurement>,
    pub geometry: SenseGeometry,
}

pub fn sense(board: &Board, block: &str, roles: &SenseRoles, limits: &BTreeMap<String, Limit>) -> Result<SenseReport, String> {
    let (inp, inn) = (pad(board, &roles.inp)?, pad(board, &roles.inn)?);
    let amp = roles.amp();
    for s in &roles.shunt {
        let n: Vec<&str> = pads(board, s)?.iter().map(|p| p.net.as_str()).collect();
        if !(n.contains(&inp.net.as_str()) && n.contains(&inn.net.as_str())) {
            return Err(format!("shunt {s} is not across {} ({}) and {} ({})", roles.inp, inp.net, roles.inn, inn.net));
        }
    }
    let mut out = Vec::new();
    let mut geo = SenseGeometry { routed: true, ..Default::default() };
    let mut legs: Vec<(Vec<P>, String)> = Vec::new();
    for (side, input) in [("pos", inp), ("neg", inn)] {
        let from = format!("{amp}.{}", input.pin);
        // The shunt pad this input's copper reaches first.
        let best = roles
            .shunt
            .iter()
            .filter_map(|s| pin_on(board, s, &input.net).ok())
            .filter_map(|to| copper_path(board, &input.net, &from, &to).map(|p| (to, p)))
            .min_by(|a, b| a.1.length.total_cmp(&b.1.length));
        let (to, path) = match best {
            Some(b) => b,
            None => {
                geo.routed = false;
                let to = pin_on(board, &roles.shunt[0], &input.net)?;
                let pts = vec![input.centre, centre(board, &to)];
                let layer = input.layers.first().cloned().unwrap_or_default();
                (to, pcb::graph::Path { length: geom::dist(pts[0], pts[1]), points: pts, layers: vec![layer.clone(), layer] })
            }
        };
        let check_path = if side == "pos" { "sense_path_pos_mm" } else { "sense_path_neg_mm" };
        out.push(measurement(block, check_path, &[&input.net], input.centre, path.length, "mm", FixStage::Block, &routed_note(geo.routed, "copper, amplifier input to the nearest shunt pad")));

        // The tap: the first point along the path inside a fill of the net or another part's
        // pad (the shunt's included), on the path's layer.
        let shunt_shapes: Vec<&Vec<P>> = roles.shunt.iter().flat_map(|s| board.parts[s].pads.iter().map(|&i| &board.pads[i])).filter(|p| p.net == input.net).flat_map(|p| p.shape.iter()).collect();
        let loaded = |q: P, layer: &str| {
            board.fills.iter().any(|f| f.net == input.net && f.layer == layer && geom::inside(&f.polygon, q))
                || board.pads.iter().any(|p| p.net == input.net && p.part != amp && p.layers.iter().any(|l| l == layer) && p.shape.iter().any(|s| geom::inside(s, q)))
        };
        let mut tap = *path.points.last().unwrap();
        'walk: for w in 0..path.points.len().saturating_sub(1) {
            let (a, c) = (path.points[w], path.points[w + 1]);
            if path.layers[w] != path.layers[w + 1] {
                continue;
            }
            let n = (geom::dist(a, c) / 0.02).ceil().max(1.0) as usize;
            for k in 0..=n {
                let q = geom::add(a, geom::scale(geom::sub(c, a), k as f64 / n as f64));
                if loaded(q, &path.layers[w]) {
                    tap = q;
                    break 'walk;
                }
            }
        }
        let offset = shunt_shapes.iter().map(|s| if geom::inside(s, tap) { 0.0 } else { geom::point_poly_dist(tap, s) }).fold(f64::INFINITY, f64::min);
        let check_tap = if side == "pos" { "kelvin_offset_pos_mm" } else { "kelvin_offset_neg_mm" };
        out.push(measurement(
            block,
            check_tap,
            &[&input.net],
            tap,
            offset,
            "mm",
            FixStage::Route,
            "where the input's path first enters a fill of its net or another part's pad, to the nearest shunt pad; 0 is a tap on the pad",
        ));
        geo.taps.push(tap);
        geo.ends.push(to);
        legs.push((path.points, input.net.clone()));
    }
    // The loop: + input to its shunt pad, across the shunt, back from the other pad to − .
    let mut lp = legs[0].0.clone();
    lp.extend(legs[1].0.iter().rev().copied());
    out.push(measurement(
        block,
        "sense_loop_area_mm2",
        &[&inp.net, &inn.net],
        geom::centroid(&lp),
        geom::loop_area(&lp, 400),
        "mm2",
        FixStage::Route,
        &routed_note(geo.routed, "copper of both inputs to the shunt, closed across the shunt and the amplifier"),
    ));
    geo.loop_ = lp;
    for x in &mut out {
        x.judge(limits.get(x.check).copied());
    }
    Ok(SenseReport { block: block.into(), roles: roles.clone(), nets: [inp.net.clone(), inn.net.clone()], measurements: out, geometry: geo })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pcb::{Fill, Part, Track};

    /// A board of 1 mm square F.Cu pads, `(part, pin, net, at)`.
    fn board(pads: &[(&str, &str, &str, P)], tracks: Vec<Track>, fills: Vec<Fill>) -> Board {
        let mut parts: BTreeMap<String, Part> = BTreeMap::new();
        let pads: Vec<Pad> = pads
            .iter()
            .enumerate()
            .map(|(i, &(part, pin, net, at))| {
                parts
                    .entry(part.to_string())
                    .or_insert_with(|| Part { reference: part.into(), value: String::new(), footprint: String::new(), at, rotation: 0.0, back: false, locked: false, courtyard: None, pads: vec![] })
                    .pads
                    .push(i);
                let sq = vec![[at[0] - 0.5, at[1] - 0.5], [at[0] + 0.5, at[1] - 0.5], [at[0] + 0.5, at[1] + 0.5], [at[0] - 0.5, at[1] + 0.5]];
                Pad { part: part.into(), pin: pin.into(), net: net.into(), layers: vec!["F.Cu".into()], centre: at, shape: vec![sq] }
            })
            .collect();
        Board { layers: vec!["F.Cu".into(), "B.Cu".into()], parts, pads, tracks, vias: vec![], fills, rules: None, outline: None }
    }

    fn track(net: &str, a: P, b: P) -> Track {
        Track { a, b, width: 0.2, layer: "F.Cu".into(), net: net.into(), locked: false }
    }

    #[test]
    fn half_bridge_finds_its_parts_and_measures_the_commutation_loop() {
        // The loop is a 4 x 4 square: C1 on the left, the switches stacked on the right.
        let b = board(
            &[
                ("C1", "1", "VM", [0.0, 0.0]),
                ("C1", "2", "GND", [0.0, 4.0]),
                ("Q1", "3", "VM", [4.0, 0.0]),
                ("Q1", "1", "PH", [4.0, 2.0]),
                ("Q1", "2", "GH", [5.0, 0.5]),
                ("Q2", "3", "PH", [4.0, 2.0]),
                ("Q2", "1", "GND", [4.0, 4.0]),
                ("Q2", "2", "GL", [5.0, 3.5]),
                ("U1", "7", "HO", [10.0, 0.0]),
                ("U1", "6", "PH", [10.0, 2.0]),
                ("U1", "8", "VB", [10.0, 1.0]),
                ("U1", "5", "LO", [10.0, 4.0]),
                ("U1", "4", "GND", [10.0, 5.0]),
                ("U1", "1", "VCC", [10.0, 6.0]),
                ("R1", "1", "HO", [8.0, 0.0]),
                ("R1", "2", "GH", [7.0, 0.0]),
                ("R2", "1", "LO", [8.0, 4.0]),
                ("R2", "2", "GL", [7.0, 4.0]),
                ("R9", "1", "HO", [8.0, -2.0]),
                ("R9", "2", "X", [7.0, -2.0]),
                ("C2", "1", "VB", [12.0, 1.0]),
                ("C2", "2", "PH", [12.0, 2.0]),
                ("C3", "1", "VCC", [12.0, 6.0]),
                ("C3", "2", "GND", [12.0, 7.0]),
            ],
            vec![track("VCC", [10.0, 6.0], [12.0, 6.0])],
            vec![],
        );
        let roles: BTreeMap<String, String> =
            [("hs", "Q1"), ("ls", "Q2"), ("cbus", "C1"), ("ho", "U1.7"), ("lo", "U1.5"), ("vs", "U1.6"), ("vb", "U1.8"), ("com", "U1.4"), ("vcc", "U1.1")]
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();
        let r = check(&b, "hb", &BridgeRoles::from_map(&roles).unwrap(), &BTreeMap::new()).unwrap();
        assert_eq!((r.nets.bus.as_str(), r.nets.phase.as_str(), r.nets.gnd.as_str()), ("VM", "PH", "GND"));
        assert_eq!((r.geometry.rgh.as_str(), r.geometry.rgl.as_str(), r.geometry.cboot.as_str()), ("R1", "R2", "C2"));
        let m = |c: &str| r.measurements.iter().find(|m| m.check == c).unwrap().measured;
        assert!((m("hot_loop_area_placed_mm2") - 16.0).abs() < 0.2, "{}", m("hot_loop_area_placed_mm2"));
        assert!((m("hot_loop_area_mm2") - 16.0).abs() < 0.2, "{}", m("hot_loop_area_mm2"));
        assert!((m("driver_supply_path_mm") - 2.0).abs() < 1e-9);
        assert_eq!(r.geometry.cvcc.as_deref(), Some("C3"));
    }

    #[test]
    fn a_sense_line_into_the_pour_is_not_kelvin() {
        // + runs straight onto its shunt pad; − enters a fill of its net 1 mm before the pad.
        let fill = Fill { net: "N".into(), layer: "F.Cu".into(), polygon: vec![[5.0, -1.5], [7.0, -1.5], [7.0, 1.0], [5.0, 1.0]] };
        let b = board(
            &[("U5", "8", "P", [0.0, -5.0]), ("U5", "1", "N", [6.0, -5.0]), ("RS", "2", "P", [0.0, 0.0]), ("RS", "1", "N", [6.0, 0.0])],
            vec![track("P", [0.0, -5.0], [0.0, 0.0]), track("N", [6.0, -5.0], [6.0, 0.0])],
            vec![fill],
        );
        let roles: BTreeMap<String, String> = [("inp", "U5.8"), ("inn", "U5.1"), ("shunt", "RS")].iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        let r = sense(&b, "s", &SenseRoles::from_map(&roles).unwrap(), &BTreeMap::new()).unwrap();
        let m = |c: &str| r.measurements.iter().find(|m| m.check == c).unwrap().measured;
        assert_eq!(m("kelvin_offset_pos_mm"), 0.0);
        assert!((m("kelvin_offset_neg_mm") - 1.0).abs() < 0.03, "{}", m("kelvin_offset_neg_mm"));
        assert!((m("sense_path_pos_mm") - 5.0).abs() < 1e-9);
        assert!((m("sense_loop_area_mm2") - 30.0).abs() < 0.5, "{}", m("sense_loop_area_mm2"));
    }
}
