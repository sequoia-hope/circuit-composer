//! Checks for protection at a connector, and for an ideal-diode controller with its MOSFET.
//!
//! Protection (a TVS, or a clamp array's pin, on a connector net):
//! - `connector_to_tvs_mm`: copper from the connector pad to the TVS pad.
//! - `vias_before_tvs`: layer changes on that path, not counting a through-hole pad's own.
//! - `tvs_lead_mm`: how much sooner the surge reaches the TVS than the first other pad on the
//!   net (copper distance from the connector to that pad, less to the TVS). Negative: something
//!   else is reached first.
//! - `tvs_stub_mm`: how far the TVS sits off the path from the connector to that first pad,
//!   `(d(conn, tvs) + d(tvs, first) - d(conn, first)) / 2`; 0 when the line runs over the TVS pad.
//! - `clamp_loop_area_mm2`: connector pad → TVS → its ground pad → the connector's ground pad.
//! - `tvs_ground_via_mm`: from the TVS's ground pad to the nearest ground via (0 in the pad);
//!   `tvs_ground_vias`: ground vias within 1 mm of that pad.
//!
//! Ideal diode (a controller that senses its MOSFET's drain-source voltage):
//! - `gate_path_mm`: copper, GATE pin to the MOSFET's gate.
//! - `anode_path_mm`, `cathode_path_mm`: copper, each sense pin to its MOSFET pad.
//! - `anode_kelvin_offset_mm`, `cathode_kelvin_offset_mm`: where each sense path first meets
//!   copper carrying the load current, from the MOSFET's pad (0 on it). Copper between the tap
//!   and the pad adds its drop to the few tens of millivolts the controller regulates and its
//!   reverse comparator trips on.
//! - `vcap_path_mm`: VCAP pin → charge-pump capacitor → anode pin, when there is one.

use std::collections::BTreeMap;

use pcb::geom::{self, P};
use pcb::graph::copper_path;
use pcb::Board;
use serde::{Deserialize, Serialize};

use crate::bridge::{bridging, centre, kelvin_tap, legs_loop, measurement, pad, pads, pin_on, routed_note};
use crate::{FixStage, Limit, Measurement};

fn roles_from(m: &BTreeMap<String, String>, known: &[&str], what: &str) -> Result<(), String> {
    match m.keys().find(|k| !known.contains(&k.as_str())) {
        Some(k) => Err(format!("`{k}` is not {what} role")),
        None => Ok(()),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtectionRoles {
    /// The connector pad on the protected net, `REF.PIN`.
    pub conn: String,
    /// The TVS (a part; its pad on the connector's net is the clamp).
    pub tvs: String,
    /// The connector's ground pad; found when not given (its pad on the TVS's ground net nearest
    /// `conn`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ret: Option<String>,
}

impl ProtectionRoles {
    pub fn from_map(m: &BTreeMap<String, String>) -> Result<ProtectionRoles, String> {
        roles_from(m, &["conn", "tvs", "ret"], "a protection")?;
        let get = |k: &str| m.get(k).cloned().ok_or_else(|| format!("protection role `{k}` is not given"));
        Ok(ProtectionRoles { conn: get("conn")?, tvs: get("tvs")?, ret: m.get("ret").cloned() })
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ProtectionGeometry {
    pub ret: String,
    pub tvs_pad: String,
    pub tvs_ground: String,
    /// The first other pad the connector's copper reaches.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first: Option<String>,
    pub to_tvs: Vec<P>,
    pub to_first: Vec<P>,
    pub clamp_loop: Vec<P>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ground_via: Option<P>,
    pub routed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProtectionReport {
    pub block: String,
    pub roles: ProtectionRoles,
    pub nets: [String; 2],
    pub measurements: Vec<Measurement>,
    pub geometry: ProtectionGeometry,
}

/// Layer changes along a path that are vias, not a through-hole pad's own layers.
fn vias_on(board: &Board, path: &pcb::graph::Path) -> usize {
    (0..path.points.len().saturating_sub(1))
        .filter(|&w| path.layers[w] != path.layers[w + 1])
        .filter(|&w| {
            let q = path.points[w];
            !board.pads.iter().any(|p| p.layers.len() > 1 && p.shape.iter().any(|s| geom::inside(s, q)))
        })
        .count()
}

pub fn protection(board: &Board, block: &str, roles: &ProtectionRoles, limits: &BTreeMap<String, Limit>) -> Result<ProtectionReport, String> {
    let conn = pad(board, &roles.conn)?;
    let net = conn.net.clone();
    let conn_part = conn.part.clone();
    let tvs_pad = pin_on(board, &roles.tvs, &net)?;
    let tvs_pads = pads(board, &roles.tvs)?;
    let conn_pads = pads(board, &conn_part)?;
    // Ground: the given return pad's net, or the TVS net the connector also has a pad on. The
    // return pad is the connector's ground pad nearest the TVS's along the copper (nearest the
    // connector pad when none is joined).
    let ret = match &roles.ret {
        Some(r) => r.clone(),
        None => {
            let gnd = tvs_pads
                .iter()
                .map(|p| p.net.as_str())
                .find(|n| *n != net && conn_pads.iter().any(|c| c.net == *n))
                .ok_or_else(|| format!("{} shares no net with {conn_part} but `{net}`; name the connector's ground pad (`ret`)", roles.tvs))?;
            let tvs_g = pin_on(board, &roles.tvs, gnd)?;
            let mut cands: Vec<String> = conn_pads.iter().filter(|c| c.net == gnd).map(|c| format!("{conn_part}.{}", c.pin)).collect();
            cands.dedup();
            let joined = cands.iter().filter_map(|c| copper_path(board, gnd, &tvs_g, c).map(|p| (c, p.length))).min_by(|a, b| a.1.total_cmp(&b.1));
            match joined {
                Some((c, _)) => c.clone(),
                None => cands.into_iter().min_by(|a, b| geom::dist(centre(board, a), conn.centre).total_cmp(&geom::dist(centre(board, b), conn.centre))).unwrap(),
            }
        }
    };
    let gnd = pad(board, &ret)?.net.clone();
    let tvs_ground = pin_on(board, &roles.tvs, &gnd)?;

    let mut out = Vec::new();
    let mut geo = ProtectionGeometry { ret: ret.clone(), tvs_pad: tvs_pad.clone(), tvs_ground: tvs_ground.clone(), routed: true, ..Default::default() };

    let to_tvs = copper_path(board, &net, &roles.conn, &tvs_pad);
    let d_ct = to_tvs.as_ref().map_or_else(|| geom::dist(conn.centre, centre(board, &tvs_pad)), |p| p.length);
    geo.routed &= to_tvs.is_some();
    out.push(measurement(block, "connector_to_tvs_mm", &[&net], conn.centre, d_ct, "mm", FixStage::Floorplan, &routed_note(to_tvs.is_some(), "shortest copper path, connector pad to TVS pad")));
    if let Some(p) = &to_tvs {
        out.push(measurement(block, "vias_before_tvs", &[&net], conn.centre, vias_on(board, p) as f64, "count", FixStage::Route, "layer changes on the connector-to-TVS path, a through-hole pad's own excluded"));
        geo.to_tvs = p.points.clone();
    }

    // The first other pad the connector's copper reaches.
    let mut first: Option<(String, pcb::graph::Path)> = None;
    for p in board.pads.iter().filter(|p| p.net == net && p.part != conn_part && p.part != roles.tvs) {
        let to = format!("{}.{}", p.part, p.pin);
        if let Some(path) = copper_path(board, &net, &roles.conn, &to) {
            if first.as_ref().is_none_or(|f| path.length < f.1.length) {
                first = Some((to, path));
            }
        }
    }
    if let (Some((f, path)), Some(_)) = (&first, &to_tvs) {
        let d_tf = copper_path(board, &net, &tvs_pad, f).map_or(f64::INFINITY, |p| p.length);
        let lead = path.length - d_ct;
        let stub = ((d_ct + d_tf - path.length) / 2.0).max(0.0);
        out.push(measurement(block, "tvs_lead_mm", &[&net], centre(board, f), lead, "mm", FixStage::Floorplan, &format!("copper from the connector to {f}, the first other pad reached, less to the TVS")));
        out.push(measurement(block, "tvs_stub_mm", &[&net], centre(board, &tvs_pad), stub, "mm", FixStage::Route, &format!("how far the TVS pad sits off the connector's path to {f}")));
        geo.to_first = path.points.clone();
        geo.first = Some(f.clone());
    }

    let (lp, routed, _) = legs_loop(board, &[(&net, &roles.conn, &tvs_pad), (&gnd, &tvs_ground, &ret)]);
    geo.routed &= routed;
    out.push(measurement(
        block,
        "clamp_loop_area_mm2",
        &[&net, &gnd],
        geom::centroid(&lp),
        geom::loop_area(&lp, 400),
        "mm2",
        FixStage::Floorplan,
        &routed_note(routed, "copper, connector pad to the TVS, its ground pad to the connector's ground pad"),
    ));
    geo.clamp_loop = lp;

    // Ground vias at the TVS's ground pad.
    let gshapes: Vec<&Vec<P>> = tvs_pads.iter().filter(|p| p.net == gnd).flat_map(|p| p.shape.iter()).collect();
    let via_d = |at: P, r: f64| gshapes.iter().map(|s| if geom::inside(s, at) { 0.0 } else { (geom::point_poly_dist(at, s) - r).max(0.0) }).fold(f64::INFINITY, f64::min);
    let vias: Vec<(f64, P)> = board.vias.iter().filter(|v| v.net == gnd).map(|v| (via_d(v.at, v.size / 2.0), v.at)).collect();
    let nearest = vias.iter().copied().min_by(|a, b| a.0.total_cmp(&b.0));
    let gp = centre(board, &tvs_ground);
    out.push(measurement(block, "tvs_ground_via_mm", &[&gnd], gp, nearest.map_or(f64::INFINITY, |n| n.0), "mm", FixStage::Route, "TVS ground pad edge to the nearest ground via's ring, 0 in the pad"));
    out.push(measurement(block, "tvs_ground_vias", &[&gnd], gp, vias.iter().filter(|v| v.0 <= 1.0).count() as f64, "count", FixStage::Route, "ground vias whose ring is within 1 mm of the TVS ground pad"));
    geo.ground_via = nearest.map(|n| n.1);

    for x in &mut out {
        x.judge(limits.get(x.check).copied());
    }
    Ok(ProtectionReport { block: block.into(), roles: roles.clone(), nets: [net, gnd], measurements: out, geometry: geo })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdealDiodeRoles {
    /// Controller pins, `REF.PIN`.
    pub anode: String,
    pub cathode: String,
    pub gate: String,
    /// The charge-pump pin, when the controller has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vcap: Option<String>,
    /// The MOSFET (a part).
    pub fet: String,
}

impl IdealDiodeRoles {
    pub fn from_map(m: &BTreeMap<String, String>) -> Result<IdealDiodeRoles, String> {
        roles_from(m, &["anode", "cathode", "gate", "vcap", "fet"], "an ideal-diode")?;
        let get = |k: &str| m.get(k).cloned().ok_or_else(|| format!("ideal-diode role `{k}` is not given"));
        Ok(IdealDiodeRoles { anode: get("anode")?, cathode: get("cathode")?, gate: get("gate")?, vcap: m.get("vcap").cloned(), fet: get("fet")? })
    }

    pub fn ctrl(&self) -> &str {
        self.anode.split_once('.').map_or("", |(r, _)| r)
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct IdealDiodeGeometry {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vcap_cap: Option<String>,
    pub gate_path: Vec<P>,
    pub anode_path: Vec<P>,
    pub cathode_path: Vec<P>,
    pub vcap_path: Vec<P>,
    pub taps: Vec<P>,
    pub routed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct IdealDiodeReport {
    pub block: String,
    pub roles: IdealDiodeRoles,
    /// Anode, cathode, gate.
    pub nets: [String; 3],
    pub measurements: Vec<Measurement>,
    pub geometry: IdealDiodeGeometry,
}

pub fn ideal_diode(board: &Board, block: &str, roles: &IdealDiodeRoles, limits: &BTreeMap<String, Limit>) -> Result<IdealDiodeReport, String> {
    let (an, ca, ga) = (pad(board, &roles.anode)?, pad(board, &roles.cathode)?, pad(board, &roles.gate)?);
    let ctrl = roles.ctrl();
    let mut out = Vec::new();
    let mut geo = IdealDiodeGeometry { routed: true, ..Default::default() };

    let fet_gate = pin_on(board, &roles.fet, &ga.net)?;
    let (gp, routed, glen) = legs_loop(board, &[(&ga.net, &roles.gate, &fet_gate)]);
    geo.routed &= routed;
    out.push(measurement(block, "gate_path_mm", &[&ga.net], ga.centre, glen, "mm", FixStage::Block, &routed_note(routed, "copper, GATE pin to the MOSFET's gate")));
    geo.gate_path = gp;

    // Capacitors carry no load current: a sense track that runs past one has not tapped the
    // load copper yet.
    let caps: Vec<&str> = board.parts.values().filter(|p| p.reference.starts_with('C')).map(|p| p.reference.as_str()).collect();
    let mut quiet = vec![ctrl];
    quiet.extend(caps);
    for (side, sense) in [("anode", an), ("cathode", ca)] {
        let to = pin_on(board, &roles.fet, &sense.net)?;
        let from = format!("{}.{}", sense.part, sense.pin);
        let (path_check, tap_check) = if side == "anode" { ("anode_path_mm", "anode_kelvin_offset_mm") } else { ("cathode_path_mm", "cathode_kelvin_offset_mm") };
        match copper_path(board, &sense.net, &from, &to) {
            Some(path) => {
                out.push(measurement(block, path_check, &[&sense.net], sense.centre, path.length, "mm", FixStage::Block, "copper, sense pin to the MOSFET's pad"));
                let (tap, offset) = kelvin_tap(board, &path, &sense.net, &quiet, &[roles.fet.as_str()]);
                out.push(measurement(
                    block,
                    tap_check,
                    &[&sense.net],
                    tap,
                    offset,
                    "mm",
                    FixStage::Route,
                    "where the sense path first enters a fill of its net or a load-carrying pad, to the MOSFET's pad; 0 is a tap on the pad",
                ));
                geo.taps.push(tap);
                if side == "anode" {
                    geo.anode_path = path.points;
                } else {
                    geo.cathode_path = path.points;
                }
            }
            None => {
                geo.routed = false;
                out.push(measurement(block, path_check, &[&sense.net], sense.centre, geom::dist(sense.centre, centre(board, &to)), "mm", FixStage::Block, "pad centres: unrouted"));
            }
        }
    }

    if let Some(vc) = &roles.vcap {
        let vp = pad(board, vc)?;
        let cap = bridging(board, &vp.net, &an.net, "C", "charge-pump capacitor")?;
        let (lp, routed, len) = legs_loop(board, &[(&vp.net, vc, &pin_on(board, &cap, &vp.net)?), (&an.net, &pin_on(board, &cap, &an.net)?, &roles.anode)]);
        geo.routed &= routed;
        out.push(measurement(block, "vcap_path_mm", &[&vp.net, &an.net], vp.centre, len, "mm", FixStage::Block, &routed_note(routed, &format!("copper, VCAP pin to {cap} to the anode pin"))));
        geo.vcap_path = lp;
        geo.vcap_cap = Some(cap);
    }

    for x in &mut out {
        x.judge(limits.get(x.check).copied());
    }
    Ok(IdealDiodeReport { block: block.into(), roles: roles.clone(), nets: [an.net.clone(), ca.net.clone(), ga.net.clone()], measurements: out, geometry: geo })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_board::{board, track};
    use pcb::{Fill, Via};

    fn roles(kv: &[(&str, &str)]) -> BTreeMap<String, String> {
        kv.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    fn measured(ms: &[Measurement], check: &str) -> f64 {
        ms.iter().find(|m| m.check == check).unwrap_or_else(|| panic!("no {check}")).measured
    }

    /// J1.1 runs straight to U1.1, 10 mm away; D1 hangs off the line at x = 5 when `stub`, or
    /// sits on it.
    fn connector(stub: bool) -> Board {
        let tvs_at = if stub { [5.0, 2.0] } else { [5.0, 0.0] };
        let mut tracks = vec![track("N", [0.0, 0.0], [10.0, 0.0]), track("GND", [5.0, 3.0], [0.0, 3.0])];
        if stub {
            tracks.push(track("N", [5.0, 0.0], [5.0, 2.0]));
        }
        let mut b = board(
            &[("J1", "1", "N", [0.0, 0.0]), ("J1", "2", "GND", [0.0, 3.0]), ("U1", "1", "N", [10.0, 0.0]), ("D1", "1", "N", tvs_at), ("D1", "2", "GND", [5.0, 3.0])],
            tracks,
            vec![],
        );
        b.vias.push(Via { at: [5.0, 3.9], size: 0.4, drill: 0.2, layers: vec!["F.Cu".into(), "B.Cu".into()], net: "GND".into() });
        b
    }

    #[test]
    fn a_tvs_on_a_branch_is_a_stub_reached_late() {
        let r = protection(&connector(true), "p", &ProtectionRoles::from_map(&roles(&[("conn", "J1.1"), ("tvs", "D1")])).unwrap(), &BTreeMap::new()).unwrap();
        let m = |c| measured(&r.measurements, c);
        assert_eq!(r.geometry.ret, "J1.2");
        assert_eq!(r.geometry.first.as_deref(), Some("U1.1"));
        assert!((m("connector_to_tvs_mm") - 7.0).abs() < 1e-9);
        assert!((m("tvs_stub_mm") - 2.0).abs() < 1e-9);
        assert!((m("tvs_lead_mm") - 3.0).abs() < 1e-9);
        assert_eq!(m("vias_before_tvs"), 0.0);
        assert!((m("clamp_loop_area_mm2") - 15.0).abs() < 0.3, "{}", m("clamp_loop_area_mm2"));
        assert!((m("tvs_ground_via_mm") - 0.2).abs() < 0.01, "{}", m("tvs_ground_via_mm"));
        assert_eq!(m("tvs_ground_vias"), 1.0);
    }

    #[test]
    fn a_tvs_on_the_line_has_no_stub() {
        let r = protection(&connector(false), "p", &ProtectionRoles::from_map(&roles(&[("conn", "J1.1"), ("tvs", "D1")])).unwrap(), &BTreeMap::new()).unwrap();
        let m = |c| measured(&r.measurements, c);
        assert!(m("tvs_stub_mm").abs() < 1e-9);
        assert!((m("tvs_lead_mm") - 5.0).abs() < 1e-9);
    }

    #[test]
    fn an_ideal_diode_sense_into_the_pour_is_not_kelvin() {
        // The anode track enters a fill of its net 1.5 mm before the MOSFET's pad; the cathode
        // track lands on its pad.
        let fill = Fill { net: "A".into(), layer: "F.Cu".into(), polygon: vec![[3.0, -1.0], [7.0, -1.0], [7.0, 1.0], [3.0, 1.0]] };
        let b = board(
            &[("U1", "6", "A", [0.0, 0.0]), ("U1", "4", "K", [0.0, 2.0]), ("U1", "5", "G", [0.0, 4.0]), ("Q1", "3", "A", [5.0, 0.0]), ("Q1", "2", "K", [5.0, 2.0]), ("Q1", "1", "G", [5.0, 4.0])],
            vec![track("A", [0.0, 0.0], [5.0, 0.0]), track("K", [0.0, 2.0], [5.0, 2.0]), track("G", [0.0, 4.0], [5.0, 4.0])],
            vec![fill],
        );
        let r = ideal_diode(&b, "d", &IdealDiodeRoles::from_map(&roles(&[("anode", "U1.6"), ("cathode", "U1.4"), ("gate", "U1.5"), ("fet", "Q1")])).unwrap(), &BTreeMap::new()).unwrap();
        let m = |c| measured(&r.measurements, c);
        assert!((m("gate_path_mm") - 5.0).abs() < 1e-9);
        assert!((m("anode_kelvin_offset_mm") - 1.5).abs() < 0.03, "{}", m("anode_kelvin_offset_mm"));
        assert_eq!(m("cathode_kelvin_offset_mm"), 0.0);
    }
}
