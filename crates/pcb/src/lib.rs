//! A KiCad board as copper per net: what the fast verifier checks measure and what the block
//! generators place against. Millimetres, KiCad's frame (Y down).

pub mod geom;
pub mod graph;

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
        let (pcb, copper) = kicad_pcb::parse_board(text).map_err(|e| e.to_string())?;
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
                layers: copper.expand(&pc.layers),
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
                let net = z.net_name.clone();
                z.filled.iter().map(move |(layer, poly)| Fill { net: net.clone(), layer: layer.clone(), polygon: poly.iter().copied().map(mm).collect() })
            })
            .collect();

        Ok(Board { layers, parts, pads, tracks, vias, fills, rules: None })
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
