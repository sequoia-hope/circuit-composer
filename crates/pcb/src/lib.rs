//! A KiCad board as copper per net: what the fast verifier checks measure and what the block
//! generators place against. Millimetres, KiCad's frame (Y down).

pub mod geom;
pub mod graph;

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::path::Path;

use geom::P;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Board {
    /// Copper layer names, front to back.
    pub layers: Vec<String>,
    pub parts: BTreeMap<String, Part>,
    pub pads: Vec<Pad>,
    pub tracks: Vec<Track>,
    pub vias: Vec<Via>,
    /// Zone fills as KiCad last computed them.
    pub fills: Vec<Fill>,
    /// Net classes and clearances from the `.kicad_pro` (and `.kicad_dru`) beside the board.
    #[serde(skip)]
    pub rules: Option<kicad_pcb::Rules>,
    /// The board's outer Edge.Cuts loop, arcs as chords within 5 um; none when it has no
    /// closed outline.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outline: Option<Vec<P>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Part {
    pub reference: String,
    pub value: String,
    pub footprint: String,
    pub at: P,
    pub rotation: f64,
    pub back: bool,
    pub locked: bool,
    /// Convex hull of the courtyard, board frame.
    pub courtyard: Option<Vec<P>>,
    /// Indices into `Board::pads`.
    pub pads: Vec<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Pad {
    pub part: String,
    pub pin: String,
    pub net: String,
    pub layers: Vec<String>,
    pub centre: P,
    /// Copper outline, one or more polygons (a union).
    pub shape: Vec<Vec<P>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Track {
    pub a: P,
    pub b: P,
    pub width: f64,
    pub layer: String,
    pub net: String,
    pub locked: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Via {
    pub at: P,
    pub size: f64,
    pub drill: f64,
    /// Every copper layer the barrel joins.
    pub layers: Vec<String>,
    pub net: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Fill {
    pub net: String,
    pub layer: String,
    pub polygon: Vec<P>,
}

const MM: f64 = 1e3;

fn mm(p: [f64; 2]) -> P {
    [p[0] * MM, p[1] * MM]
}

impl Board {
    pub fn load(path: &Path) -> Result<Board, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut board = Board::parse(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        board.rules = kicad_pcb::Rules::read(&path.with_extension("kicad_pro")).ok();
        Ok(board)
    }

    /// The copper clearance KiCad requires between two nets, mm: the larger of their classes'
    /// and the board minimum. `None` without a project file.
    pub fn clearance(&self, a: &str, b: &str) -> Option<f64> {
        self.rules.as_ref().map(|r| r.clearance(a, b) * MM)
    }

    pub fn parse(text: &str) -> Result<Board, String> {
        let text = normalise_track_layers(text);
        let (pcb, copper) = kicad_pcb::parse_board(&text).map_err(|e| e.to_string())?;
        let net_name: BTreeMap<u32, String> = pcb.nets.iter().map(|n| (n.number, n.name.clone())).collect();
        let name = |n: u32| net_name.get(&n).cloned().unwrap_or_default();
        let layers: Vec<String> = copper.layers.iter().map(|l| l.name.clone()).collect();

        let mut parts = BTreeMap::new();
        for (i, fp) in pcb.footprints.iter().enumerate() {
            parts.insert(
                fp.reference.clone(),
                Part {
                    reference: fp.reference.clone(),
                    value: fp.value.clone(),
                    footprint: fp.library_id.clone(),
                    at: mm(fp.at),
                    rotation: fp.rotation_deg,
                    back: fp.side == kicad_pcb::Side::Back,
                    locked: copper.locked_footprints.get(i).copied().unwrap_or(false),
                    courtyard: copper.courtyards.get(i).cloned().flatten().map(|c| c.into_iter().map(mm).collect()),
                    pads: Vec::new(),
                },
            );
        }

        let mut pads = Vec::new();
        for pc in &copper.pads {
            let pad_layers = copper.expand(&pc.layers);
            if pad_layers.is_empty() {
                continue;
            }
            let fp = &pcb.footprints[pc.footprint];
            let pad = &fp.pads[pc.pad];
            let shape = match &pc.outline {
                kicad_pcb::Outline::Circle { centre, radius } => vec![geom::circle(mm(*centre), radius * MM, 24)],
                kicad_pcb::Outline::Polygons(ps) => ps.iter().map(|p| p.iter().copied().map(mm).collect()).collect(),
            };
            let index = pads.len();
            pads.push(Pad {
                part: fp.reference.clone(),
                pin: pad.number.clone(),
                net: pad.net.as_ref().map(|n| n.name.clone()).unwrap_or_default(),
                layers: pad_layers,
                centre: mm(pad.position),
                shape,
            });
            if let Some(p) = parts.get_mut(&fp.reference) {
                p.pads.push(index);
            }
        }

        let mut tracks: Vec<Track> = copper
            .tracks
            .iter()
            .map(|t| Track { a: mm(t.start), b: mm(t.end), width: t.width * MM, layer: t.layer.clone(), net: name(t.net), locked: t.locked })
            .collect();
        // Arcs as two chords through their midpoint: close enough for loop areas and gaps.
        for a in &copper.arcs {
            for (s, e) in [(a.start, a.mid), (a.mid, a.end)] {
                tracks.push(Track { a: mm(s), b: mm(e), width: a.width * MM, layer: a.layer.clone(), net: name(a.net), locked: a.locked });
            }
        }

        let vias = copper
            .vias
            .iter()
            .map(|v| {
                let (i, j) = (layers.iter().position(|l| *l == v.layers[0]), layers.iter().position(|l| *l == v.layers[1]));
                let span = match (i, j) {
                    (Some(i), Some(j)) => layers[i.min(j)..=i.max(j)].to_vec(),
                    _ => layers.clone(),
                };
                Via { at: mm(v.at), size: v.size * MM, drill: v.drill * MM, layers: span, net: name(v.net) }
            })
            .collect();

        let fills = copper
            .zones
            .iter()
            .filter(|z| z.keepout.is_none())
            .flat_map(|z| {
                // KiCad <= 9 writes `(net N) (net_name "...")`; KiCad 10 writes `(net "...")` with
                // no `net_name` child, which `kicad_pcb` numbers into `z.net`.
                let net = if z.net_name.is_empty() { name(z.net) } else { z.net_name.clone() };
                z.filled.iter().map(move |(layer, poly)| Fill { net: net.clone(), layer: layer.clone(), polygon: poly.iter().copied().map(mm).collect() })
            })
            .collect();

        let outline = pcb.outline_loops().ok().map(|l| l.outer.points(5e-6).into_iter().map(mm).collect());
        Ok(Board { layers, parts, pads, tracks, vias, fills, rules: None, outline })
    }

    /// The pad `REF.PIN`. Footprints with several pads of one number (exposed pads, connector
    /// shells) give the first.
    pub fn pad(&self, ref_pin: &str) -> Option<&Pad> {
        let (r, pin) = ref_pin.split_once('.')?;
        let part = self.parts.get(r)?;
        part.pads.iter().map(|&i| &self.pads[i]).find(|p| p.pin == pin)
    }

    /// Every copper shape of a net, per layer, as polygons.
    pub fn net_copper(&self, net: &str) -> Vec<(String, Vec<P>)> {
        let mut out = Vec::new();
        for p in self.pads.iter().filter(|p| p.net == net) {
            for l in &p.layers {
                for s in &p.shape {
                    out.push((l.clone(), s.clone()));
                }
            }
        }
        for t in self.tracks.iter().filter(|t| t.net == net) {
            out.push((t.layer.clone(), geom::capsule(t.a, t.b, t.width)));
        }
        for v in self.vias.iter().filter(|v| v.net == net) {
            for l in &v.layers {
                out.push((l.clone(), geom::circle(v.at, v.size / 2.0, 16)));
            }
        }
        for f in self.fills.iter().filter(|f| f.net == net) {
            out.push((f.layer.clone(), f.polygon.clone()));
        }
        out
    }
}

/// KiCad 8+ writes `(layers "B.Cu" "B.Mask")` instead of `(layer "B.Cu")` on a `segment` or `arc`
/// whose solder mask is opened (common on exposed power busbars and Altium imports). Rewrites only
/// direct `(layers ...)` children of `(segment ...)` and `(arc ...)` to their copper `(layer ...)`.
fn normalise_track_layers(text: &str) -> Cow<'_, str> {
    let bytes = text.as_bytes();
    let mut out: Option<String> = None;
    let mut copied = 0usize;
    let mut i = 0usize;
    let mut depth = 0usize;
    let mut in_string = false;
    let mut track_depth: Option<usize> = None;

    while i < bytes.len() {
        let b = bytes[i];
        if in_string {
            if b == b'\\' {
                i += 2;
                continue;
            }
            if b == b'"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        match b {
            b'"' => {
                in_string = true;
                i += 1;
            }
            b'(' => {
                depth += 1;
                let rest = &text[i + 1..];
                let kw_end = rest.find(|c: char| c.is_ascii_whitespace() || c == ')' || c == '(').unwrap_or(rest.len());
                let kw = &rest[..kw_end];
                if track_depth.is_none() && (kw == "segment" || kw == "arc") {
                    track_depth = Some(depth);
                    i += 1 + kw_end;
                } else if track_depth == Some(depth - 1) && kw == "layers" {
                    // Scan to the matching ')' of this `(layers ...)` form.
                    let start = i;
                    let mut j = i + 1 + kw_end;
                    let mut sub_depth = 1usize;
                    let mut sub_str = false;
                    while j < bytes.len() && sub_depth > 0 {
                        match (sub_str, bytes[j]) {
                            (true, b'\\') => {
                                j += 2;
                                continue;
                            }
                            (true, b'"') => sub_str = false,
                            (false, b'"') => sub_str = true,
                            (false, b'(') => sub_depth += 1,
                            (false, b')') => sub_depth -= 1,
                            _ => {}
                        }
                        j += 1;
                    }
                    let form = &text[start..j];
                    if let Some(cu) = first_copper_layer(&form[1 + kw_end..form.len().saturating_sub(1)]) {
                        let buf = out.get_or_insert_with(|| String::with_capacity(text.len()));
                        buf.push_str(&text[copied..start]);
                        let newlines = form.bytes().filter(|&c| c == b'\n').count();
                        buf.push_str(&format!("(layer \"{cu}\")"));
                        for _ in 0..newlines {
                            buf.push('\n');
                        }
                        copied = j;
                    }
                    depth -= 1;
                    i = j;
                } else {
                    i += 1;
                }
            }
            b')' => {
                if track_depth == Some(depth) {
                    track_depth = None;
                }
                depth = depth.saturating_sub(1);
                i += 1;
            }
            _ => i += 1,
        }
    }

    match out {
        Some(mut buf) => {
            buf.push_str(&text[copied..]);
            Cow::Owned(buf)
        }
        None => Cow::Borrowed(text),
    }
}

fn first_copper_layer(s: &str) -> Option<&str> {
    for tok in s.split_ascii_whitespace() {
        let name = tok.trim_matches('"');
        if name.ends_with(".Cu") && !name.contains('*') {
            return Some(name);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exposed_mask_segment_layers_parse() {
        let src = r#"(kicad_pcb (version 20241229) (generator "pcbnew")
          (general (thickness 1.6))
          (layers (0 "F.Cu" signal) (31 "B.Cu" signal) (36 "B.Mask" user))
          (net 0 "")
          (net 1 "GND")
          (footprint "R:R_0402" (layer "F.Cu") (at 0 0)
            (property "Reference" "R1")
            (property "Value" "10k")
            (pad "1" smd rect (at 0 0) (size 0.5 0.5) (layers "F.Cu" "F.Mask") (net 1 "GND")))
          (segment (start 0 0) (end 5 0) (width 0.25) (layers "B.Cu" "B.Mask") (solder_mask_margin 0) (net 1))
        )"#;
        let b = Board::parse(src).expect("parse exposed-mask segment");
        assert_eq!(b.tracks.len(), 1);
        assert_eq!(b.tracks[0].layer, "B.Cu");
        assert_eq!(b.tracks[0].net, "GND");
        assert_eq!(b.pads.len(), 1);
    }

    #[test]
    fn paste_only_pads_are_ignored() {
        // KiCad 10 0201 footprints carry unnamed `(layers "F.Paste")` stencil apertures alongside
        // the two copper pads; they are not electrical pads.
        let src = r#"(kicad_pcb (version 20260206) (generator "pcbnew")
          (general (thickness 1.6))
          (layers (0 "F.Cu" signal) (2 "B.Cu" signal) (13 "F.Paste" user) (1 "F.Mask" user))
          (footprint "Capacitor_SMD:C_0201_0603Metric" (layer "F.Cu") (at 0 0)
            (property "Reference" "C1")
            (property "Value" "100n")
            (pad "" smd roundrect (at -0.345 0) (size 0.318 0.36) (layers "F.Paste") (roundrect_rratio 0.25))
            (pad "" smd roundrect (at 0.345 0) (size 0.318 0.36) (layers "F.Paste") (roundrect_rratio 0.25))
            (pad "1" smd roundrect (at -0.32 0) (size 0.46 0.4) (layers "F.Cu" "F.Mask") (roundrect_rratio 0.25) (net "+3.3V"))
            (pad "2" smd roundrect (at 0.32 0) (size 0.46 0.4) (layers "F.Cu" "F.Mask") (roundrect_rratio 0.25) (net "GND")))
        )"#;
        let b = Board::parse(src).expect("parse KiCad 10 0201 footprint");
        assert_eq!(b.pads.len(), 2);
        assert_eq!(b.parts["C1"].pads.len(), 2);
    }

    #[test]
    fn kicad10_zone_fills_preserve_net_name() {
        // KiCad 10 omits `(net_name "...")` on zones, writing `(net "GND")` directly.
        let src = r#"(kicad_pcb (version 20260206) (generator "pcbnew")
          (general (thickness 1.6))
          (layers (0 "F.Cu" signal) (1 "In1.Cu" signal) (2 "B.Cu" signal))
          (zone (net "GND") (layer "In1.Cu")
            (polygon (pts (xy 0 0) (xy 10 0) (xy 10 10) (xy 0 10)))
            (filled_polygon (layer "In1.Cu") (pts (xy 0 0) (xy 10 0) (xy 10 10) (xy 0 10))))
        )"#;
        let b = Board::parse(src).expect("parse KiCad 10 zone");
        assert_eq!(b.fills.len(), 1);
        assert_eq!(b.fills[0].net, "GND");
        assert_eq!(b.fills[0].layer, "In1.Cu");
    }
}
