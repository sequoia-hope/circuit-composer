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

use intent::{IntentFile, RegionKind, RuleMode, Selector, Strength};
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
}

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
    };
    Ok((routing, notes))
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
        }
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
