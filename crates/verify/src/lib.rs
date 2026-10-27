//! Fast-tier verifier checks. Each check measures one physics proxy on board geometry and, when
//! given a limit, reports pass or fail with the stage that has to fix it.

pub mod buck;
pub mod svg;

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
