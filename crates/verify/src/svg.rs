//! A picture of a buck block's copper with its loops and feedback path drawn over it, so a
//! person can see what the numbers measured.

use std::fmt::Write;

use pcb::geom::{self, P};
use pcb::Board;

use crate::buck::BuckReport;

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

/// The block's parts and everything within `margin` mm of them.
pub fn render(board: &Board, report: &BuckReport, title: &str, margin: f64) -> String {
    let r = &report.roles;
    let refs = [r.ic(), r.cin.as_str(), r.cout.as_str(), r.inductor.as_str()];
    let ic = r.ic();
    // Frame the passives and the IC pins they use, not the whole IC.
    let mut pts: Vec<P> = Vec::new();
    for rf in &refs[1..] {
        if let Some(p) = board.parts.get(*rf) {
            pts.extend(p.pads.iter().flat_map(|&i| board.pads[i].shape.iter().flatten().copied()));
        }
    }
    for pin in [&r.vin, &r.sw, &r.pgnd, &r.fb] {
        if let Some(p) = board.pad(pin) {
            pts.extend(p.shape.iter().flatten().copied());
        }
    }
    // And the block nets' vias and tracks near those pads.
    if let Some((a, b)) = geom::bbox(pts.iter().copied()) {
        let near = |q: P| q[0] >= a[0] - 2.0 && q[0] <= b[0] + 2.0 && q[1] >= a[1] - 2.0 && q[1] <= b[1] + 2.0;
        let nets = [&report.nets.vin, &report.nets.sw, &report.nets.gnd, &report.nets.out];
        pts.extend(board.vias.iter().filter(|v| nets.contains(&&v.net) && near(v.at)).flat_map(|v| geom::circle(v.at, v.size / 2.0, 8)));
        pts.extend(board.tracks.iter().filter(|t| nets.contains(&&t.net) && near(t.a) && near(t.b)).flat_map(|t| [t.a, t.b]));
    }
    let (lo, hi) = geom::bbox(pts).unwrap_or(([0.0, 0.0], [10.0, 10.0]));
    let (lo, hi) = ([lo[0] - margin, lo[1] - margin], [hi[0] + margin, hi[1] + margin]);
    let (w, h) = (hi[0] - lo[0], hi[1] - lo[1]);
    let visible = |poly: &[P]| geom::bbox(poly.iter().copied()).is_some_and(|(a, b)| b[0] >= lo[0] && a[0] <= hi[0] && b[1] >= lo[1] && a[1] <= hi[1]);
    let block_nets = [&report.nets.vin, &report.nets.sw, &report.nets.gnd, &report.nets.out];

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
            let op = if block_nets.contains(&&f.net) { 0.45 } else { 0.18 };
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
            let col = if p.net == report.nets.sw { "#ff9a1f" } else { layer_colour(p.layers.first().map_or("F.Cu", |l| l.as_str())) };
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
                if rf != ic {
                    let _ = writeln!(s, r##"<text x="{:.3}" y="{:.3}" font-size="0.35" fill="#e6e9ee" text-anchor="middle">{}</text>"##, part.at[0], part.at[1] + 0.12, xml(rf));
                }
            }
        }
    }

    let g = &report.geometry;
    let _ = writeln!(s, r##"<path d="{}" fill="#ff3df2" fill-opacity="0.22" stroke="#ff3df2" stroke-width="0.06"/>"##, path_d(&g.hot_loop, true));
    let _ = writeln!(s, r##"<path d="{}" fill="none" stroke="#3de0ff" stroke-width="0.05" stroke-dasharray="0.15 0.1"/>"##, path_d(&g.output_loop, true));
    let _ = writeln!(s, r##"<path d="{}" fill="none" stroke="#fff35c" stroke-width="0.07"/>"##, path_d(&g.feedback_path, false));
    if let Some((a, b)) = g.feedback_closest {
        let _ = writeln!(s, r##"<line x1="{:.3}" y1="{:.3}" x2="{:.3}" y2="{:.3}" stroke="#fff35c" stroke-width="0.03" stroke-dasharray="0.05 0.05"/>"##, a[0], a[1], b[0], b[1]);
    }
    s.push_str("</svg>\n");
    s
}

fn xml(t: &str) -> String {
    t.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}
