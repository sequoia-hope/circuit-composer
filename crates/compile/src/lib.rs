//! The constraint compiler: an intent file and its board, compiled to what the router and KiCad
//! read.
//!
//! - `routing.json` for ultraroute's `--regions`: region polygons in board millimetres with
//!   per-layer `must`/`prefer`/`avoid`/`never` rules on nets, and per-net length limits.
//! - KiCad custom rules (`.kicad_dru`): the length limits, so KiCad's DRC checks the same numbers.
//!
//! Class selectors are expanded to explicit net lists from the intent file, so the output never
//! depends on the KiCad project having a net class of the same name.

use std::collections::BTreeMap;

use intent::{BoundOp, IntentFile, Metric, RegionKind, RuleMode, Selector, Strength};
use pcb::geom::{self, P};
use pcb::Board;
use serde::{Deserialize, Serialize};

/// Intent weights are relative, in (0, 1]; the router's are extra-length multipliers.
pub const WEIGHT_SCALE: f64 = 10.0;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Routing {
    pub version: u32,
    #[serde(default)]
    pub generator: String,
    /// How intent weights became the router's: `router = 10 × intent`.
    #[serde(default)]
    pub weight_mapping: String,
    pub regions: Vec<RRegion>,
    #[serde(default)]
    pub max_lengths: Vec<MaxLength>,
    /// Nets no other net's via or hole may open a plane antipad under (ultraroute f179fcb).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub clear_under: Vec<ClearUnder>,
}

/// Keep other nets' antipads (a via's or hole's clearance in a plane) from under these nets'
/// tracks: on `layers`, the planes on `planes`, by `margin_mm`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClearUnder {
    pub nets: Vec<String>,
    pub layers: Vec<String>,
    pub planes: Vec<String>,
    pub margin_mm: f64,
}

/// `clear_under`'s margin, mm: ultraroute measures 4 to 8 µm at 0, and the checker counts any gap.
pub const CLEAR_UNDER_MARGIN: f64 = 0.05;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RRegion {
    pub name: String,
    pub polygon: Vec<P>,
    /// Absent: every copper layer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layers: Option<Vec<String>>,
    pub rules: Vec<RRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RRule {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nets: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub classes: Option<Vec<String>>,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weight: Option<f64>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub vias: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaxLength {
    pub nets: Vec<String>,
    pub max_length_mm: f64,
    /// The citation, for a person reading the file.
    #[serde(default)]
    pub why: String,
}

/// What the compiler left out, and why.
#[derive(Debug, Clone, Serialize)]
pub struct Note {
    pub subject: String,
    pub message: String,
}

pub fn compile(intent: &IntentFile, board: &Board) -> Result<(Routing, Vec<Note>), String> {
    let mut notes = Vec::new();
    let mut by_class: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for (name, n) in &intent.nets {
        if let Some(c) = &n.class {
            by_class.entry(c.as_str()).or_default().push(name.clone());
        }
    }

    let mut regions = Vec::new();
    for r in &intent.regions {
        let Some(poly) = &r.polygon else {
            notes.push(Note { subject: format!("region {}", r.name), message: "no polygon (a floorplanner region): placed in milestone 5, skipped".into() });
            continue;
        };
        let polygon: Vec<P> = match &r.anchor {
            None => poly.clone(),
            Some(a) => {
                let part = board.parts.get(a).ok_or_else(|| format!("region {}: anchor {a} is not on the board", r.name))?;
                if part.back {
                    return Err(format!("region {}: anchor {a} is on the back; mirrored anchors are not handled yet", r.name));
                }
                poly.iter().map(|q| geom::add(part.at, geom::rotate(*q, part.rotation))).collect()
            }
        };
        if matches!(r.kind, RegionKind::ProtectedZone | RegionKind::IsolationBarrier) {
            notes.push(Note { subject: format!("region {}", r.name), message: "protected zones and isolation barriers go to the router as their rules only; their clearances are milestone 7".into() });
        }
        let mut rules = Vec::new();
        for rule in &r.rules {
            let (nets, classes) = match &rule.applies_to {
                Selector::Net(n) => (Some(vec![n.clone()]), None),
                Selector::AllNets => (None, Some(vec!["*".to_string()])),
                Selector::Class(c) => match by_class.get(c.as_str()) {
                    Some(ns) => (Some(ns.clone()), None),
                    None => {
                        notes.push(Note { subject: format!("region {}", r.name), message: format!("class {c} has no nets on this board; rule dropped") });
                        continue;
                    }
                },
            };
            let kind = match rule.mode {
                RuleMode::Must => "must",
                RuleMode::Prefer => "prefer",
                RuleMode::Avoid => "avoid",
                RuleMode::Never => "never",
            };
            rules.push(RRule { nets, classes, kind: kind.into(), weight: rule.weight.map(|w| w * WEIGHT_SCALE), vias: rule.vias });
        }
        let layers = (!r.layers.is_empty()).then(|| r.layers.clone());
        if let Some(ls) = &layers {
            for l in ls {
                if !board.layers.contains(l) {
                    return Err(format!("region {}: layer {l} is not on the board", r.name));
                }
            }
        }
        regions.push(RRegion { name: r.name.clone(), polygon, layers, rules });
    }

    let (block, clear_under) = block_regions(intent, board, &mut notes);
    regions.extend(block);

    let mut max_lengths = Vec::new();
    for (name, n) in &intent.nets {
        let Some(a) = &n.max_length else { continue };
        if a.strength != Strength::Hard {
            notes.push(Note { subject: format!("net {name}"), message: "soft max_length: the router takes hard limits only; left out".into() });
            continue;
        }
        let why = a.provenance.as_ref().map_or(String::new(), |p| format!("{} {}", p.source, p.locator.clone().unwrap_or_default()).trim().to_string());
        max_lengths.push(MaxLength { nets: vec![name.clone()], max_length_mm: a.value.mm, why });
    }

    let routing = Routing {
        version: 1,
        generator: format!("circuit-composer {}", env!("CARGO_PKG_VERSION")),
        weight_mapping: format!("router weight = {WEIGHT_SCALE} x intent weight (intent weights are relative, in (0, 1])"),
        regions,
        max_lengths,
        clear_under,
    };
    Ok((routing, notes))
}

/// Copper layers no neighbouring layer gives a ground return: no fill of a GROUND-class net on
/// the layer above or below. Judged by layer, from the fills as KiCad last computed them, so a
/// plane with a hole still counts.
pub fn unreferenced_layers(intent: &IntentFile, board: &Board) -> Vec<String> {
    let ground = |net: &str| intent.nets.get(net).and_then(|n| n.class.as_deref()) == Some("GROUND");
    let grounded: Vec<bool> = board.layers.iter().map(|l| board.fills.iter().any(|f| &f.layer == l && ground(&f.net))).collect();
    (0..board.layers.len()).filter(|&i| !((i > 0 && grounded[i - 1]) || grounded.get(i + 1).copied().unwrap_or(false))).map(|i| board.layers[i].clone()).collect()
}

/// The referenced copper layers, grouped by the ground-filled layers beside them: (planes, layers).
fn ground_groups(intent: &IntentFile, board: &Board, unref: &[String]) -> Vec<(Vec<String>, Vec<String>)> {
    let ground = |net: &str| intent.nets.get(net).and_then(|n| n.class.as_deref()) == Some("GROUND");
    let mut groups: Vec<(Vec<String>, Vec<String>)> = Vec::new();
    for (i, l) in board.layers.iter().enumerate().filter(|(_, l)| !unref.contains(l)) {
        let near: Vec<String> = [i.checked_sub(1), Some(i + 1)].into_iter().flatten().filter_map(|j| board.layers.get(j)).filter(|n| board.fills.iter().any(|f| &f.layer == *n && ground(&f.net))).cloned().collect();
        match groups.iter_mut().find(|(g, _)| *g == near) {
            Some((_, ls)) => ls.push(l.clone()),
            None => groups.push((near, vec![l.clone()])),
        }
    }
    groups
}

/// Rasterising step for a ground fill's voids, mm.
const VOID_STEP: f64 = 0.1;
/// The outline's rim left out of the voids: a plane pulls back from the edge, and copper keeps off
/// it anyway; counted, the rim would be one void the size of the board.
const VOID_RIM: f64 = 1.0;

/// For the referenced copper layers, the voids in the ground fills beside them: where a track on
/// that layer would have no ground under it (a through-hole's clearance, a thermal relief, a
/// keep-out). Each void is its bounding rectangle on a 0.1 mm grid, the layers sharing one set of
/// neighbouring fills together.
fn ground_voids(intent: &IntentFile, board: &Board, outline: &[P], unref: &[String]) -> Vec<(Vec<P>, Vec<String>)> {
    let ground = |net: &str| intent.nets.get(net).and_then(|n| n.class.as_deref()) == Some("GROUND");
    let groups = ground_groups(intent, board, unref);
    let Some((lo, hi)) = geom::bbox(outline.iter().copied()) else { return vec![] };
    let (nx, ny) = (((hi[0] - lo[0]) / VOID_STEP).ceil() as usize, ((hi[1] - lo[1]) / VOID_STEP).ceil() as usize);
    let edge_dist = |p: P| (0..outline.len()).map(|i| geom::point_seg_dist(p, outline[i], outline[(i + 1) % outline.len()])).fold(f64::INFINITY, f64::min);
    let mut out = Vec::new();
    for (near, layers) in groups {
        let fills: Vec<(&[P], (P, P))> = board.fills.iter().filter(|f| ground(&f.net) && near.contains(&f.layer)).filter_map(|f| Some((f.polygon.as_slice(), geom::bbox(f.polygon.iter().copied())?))).collect();
        let covered = |p: P| fills.iter().any(|(poly, (a, b))| p[0] >= a[0] && p[0] <= b[0] && p[1] >= a[1] && p[1] <= b[1] && geom::inside(poly, p));
        let mut void = vec![false; nx * ny];
        for y in 0..ny {
            for x in 0..nx {
                let p = [lo[0] + (x as f64 + 0.5) * VOID_STEP, lo[1] + (y as f64 + 0.5) * VOID_STEP];
                void[y * nx + x] = geom::inside(outline, p) && edge_dist(p) > VOID_RIM && !covered(p);
            }
        }
        // Connected pieces, each as its bounding rectangle.
        let mut seen = vec![false; nx * ny];
        for start in 0..nx * ny {
            if !void[start] || seen[start] {
                continue;
            }
            let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
            let mut stack = vec![start];
            seen[start] = true;
            while let Some(k) = stack.pop() {
                let (x, y) = (k % nx, k / nx);
                (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
                let mut push = |k: usize| {
                    if void[k] && !seen[k] {
                        seen[k] = true;
                        stack.push(k);
                    }
                };
                if x > 0 { push(k - 1); }
                if x + 1 < nx { push(k + 1); }
                if y > 0 { push(k - nx); }
                if y + 1 < ny { push(k + nx); }
            }
            let (a, b) = ([lo[0] + x0 as f64 * VOID_STEP, lo[1] + y0 as f64 * VOID_STEP], [lo[0] + (x1 + 1) as f64 * VOID_STEP, lo[1] + (y1 + 1) as f64 * VOID_STEP]);
            out.push((vec![a, [b[0], a[1]], b, [a[0], b[1]]], layers.clone()));
        }
    }
    out
}

/// A block's routing constraints as regions over the whole board, so a package's rules reach the
/// router as well as the checker:
///
/// - `via_count <= 0` on nets: no vias for them, anywhere;
/// - `unreferenced_length_mm <= 0` on nets: none of their track on a layer with no ground fill
///   beside it (`unreferenced_layers`), and on the other layers none over a void in the ground
///   fill beside them (`ground_voids`).
///
/// A hard constraint is a `never` rule; a soft one an `avoid`, its weight the constraint's (a via
/// then costs `10 × weight` mm of track, a track `10 × weight` times its length). The other
/// metrics are measured after routing, not routed to, and are noted as such.
fn block_regions(intent: &IntentFile, board: &Board, notes: &mut Vec<Note>) -> (Vec<RRegion>, Vec<ClearUnder>) {
    let mut out = Vec::new();
    let mut clear = Vec::new();
    let outline = match &board.outline {
        Some(o) => o.clone(),
        None => {
            if intent.blocks.values().any(|b| b.placement_goal.as_ref().is_some_and(|g| !g.constraints.is_empty())) {
                notes.push(Note { subject: "blocks".into(), message: "the board has no closed outline: block constraints not compiled to regions".into() });
            }
            return (out, clear);
        }
    };
    let unref = unreferenced_layers(intent, board);
    let voids = ground_voids(intent, board, &outline, &unref);
    for (id, b) in &intent.blocks {
        let Some(g) = &b.placement_goal else { continue };
        for (k, c) in g.constraints.iter().enumerate() {
            let m = &c.value;
            let subject = format!("block {id}");
            let zero_max = matches!(m.op, BoundOp::Le) && m.limit <= 0.0;
            let (what, vias, layers) = match m.metric {
                Metric::ViaCount if zero_max => ("no_vias", true, None),
                Metric::UnreferencedLengthMm if zero_max => ("over_ground", false, Some(unref.clone())),
                Metric::ViaCount | Metric::UnreferencedLengthMm | Metric::TraceLengthMm | Metric::DecouplingPathMm | Metric::PartDistanceMm | Metric::ClearanceMm => {
                    notes.push(Note { subject, message: format!("{:?} {:?} {}: measured after routing, not routed to", m.metric, m.op, m.limit) });
                    continue;
                }
                // The generator's metrics: a generated, locked layout keeps them.
                _ => continue,
            };
            let nets: Vec<String> = m.nets.iter().filter(|n| board.pads.iter().any(|p| &p.net == *n)).cloned().collect();
            if nets.is_empty() {
                notes.push(Note { subject, message: format!("{what}: none of its nets ({}) is on the board", m.nets.join(", ")) });
                continue;
            }
            let hard = c.strength == Strength::Hard;
            let weight = match (hard, c.weight) {
                (true, _) => None,
                (false, Some(w)) => Some(w * WEIGHT_SCALE),
                (false, None) => {
                    notes.push(Note { subject, message: format!("{what}: a soft constraint with no weight; left out") });
                    continue;
                }
            };
            let rule = || RRule { nets: Some(nets.clone()), classes: None, kind: (if hard { "never" } else { "avoid" }).into(), weight, vias };
            let name = format!("{id}:{what}:{k}");
            if !layers.as_ref().is_some_and(|l| l.is_empty()) {
                out.push(RRegion { name: name.clone(), polygon: outline.clone(), layers: layers.clone(), rules: vec![rule()] });
            }
            // Over ground: on the layers a ground fill references, the nets keep out of its voids
            // too (a through-hole's clearance, a thermal relief, a keep-out).
            if what == "over_ground" && hard {
                // And other nets' vias keep their plane holes from under them (only a hard rule:
                // the router has no price for it).
                for (planes, on) in ground_groups(intent, board, &unref) {
                    clear.push(ClearUnder { nets: nets.clone(), layers: on, planes, margin_mm: CLEAR_UNDER_MARGIN });
                }
            }
            if what == "over_ground" {
                for (v, (polygon, on)) in voids.iter().enumerate() {
                    out.push(RRegion { name: format!("{name}:void{v}"), polygon: polygon.clone(), layers: Some(on.clone()), rules: vec![rule()] });
                }
            }
        }
    }
    (out, clear)
}

pub const DRU_BEGIN: &str = "# --- circuit-composer: begin (generated by `ccomp compile`; edits here are overwritten)";
pub const DRU_END: &str = "# --- circuit-composer: end";

/// KiCad custom rules for the length limits, merged into an existing `.kicad_dru` text: the
/// block between the markers is replaced, everything else is kept.
pub fn merge_dru(existing: Option<&str>, routing: &Routing) -> String {
    let mut block = String::new();
    block.push_str(DRU_BEGIN);
    block.push('\n');
    for m in &routing.max_lengths {
        for net in &m.nets {
            block.push_str(&format!(
                "(rule \"cc max length {net}\"\n  (condition \"A.NetName == '{net}'\")\n  (constraint length (max {}mm)))\n",
                fmt_mm(m.max_length_mm)
            ));
        }
    }
    block.push_str(DRU_END);
    block.push('\n');
    match existing {
        None => format!("(version 1)\n\n{block}"),
        Some(text) => match (text.find(DRU_BEGIN), text.find(DRU_END)) {
            (Some(a), Some(b)) if b > a => {
                let end = b + DRU_END.len() + text[b + DRU_END.len()..].find('\n').map_or(0, |i| i + 1);
                format!("{}{}{}", &text[..a], block, &text[end..])
            }
            _ => format!("{}\n\n{}", text.trim_end(), block),
        },
    }
}

fn fmt_mm(v: f64) -> String {
    let s = format!("{v:.4}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn routing(mm: f64) -> Routing {
        Routing {
            version: 1,
            generator: "t".into(),
            weight_mapping: String::new(),
            regions: vec![],
            max_lengths: vec![MaxLength { nets: vec!["/XIN".into()], max_length_mm: mm, why: String::new() }],
            clear_under: vec![],
        }
    }

    #[test]
    fn block_constraints_become_regions() {
        let sq = |x0: f64, y0: f64, x1: f64, y1: f64| vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]];
        let board = Board {
            layers: ["F.Cu", "In1.Cu", "In2.Cu", "B.Cu"].map(String::from).to_vec(),
            parts: Default::default(),
            pads: vec![pcb::Pad { part: "U1".into(), pin: "1".into(), net: "/USB".into(), layers: vec!["F.Cu".into()], centre: [2.0, 2.0], shape: vec![sq(1.9, 1.9, 2.1, 2.1)] }],
            tracks: vec![],
            vias: vec![],
            // In1's ground covers the left 6 mm of the 10 mm board.
            fills: vec![pcb::Fill { net: "GND".into(), layer: "In1.Cu".into(), polygon: sq(0.0, 0.0, 6.0, 10.0) }],
            rules: None,
            outline: Some(sq(0.0, 0.0, 10.0, 10.0)),
        };
        let intent: IntentFile = serde_json::from_value(serde_json::json!({
            "schema_version": "0.1",
            "board": { "name": "t" },
            "nets": { "GND": { "class": "GROUND" }, "/USB": { "class": "DIFF_HS" } },
            "blocks": { "usb": { "type": "usb", "parts": [], "placement_goal": { "constraints": [
                { "value": { "metric": "unreferenced_length_mm", "op": "<=", "limit": 0.0, "nets": ["/USB", "/GONE"] }, "strength": "hard" },
                { "value": { "metric": "via_count", "op": "<=", "limit": 0.0, "nets": ["/USB"] }, "strength": "soft", "weight": 0.5 }
            ] } } }
        }))
        .unwrap();
        assert_eq!(unreferenced_layers(&intent, &board), ["In1.Cu", "B.Cu"]);
        let mut notes = vec![];
        let (rs, clear) = block_regions(&intent, &board, &mut notes);
        let by = |n: &str| rs.iter().find(|r| r.name == n).unwrap_or_else(|| panic!("no {n} in {:?}", rs.iter().map(|r| &r.name).collect::<Vec<_>>()));
        // Off the unreferenced layers, everywhere; only the nets on the board.
        let off = by("usb:over_ground:0");
        assert_eq!(off.layers.as_deref(), Some(&["In1.Cu".to_string(), "B.Cu".to_string()][..]));
        assert_eq!((off.rules[0].kind.as_str(), off.rules[0].nets.as_deref()), ("never", Some(&["/USB".to_string()][..])));
        // Off the void beside the fill, on the layers In1 references: x 6..9 inside the rim.
        let v = by("usb:over_ground:0:void0");
        assert_eq!(v.layers.as_deref(), Some(&["F.Cu".to_string(), "In2.Cu".to_string()][..]));
        let (a, b) = (v.polygon[0], v.polygon[2]);
        assert!((a[0] - 6.0).abs() < 0.11 && (b[0] - 9.0).abs() < 0.11 && (a[1] - 1.0).abs() < 0.11 && (b[1] - 9.0).abs() < 0.11, "{a:?} {b:?}");
        // Other nets' vias keep their In1 holes from under the hard over-ground net's tracks.
        assert_eq!(clear.len(), 1);
        assert_eq!((clear[0].nets.clone(), clear[0].layers.clone(), clear[0].planes.clone()), (vec!["/USB".to_string()], vec!["F.Cu".to_string(), "In2.Cu".to_string()], vec!["In1.Cu".to_string()]));
        // A soft no-vias rule is a price on vias.
        let nv = by("usb:no_vias:1");
        assert!(nv.rules[0].vias && nv.rules[0].kind == "avoid" && nv.rules[0].weight == Some(5.0));
    }

    #[test]
    fn dru_merge_keeps_and_replaces() {
        let base = "(version 1)\n\n(rule \"VBUS\" (condition \"A.NetName == 'VBUS'\") (constraint track_width (min 0.2mm)))\n";
        let once = merge_dru(Some(base), &routing(10.0));
        assert!(once.contains("track_width (min 0.2mm)") && once.contains("(max 10mm)"));
        let twice = merge_dru(Some(&once), &routing(12.5));
        assert!(twice.contains("(max 12.5mm)") && !twice.contains("(max 10mm)"));
        assert_eq!(twice.matches(DRU_BEGIN).count(), 1);
        assert!(twice.contains("track_width (min 0.2mm)"));
    }
}

/// A board's copper against a routing file's region rules: what a router (or a person) has done
/// that the regions forbid. `avoid` and `prefer` are prices, not limits, so they report the
/// length that pays them, not a violation.
#[derive(Debug, Clone, Serialize)]
pub struct RegionFinding {
    pub region: String,
    pub kind: String,
    pub net: String,
    /// `track`, `via` or `pour`.
    pub item: &'static str,
    pub layer: String,
    pub location: P,
    /// For `avoid`/`prefer`: track length that pays the price, mm.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub length_mm: Option<f64>,
    pub violation: bool,
}

/// Copper that only touches a region's edge is outside it: KiCad's own tolerance, 0.5 µm.
pub const EDGE_EPSILON_MM: f64 = 0.0005;

pub fn check_regions(board: &Board, routing: &Routing) -> Vec<RegionFinding> {
    let mut out = Vec::new();
    for r in &routing.regions {
        let on_layer = |l: &str| r.layers.as_ref().is_none_or(|ls| ls.iter().any(|x| x == l));
        for rule in &r.rules {
            let applies = |net: &str| match (&rule.nets, &rule.classes) {
                (Some(ns), _) => ns.iter().any(|n| n == net),
                (None, Some(cs)) => cs.iter().any(|c| c == "*") && !net.is_empty(),
                _ => false,
            };
            let kind = rule.kind.clone();
            let mut push = |net: &str, item: &'static str, layer: &str, location: P, length_mm: Option<f64>, violation: bool| {
                out.push(RegionFinding { region: r.name.clone(), kind: kind.clone(), net: net.to_string(), item, layer: layer.to_string(), location, length_mm, violation });
            };
            if rule.vias {
                for v in board.vias.iter().filter(|v| applies(&v.net)) {
                    // A via is reported once, on the first of its layers the region covers.
                    let Some(l) = v.layers.iter().find(|l| on_layer(l)) else { continue };
                    let touches = geom::point_poly_dist(v.at, &r.polygon) < v.size / 2.0 - EDGE_EPSILON_MM;
                    match rule.kind.as_str() {
                        "never" if touches => push(&v.net, "via", l, v.at, None, true),
                        "must" if !geom::inside(&r.polygon, v.at) => push(&v.net, "via", l, v.at, None, true),
                        _ => {}
                    }
                }
                continue;
            }
            for t in board.tracks.iter().filter(|t| applies(&t.net) && on_layer(&t.layer)) {
                let inside_len = clipped_length(t.a, t.b, &r.polygon);
                let len = geom::dist(t.a, t.b);
                let mid = geom::scale(geom::add(t.a, t.b), 0.5);
                match rule.kind.as_str() {
                    "never" if inside_len > 1e-6 || geom::polyline_poly_dist(&[t.a, t.b], &r.polygon) < t.width / 2.0 - EDGE_EPSILON_MM => {
                        push(&t.net, "track", &t.layer, mid, Some(inside_len), true)
                    }
                    "must" if len - inside_len > 1e-6 => push(&t.net, "track", &t.layer, mid, Some(len - inside_len), true),
                    "avoid" if inside_len > 1e-6 => push(&t.net, "track", &t.layer, mid, Some(inside_len), false),
                    "prefer" if len - inside_len > 1e-6 => push(&t.net, "track", &t.layer, mid, Some(len - inside_len), false),
                    _ => {}
                }
            }
            if rule.kind == "never" {
                for f in board.fills.iter().filter(|f| applies(&f.net) && on_layer(&f.layer)) {
                    // Overlap, not contact: a pour that stops at the region's edge is outside it.
                    if geom::poly_poly_dist(&f.polygon, &r.polygon) == 0.0 && geom::overlap_area(std::slice::from_ref(&r.polygon), Some(std::slice::from_ref(&f.polygon)), 0.01) > 1e-3 {
                        push(&f.net, "pour", &f.layer, geom::centroid(&r.polygon), None, true);
                    }
                }
            }
        }
    }
    out
}

/// Length of segment ab inside a polygon, by sampling every 10 µm.
fn clipped_length(a: P, b: P, poly: &[P]) -> f64 {
    let len = geom::dist(a, b);
    let n = (len / 0.01).ceil().max(1.0) as usize;
    let inside = (0..n).filter(|i| geom::inside(poly, geom::add(a, geom::scale(geom::sub(b, a), (*i as f64 + 0.5) / n as f64)))).count();
    len * inside as f64 / n as f64
}
