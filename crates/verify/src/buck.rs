//! Checks for a buck converter block, named by the pads and parts that play each role.
//!
//! Loop areas are the integral of |winding number| (`geom::loop_area`), so a loop that crosses
//! itself is not credited with cancelling lobes.
//!
//! - `hot_loop_area_mm2`: the input loop, VIN pin → CIN → PGND pin, the loop whose current
//!   switches on every edge. Routed: the copper paths on the VIN and GND nets closed through
//!   the capacitor and across the IC. Placed (`hot_loop_area_placed_mm2`): through pad centres.
//! - `output_loop_area_mm2`: SW pin → inductor → COUT → PGND pin, the same way.
//! - `switch_node_area_mm2`: all copper of the switch net, every layer, overlaps counted once.
//! - `feedback_clearance_mm`: the feedback path's centreline (FB pin to COUT, or to the divider
//!   when FB sits behind one) to switch-node copper on any layer, the IC's own pins excluded
//!   (the package fixes those).
//!
//! An asynchronous buck names its freewheeling `diode`; its loops close through the diode
//! instead of the IC's ground pin.
//! - `switch_node_underlap_mm2`: other nets' copper on the layer under the switch node's outer
//!   copper (datasheets ask for it cut away).

use std::collections::BTreeMap;

use pcb::geom::{self, P};
use pcb::graph::copper_path;
use pcb::{Board, Pad};
use serde::{Deserialize, Serialize};

use crate::{FixStage, Limit, Measurement};

/// Who plays each part of a buck: pads as `REF.PIN`, parts by reference.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuckRoles {
    pub vin: String,
    pub sw: String,
    pub pgnd: String,
    pub fb: String,
    pub cin: String,
    pub cout: String,
    pub inductor: String,
    /// The freewheeling diode of an asynchronous buck (a part). Absent for a synchronous buck,
    /// whose low-side switch is inside the IC and returns through `pgnd`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diode: Option<String>,
}

impl BuckRoles {
    /// From `vin=U1.49,sw=U1.48,...` or a role map.
    pub fn from_map(m: &BTreeMap<String, String>) -> Result<BuckRoles, String> {
        let get = |k: &str| m.get(k).cloned().ok_or_else(|| format!("buck role `{k}` is not given"));
        Ok(BuckRoles {
            vin: get("vin")?,
            sw: get("sw")?,
            pgnd: get("pgnd")?,
            fb: get("fb")?,
            cin: get("cin")?,
            cout: get("cout")?,
            inductor: get("inductor")?,
            diode: m.get("diode").cloned(),
        })
    }

    pub fn ic(&self) -> &str {
        self.vin.split_once('.').map_or("", |(r, _)| r)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct BuckNets {
    pub vin: String,
    pub sw: String,
    pub gnd: String,
    pub out: String,
}

/// The shapes behind the numbers, for drawing.
#[derive(Debug, Clone, Default, Serialize)]
pub struct BuckGeometry {
    pub hot_loop: Vec<P>,
    pub output_loop: Vec<P>,
    pub feedback_path: Vec<P>,
    /// Where the feedback path comes closest to the switch node: (on path, on copper).
    pub feedback_closest: Option<(P, P)>,
    pub routed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct BuckReport {
    pub block: String,
    pub roles: BuckRoles,
    pub nets: BuckNets,
    pub measurements: Vec<Measurement>,
    pub geometry: BuckGeometry,
}

fn pad<'a>(b: &'a Board, ref_pin: &str) -> Result<&'a Pad, String> {
    b.pad(ref_pin).ok_or_else(|| format!("pad {ref_pin} is not on the board"))
}

/// The pad of a two-terminal part on `net`.
fn terminal<'a>(b: &'a Board, part: &str, net: &str) -> Result<&'a Pad, String> {
    let p = b.parts.get(part).ok_or_else(|| format!("part {part} is not on the board"))?;
    p.pads.iter().map(|&i| &b.pads[i]).find(|p| p.net == net).ok_or_else(|| format!("{part} has no pad on net `{net}`"))
}

fn name(p: &Pad) -> String {
    format!("{}.{}", p.part, p.pin)
}

/// A loop: copper path `a` (first leg), then the second leg `b`, closed straight back.
fn loop_polygon(b: &Board, legs: &[(&str, &Pad, &Pad)]) -> (Vec<P>, bool) {
    let mut pts = Vec::new();
    let mut routed = true;
    for (net, from, to) in legs {
        match copper_path(b, net, &name(from), &name(to)) {
            Some(p) => pts.extend(p.points),
            None => {
                routed = false;
                pts.extend([from.centre, to.centre]);
            }
        }
    }
    (pts, routed)
}

pub fn check(board: &Board, block: &str, roles: &BuckRoles, limits: &BTreeMap<String, Limit>) -> Result<BuckReport, String> {
    let (vin, sw, pgnd, fb) = (pad(board, &roles.vin)?, pad(board, &roles.sw)?, pad(board, &roles.pgnd)?, pad(board, &roles.fb)?);
    // The output is the inductor's other side; the feedback pin is on it (an on-chip divider) or
    // behind an external divider.
    let l_sw = terminal(board, &roles.inductor, &sw.net)?;
    let l_out = board.parts[&roles.inductor].pads.iter().map(|&i| &board.pads[i]).find(|p| p.net != sw.net).ok_or("inductor has no output pad")?;
    let nets = BuckNets { vin: vin.net.clone(), sw: sw.net.clone(), gnd: pgnd.net.clone(), out: l_out.net.clone() };
    let (cin_v, cin_g) = (terminal(board, &roles.cin, &nets.vin)?, terminal(board, &roles.cin, &nets.gnd)?);
    let (cout_o, cout_g) = (terminal(board, &roles.cout, &nets.out)?, terminal(board, &roles.cout, &nets.gnd)?);
    let diode = match &roles.diode {
        Some(d) => Some((terminal(board, d, &nets.gnd)?, terminal(board, d, &nets.sw)?)),
        None => None,
    };
    // Where the feedback path ends: COUT when FB is on the output, else the nearest other pad on
    // the feedback net (the divider).
    let fb_target: &Pad = if fb.net == nets.out {
        cout_o
    } else {
        board
            .pads
            .iter()
            .filter(|p| p.net == fb.net && p.part != fb.part)
            .min_by(|a, b| geom::dist(a.centre, fb.centre).total_cmp(&geom::dist(b.centre, fb.centre)))
            .ok_or("the feedback net reaches no other pad")?
    };

    let mut out = Vec::new();
    let mut geo = BuckGeometry::default();
    let m = |check: &'static str, nets: &[&String], location: P, measured: f64, unit: &'static str, fix: FixStage, method: Option<&str>| Measurement {
        check,
        block: block.to_string(),
        nets: nets.iter().map(|s| s.to_string()).collect(),
        location,
        measured,
        unit,
        limit: None,
        passed: None,
        fix_stage: fix,
        method: method.map(str::to_string),
    };

    // Hot loop: through the IC's ground pin, or for an asynchronous buck through the diode.
    let (hot_legs, placed): (Vec<(&str, &Pad, &Pad)>, Vec<P>) = match diode {
        None => (vec![(&nets.vin, vin, cin_v), (&nets.gnd, cin_g, pgnd)], vec![vin.centre, cin_v.centre, cin_g.centre, pgnd.centre]),
        Some((da, dk)) => (
            vec![(&nets.vin, vin, cin_v), (&nets.gnd, cin_g, da), (&nets.sw, dk, sw)],
            vec![vin.centre, cin_v.centre, cin_g.centre, da.centre, dk.centre, sw.centre],
        ),
    };
    let (hot, routed) = loop_polygon(board, &hot_legs);
    let method = match (routed, diode.is_some()) {
        (true, false) => "copper paths on VIN and GND, closed through CIN and the IC",
        (true, true) => "copper paths on VIN, GND and SW, closed through CIN, the diode and the IC",
        (false, _) => "pad centres: a leg is unrouted",
    };
    out.push(m("hot_loop_area_mm2", &[&nets.vin, &nets.gnd], geom::centroid(&hot), geom::loop_area(&hot, 400), "mm2", FixStage::Block, Some(method)));
    out.push(m("hot_loop_area_placed_mm2", &[&nets.vin, &nets.gnd], geom::centroid(&placed), geom::loop_area(&placed, 400), "mm2", FixStage::Block, Some("pad centres")));
    geo.hot_loop = hot;
    geo.routed = routed;

    // Output loop: switch → inductor → COUT → ground, back through the IC's ground pin or the diode.
    let out_legs: Vec<(&str, &Pad, &Pad)> = match diode {
        None => vec![(&nets.sw, sw, l_sw), (&nets.out, l_out, cout_o), (&nets.gnd, cout_g, pgnd)],
        Some((da, dk)) => vec![(&nets.sw, dk, l_sw), (&nets.out, l_out, cout_o), (&nets.gnd, cout_g, da)],
    };
    let (outl, routed_o) = loop_polygon(board, &out_legs);
    let method = if routed_o { "copper paths on SW, output and GND" } else { "pad centres: a leg is unrouted" };
    out.push(m("output_loop_area_mm2", &[&nets.sw, &nets.out, &nets.gnd], geom::centroid(&outl), geom::loop_area(&outl, 400), "mm2", FixStage::Block, Some(method)));
    geo.output_loop = outl;

    // Switch node area.
    let ic = roles.ic();
    let sw_copper = board.net_copper(&nets.sw);
    let mut by_layer: BTreeMap<&str, Vec<Vec<P>>> = BTreeMap::new();
    for (l, poly) in &sw_copper {
        by_layer.entry(l.as_str()).or_default().push(poly.clone());
    }
    let sw_area: f64 = by_layer.values().map(|ps| geom::union_area(ps, 0.01)).sum();
    out.push(m("switch_node_area_mm2", &[&nets.sw], sw.centre, sw_area, "mm2", FixStage::Block, Some("union of the net's copper per layer, summed, 10 um raster")));

    // Feedback clearance: in plan view, layer by layer, except where a fill on a layer between
    // the two (a plane of another net) covers the crossing and shields it.
    let (fb_path, fb_layers) = match copper_path(board, &fb.net, &roles.fb, &name(fb_target)) {
        Some(p) => (p.points, p.layers),
        None => (vec![fb.centre, fb_target.centre], vec![fb.layers.first().cloned().unwrap_or_default(); 2]),
    };
    let mut sw_shapes: Vec<(String, Vec<P>)> = Vec::new();
    for p in board.pads.iter().filter(|p| p.net == nets.sw && p.part != ic) {
        for l in &p.layers {
            sw_shapes.extend(p.shape.iter().map(|sh| (l.clone(), sh.clone())));
        }
    }
    sw_shapes.extend(board.tracks.iter().filter(|t| t.net == nets.sw).map(|t| (t.layer.clone(), geom::capsule(t.a, t.b, t.width))));
    for v in board.vias.iter().filter(|v| v.net == nets.sw) {
        sw_shapes.extend(v.layers.iter().map(|l| (l.clone(), geom::circle(v.at, v.size / 2.0, 16))));
    }
    sw_shapes.extend(board.fills.iter().filter(|f| f.net == nets.sw).map(|f| (f.layer.clone(), f.polygon.clone())));
    let li = |l: &str| board.layers.iter().position(|x| x == l);
    let shielded = |p: P, la: &str, lb: &str| -> bool {
        let (Some(i), Some(j)) = (li(la), li(lb)) else { return false };
        let (lo, hi) = (i.min(j), i.max(j));
        board.fills.iter().any(|f| {
            f.net != nets.sw && f.net != fb.net && li(&f.layer).is_some_and(|k| k > lo && k < hi) && geom::inside(&f.polygon, p)
        })
    };
    let mut best = (f64::INFINITY, None);
    let mut shielded_crossings = 0usize;
    for w in 0..fb_path.len().saturating_sub(1) {
        let (a, b, layer) = (fb_path[w], fb_path[w + 1], &fb_layers[w]);
        if fb_layers[w + 1] != *layer {
            continue; // a via: its barrel is measured as the segments either side
        }
        for (sl, sh) in &sw_shapes {
            let d = geom::polyline_poly_dist(&[a, b], sh);
            if d >= best.0 {
                continue;
            }
            let c = closest(&[a, b], sh);
            if sl != layer && shielded(geom::scale(geom::add(c.0, c.1), 0.5), layer, sl) {
                shielded_crossings += 1;
                continue;
            }
            best = (d, Some(c));
        }
    }
    let method = format!(
        "feedback path centreline to switch-node copper, plan view, IC pins excluded; {shielded_crossings} closer approach(es) on other layers shielded by a plane between"
    );
    out.push(m("feedback_clearance_mm", &[&fb.net, &nets.sw], best.1.map_or(fb.centre, |c: (P, P)| c.0), best.0, "mm", FixStage::Block, Some(&method)));
    geo.feedback_path = fb_path;
    geo.feedback_closest = best.1;

    // Copper under the switch node.
    let outer = &sw.layers.first().cloned().unwrap_or_else(|| "F.Cu".into());
    if let Some(i) = board.layers.iter().position(|l| l == outer) {
        let below = if i == 0 { board.layers.get(1) } else { board.layers.get(i - 1) };
        if let Some(below) = below {
            let top: Vec<Vec<P>> = sw_copper.iter().filter(|(l, _)| l == outer).map(|(_, p)| p.clone()).collect();
            let others: Vec<Vec<P>> = board
                .fills
                .iter()
                .filter(|f| &f.layer == below && f.net != nets.sw)
                .map(|f| f.polygon.clone())
                .chain(board.tracks.iter().filter(|t| &t.layer == below && t.net != nets.sw).map(|t| geom::capsule(t.a, t.b, t.width)))
                .collect();
            let a = geom::overlap_area(&top, Some(&others), 0.02);
            let checkname = "switch_node_underlap_mm2";
            out.push(m(checkname, &[&nets.sw], sw.centre, a, "mm2", FixStage::Block, Some(&format!("{outer} switch copper over other nets' copper on {below}, 20 um raster"))));
        }
    }

    for x in &mut out {
        x.judge(limits.get(x.check).copied());
    }
    Ok(BuckReport { block: block.into(), roles: roles.clone(), nets, measurements: out, geometry: geo })
}

/// The closest pair of points between a polyline and a polygon's edges.
fn closest(line: &[P], poly: &[P]) -> (P, P) {
    let mut best = (f64::INFINITY, line[0], poly[0]);
    let n = poly.len();
    for w in line.windows(2) {
        for i in 0..n {
            let (c, d) = (poly[i], poly[(i + 1) % n]);
            for (p, a, b, flip) in [(w[0], c, d, false), (w[1], c, d, false), (c, w[0], w[1], true), (d, w[0], w[1], true)] {
                let q = project(p, a, b);
                let dd = geom::dist(p, q);
                if dd < best.0 {
                    best = if flip { (dd, q, p) } else { (dd, p, q) };
                }
            }
        }
    }
    (best.1, best.2)
}

fn project(p: P, a: P, b: P) -> P {
    let ab = geom::sub(b, a);
    let l2 = geom::dot(ab, ab);
    let t = if l2 == 0.0 { 0.0 } else { (geom::dot(geom::sub(p, a), ab) / l2).clamp(0.0, 1.0) };
    geom::add(a, geom::scale(ab, t))
}
