//! Fast-tier verifier checks. Each check measures one physics proxy on board geometry and, when
//! given a limit, reports pass or fail with the stage that has to fix it.

pub mod bridge;
pub mod buck;
pub mod protect;
pub mod nets;
pub mod svg;

/// Synthetic boards for the checks' tests.
#[cfg(test)]
pub(crate) mod test_board {
    use std::collections::BTreeMap;

    use pcb::geom::P;
    use pcb::{Board, Fill, Pad, Part, Track};

    /// A board of 1 mm square F.Cu pads, `(part, pin, net, at)`.
    pub fn board(pads: &[(&str, &str, &str, P)], tracks: Vec<Track>, fills: Vec<Fill>) -> Board {
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

    pub fn track(net: &str, a: P, b: P) -> Track {
        Track { a, b, width: 0.2, layer: "F.Cu".into(), net: net.into(), locked: false }
    }
}

use serde::Serialize;

/// Where a failure has to be fixed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FixStage {
    Intent,
    Floorplan,
    Block,
    Route,
}

#[derive(Debug, Clone, Serialize)]
pub struct Measurement {
    /// The check, e.g. `hot_loop_area_mm2`.
    pub check: &'static str,
    pub block: String,
    pub nets: Vec<String>,
    /// A representative point, board mm.
    pub location: [f64; 2],
    pub measured: f64,
    pub unit: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<Limit>,
    /// `None` when there is no limit to judge by.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub passed: Option<bool>,
    pub fix_stage: FixStage,
    /// How the number was obtained, when it is not the obvious way.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Limit {
    /// `<=` or `>=`.
    pub op: &'static str,
    pub value: f64,
}

impl Measurement {
    pub fn judge(&mut self, limit: Option<Limit>) {
        self.limit = limit;
        self.passed = limit.map(|l| match l.op {
            ">=" => self.measured >= l.value,
            _ => self.measured <= l.value,
        });
    }
}
