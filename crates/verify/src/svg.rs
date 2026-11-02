//! A picture of a block's copper with its loops and paths drawn over it, so a person can see
//! what the numbers measured.

use std::fmt::Write;

use pcb::geom::{self, P};
use pcb::Board;

use crate::bridge::{BridgeReport, SenseReport};
use crate::buck::BuckReport;
use crate::protect::{IdealDiodeReport, ProtectionReport};

fn layer_colour(layer: &str) -> &'static str {
    match layer {
        "F.Cu" => "#c83434",
        "B.Cu" => "#3465c8",
        "In1.Cu" => "#3a9a4a",
        "In2.Cu" => "#c8a234",
        _ => "#888888",
    }
}

fn path_d(poly: &[P], close: bool) -> String {
    let mut d = String::new();
    for (i, p) in poly.iter().enumerate() {
        let _ = write!(d, "{}{:.3} {:.3} ", if i == 0 { "M" } else { "L" }, p[0], p[1]);
    }
    if close {
        d.push('Z');
    }
    d
}

/// A shape drawn over the copper.
pub struct Overlay<'a> {
    pub points: &'a [P],
    pub colour: &'static str,
    pub closed: bool,
    /// Filled faintly (a loop whose area is the number).
    pub filled: bool,
    /// An SVG dash pattern.
    pub dash: Option<&'static str>,
    pub width: f64,
}

/// A buck block's parts and everything within `margin` mm of them.
pub fn render(board: &Board, report: &BuckReport, title: &str, margin: f64) -> String {
    let r = &report.roles;
    let ic = r.ic();
    // Frame the passives and the IC pins they use, not the whole IC.
    let mut pts = part_points(board, &[r.cin.as_str(), r.cout.as_str(), r.inductor.as_str()]);
    pts.extend(pin_points(board, &[&r.vin, &r.sw, &r.pgnd, &r.fb]));
    let nets = [report.nets.vin.as_str(), &report.nets.sw, &report.nets.gnd, &report.nets.out];
    let g = &report.geometry;
    let mut overlays = vec![
        Overlay { points: &g.hot_loop, colour: "#ff3df2", closed: true, filled: true, dash: None, width: 0.06 },
        Overlay { points: &g.output_loop, colour: "#3de0ff", closed: true, filled: false, dash: Some("0.15 0.1"), width: 0.05 },
        Overlay { points: &g.feedback_path, colour: "#fff35c", closed: false, filled: false, dash: None, width: 0.07 },
    ];
    let closest = g.feedback_closest.map(|(a, b)| [a, b]);
    if let Some(c) = &closest {
        overlays.push(Overlay { points: c, colour: "#fff35c", closed: false, filled: false, dash: Some("0.05 0.05"), width: 0.03 });
    }
    draw(board, title, pts, &nets, &report.nets.sw, &[ic], &overlays, margin)
}

/// A half-bridge: the switches, bus capacitor, gate resistors and bootstrap capacitor, with the
/// driver's pins; the commutation loop filled, the gate and bootstrap loops outlined.
pub fn render_bridge(board: &Board, report: &BridgeReport, title: &str, margin: f64) -> String {
    let (r, g) = (&report.roles, &report.geometry);
    let mut pts = part_points(board, &[r.hs.as_str(), &r.ls, &r.cbus, &g.rgh, &g.rgl, &g.cboot]);
    pts.extend(pin_points(board, &[&r.ho, &r.lo, &r.vs, &r.vb, &r.com, &r.vcc]));
    let n = &report.nets;
    let nets = [n.bus.as_str(), &n.phase, &n.hs_gate, &n.ls_gate];
    let driver = r.vcc.split_once('.').map_or("", |(d, _)| d);
    let overlays = [
        Overlay { points: &g.hot_loop, colour: "#ff3df2", closed: true, filled: true, dash: None, width: 0.08 },
        Overlay { points: &g.hs_gate_loop, colour: "#fff35c", closed: true, filled: false, dash: None, width: 0.06 },
        Overlay { points: &g.ls_gate_loop, colour: "#3de0ff", closed: true, filled: false, dash: None, width: 0.06 },
        Overlay { points: &g.bootstrap_loop, colour: "#7dff6a", closed: true, filled: false, dash: Some("0.15 0.1"), width: 0.06 },
    ];
    draw(board, title, pts, &nets, &n.phase, &[driver], &overlays, margin)
}

/// A current-sense amplifier's inputs and its shunts: the sense loop filled, each input's tap
/// ringed.
pub fn render_sense(board: &Board, report: &SenseReport, title: &str, margin: f64) -> String {
    let (r, g) = (&report.roles, &report.geometry);
    let shunts: Vec<&str> = r.shunt.iter().map(String::as_str).collect();
    let mut pts = part_points(board, &shunts);
    pts.extend(pin_points(board, &[&r.inp, &r.inn]));
    let nets = [report.nets[0].as_str(), &report.nets[1]];
    let rings: Vec<Vec<P>> = g.taps.iter().map(|t| geom::circle(*t, 0.35, 24)).collect();
    let mut overlays = vec![Overlay { points: &g.loop_, colour: "#ff3df2", closed: true, filled: true, dash: None, width: 0.06 }];
    overlays.extend(rings.iter().map(|c| Overlay { points: c, colour: "#fff35c", closed: true, filled: false, dash: None, width: 0.08 }));
    draw(board, title, pts, &nets, "", &[r.amp()], &overlays, margin)
}

/// Protection at a connector: the clamp loop filled, the path to the TVS and to the first
/// other pad, the nearest ground via ringed.
pub fn render_protection(board: &Board, report: &ProtectionReport, title: &str, margin: f64) -> String {
    let (r, g) = (&report.roles, &report.geometry);
    let mut pts = part_points(board, &[r.tvs.as_str()]);
    pts.extend(pin_points(board, &[&r.conn, &g.ret]));
    if let Some(f) = &g.first {
        pts.extend(pin_points(board, &[f]));
    }
    let nets = [report.nets[0].as_str()];
    let ring: Vec<P> = g.ground_via.map(|v| geom::circle(v, 0.5, 24)).unwrap_or_default();
    let overlays = [
        Overlay { points: &g.clamp_loop, colour: "#ff3df2", closed: true, filled: true, dash: None, width: 0.08 },
        Overlay { points: &g.to_tvs, colour: "#fff35c", closed: false, filled: false, dash: None, width: 0.1 },
        Overlay { points: &g.to_first, colour: "#3de0ff", closed: false, filled: false, dash: Some("0.3 0.2"), width: 0.08 },
        Overlay { points: &ring, colour: "#7dff6a", closed: true, filled: false, dash: None, width: 0.08 },
    ];
    draw(board, title, pts, &nets, &report.nets[0], &[], &overlays, margin)
}

/// An ideal-diode controller and its MOSFET: the gate path, both sense paths with their taps
/// ringed, the charge-pump loop.
pub fn render_ideal_diode(board: &Board, report: &IdealDiodeReport, title: &str, margin: f64) -> String {
    let (r, g) = (&report.roles, &report.geometry);
    let ctrl = r.ctrl();
    let mut parts = vec![ctrl, r.fet.as_str()];
    if let Some(c) = &g.vcap_cap {
        parts.push(c);
    }
    let pts = part_points(board, &parts);
    let nets = [report.nets[0].as_str(), &report.nets[1], &report.nets[2]];
    let rings: Vec<Vec<P>> = g.taps.iter().map(|t| geom::circle(*t, 0.3, 24)).collect();
    let mut overlays = vec![
        Overlay { points: &g.gate_path, colour: "#fff35c", closed: false, filled: false, dash: None, width: 0.08 },
        Overlay { points: &g.anode_path, colour: "#ff3df2", closed: false, filled: false, dash: None, width: 0.08 },
        Overlay { points: &g.cathode_path, colour: "#3de0ff", closed: false, filled: false, dash: None, width: 0.08 },
        Overlay { points: &g.vcap_path, colour: "#7dff6a", closed: true, filled: false, dash: Some("0.15 0.1"), width: 0.06 },
    ];
    overlays.extend(rings.iter().map(|c| Overlay { points: c, colour: "#ffffff", closed: true, filled: false, dash: None, width: 0.06 }));
    draw(board, title, pts, &nets, "", &[], &overlays, margin)
}

/// The pad outlines of these parts.
pub fn part_points(board: &Board, parts: &[&str]) -> Vec<P> {
    parts.iter().filter_map(|rf| board.parts.get(*rf)).flat_map(|p| p.pads.iter().flat_map(|&i| board.pads[i].shape.iter().flatten().copied())).collect()
}

/// The pad outlines of these pins (`REF.PIN`).
pub fn pin_points(board: &Board, pins: &[&str]) -> Vec<P> {
    pins.iter().filter_map(|pin| board.pad(pin)).flat_map(|p| p.shape.iter().flatten().copied()).collect()
}

/// The board's copper around `focus` (points to frame), with `nets` drawn stronger, `accent`'s
/// pads in orange, and the overlays on top. `unlabelled` parts get no reference.
#[allow(clippy::too_many_arguments)]
pub fn draw(board: &Board, title: &str, mut pts: Vec<P>, nets: &[&str], accent: &str, unlabelled: &[&str], overlays: &[Overlay], margin: f64) -> String {
    // And the block nets' vias and tracks near those pads.
    if let Some((a, b)) = geom::bbox(pts.iter().copied()) {
        let near = |q: P| q[0] >= a[0] - 2.0 && q[0] <= b[0] + 2.0 && q[1] >= a[1] - 2.0 && q[1] <= b[1] + 2.0;
        pts.extend(board.vias.iter().filter(|v| nets.contains(&v.net.as_str()) && near(v.at)).flat_map(|v| geom::circle(v.at, v.size / 2.0, 8)));
        pts.extend(board.tracks.iter().filter(|t| nets.contains(&t.net.as_str()) && near(t.a) && near(t.b)).flat_map(|t| [t.a, t.b]));
    }
    let (lo, hi) = geom::bbox(pts).unwrap_or(([0.0, 0.0], [10.0, 10.0]));
    let (lo, hi) = ([lo[0] - margin, lo[1] - margin], [hi[0] + margin, hi[1] + margin]);
    let (w, h) = (hi[0] - lo[0], hi[1] - lo[1]);
    let visible = |poly: &[P]| geom::bbox(poly.iter().copied()).is_some_and(|(a, b)| b[0] >= lo[0] && a[0] <= hi[0] && b[1] >= lo[1] && a[1] <= hi[1]);

    let mut s = String::new();
    let _ = write!(
        s,
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="{:.3} {:.3} {:.3} {:.3}" width="{:.0}" height="{:.0}" font-family="system-ui, sans-serif">
<rect x="{:.3}" y="{:.3}" width="{:.3}" height="{:.3}" fill="#101318"/>
"##,
        lo[0], lo[1], w, h, w * 100.0, h * 100.0, lo[0], lo[1], w, h
    );
    let _ = writeln!(s, "<title>{}</title>", xml(title));

    // Back layers first, so the top layer reads on top.
    for layer in board.layers.iter().rev() {
        let col = layer_colour(layer);
        for f in board.fills.iter().filter(|f| &f.layer == layer && visible(&f.polygon)) {
            let op = if nets.contains(&f.net.as_str()) { 0.45 } else { 0.18 };
            let _ = writeln!(s, r#"<path d="{}" fill="{col}" fill-opacity="{op}" fill-rule="evenodd"/>"#, path_d(&f.polygon, true));
        }
        for t in board.tracks.iter().filter(|t| &t.layer == layer) {
            let cap = geom::capsule(t.a, t.b, t.width);
            if visible(&cap) {
                let _ = writeln!(s, r#"<path d="{}" fill="{col}" fill-opacity="0.75"/>"#, path_d(&cap, true));
            }
        }
    }
    for p in board.pads.iter() {
        for shape in p.shape.iter().filter(|sh| visible(sh)) {
            let col = if p.net == accent { "#ff9a1f" } else { layer_colour(p.layers.first().map_or("F.Cu", |l| l.as_str())) };
            let _ = writeln!(s, r##"<path d="{}" fill="{col}" fill-opacity="0.9" stroke="#000" stroke-width="0.02"/>"##, path_d(shape, true));
        }
    }
    for v in board.vias.iter().filter(|v| v.at[0] >= lo[0] && v.at[0] <= hi[0] && v.at[1] >= lo[1] && v.at[1] <= hi[1]) {
        let _ = writeln!(
            s,
            r##"<circle cx="{:.3}" cy="{:.3}" r="{:.3}" fill="#d8d8d8"/><circle cx="{:.3}" cy="{:.3}" r="{:.3}" fill="#101318"/>"##,
            v.at[0], v.at[1], v.size / 2.0, v.at[0], v.at[1], v.drill / 2.0
        );
    }
    for (rf, part) in &board.parts {
        if let Some(c) = &part.courtyard {
            if visible(c) {
                let _ = writeln!(s, r##"<path d="{}" fill="none" stroke="#9aa3b0" stroke-width="0.03" stroke-dasharray="0.1 0.08"/>"##, path_d(c, true));
                if !unlabelled.contains(&rf.as_str()) {
                    let _ = writeln!(s, r##"<text x="{:.3}" y="{:.3}" font-size="0.35" fill="#e6e9ee" text-anchor="middle">{}</text>"##, part.at[0], part.at[1] + 0.12, xml(rf));
                }
            }
        }
    }

    for o in overlays {
        let fill = if o.filled { format!(r#"fill="{}" fill-opacity="0.22""#, o.colour) } else { r#"fill="none""#.to_string() };
        let dash = o.dash.map_or(String::new(), |d| format!(r#" stroke-dasharray="{d}""#));
        let _ = writeln!(s, r#"<path d="{}" {fill} stroke="{}" stroke-width="{}"{dash}/>"#, path_d(o.points, o.closed), o.colour, o.width);
    }
    s.push_str("</svg>\n");
    s
}

fn xml(t: &str) -> String {
    t.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}
