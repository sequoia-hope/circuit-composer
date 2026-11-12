//! Power contracts: what a power pin needs or gives, in a form the supply rule can match.
//!
//! A `power_in` or `power_out` function in `ports.json` may carry a `power` object (system spec,
//! Supply selection). Every figure is cited, like the rest of the package:
//!
//! - **power_in**: `v`, the recommended operating range, and `draw_a`, what it draws at a stated
//!   condition (`{typ, max, when}`). A package's draw on a port is the sum over the functions on
//!   it, so a figure sits on the function the datasheet gives it for (the RP2350's core current on
//!   VREG_VIN, not on IOVDD).
//! - **power_out**: `from`, the `power_in` function that feeds it, whose `v` is the range it runs
//!   over; `topology`; `v_out`, the regulated range; `i_max_a`; and how it behaves when the input
//!   is too low to regulate: `dropout_v` for an LDO, `r_on_ohm` for a buck at 100 % duty or a
//!   switch, `off_below_v` for one that turns off. Optional: `port`, the package circuit's port
//!   when the circuit makes the output (a buck past its inductor); `efficiency`; `iq_a`;
//!   `theta_ja_c_per_w` with `tj_max_c`; and `enable`, the function that turns it on, whether
//!   it is on when left open, and otherwise what to tie it to.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::Citation;

pub const TOPOLOGIES: [&str; 8] = ["ldo", "buck", "boost", "buck_boost", "charger", "switch", "diode", "source"];

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PowerIn {
    /// Recommended operating range, volts.
    pub v: [f64; 2],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draw_a: Option<Draw>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub citation: Citation,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Draw {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub typ: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    /// The condition the figures hold at.
    pub when: String,
    /// When the draw comes from another table than the range.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub citation: Option<Citation>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PowerOut {
    /// The `power_in` function feeding it; empty for a `source` (power arriving from off the board).
    #[serde(default)]
    pub from: String,
    /// The package circuit's port the output is on, when the circuit makes it rather than a pin
    /// (a buck's output is past its inductor and divider).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<String>,
    pub topology: String,
    /// Regulated output range, volts (worst case over line, load and temperature where given).
    pub v_out: [f64; 2],
    pub i_max_a: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dropout_v: Option<Dropout>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r_on_ohm: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub off_below_v: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub efficiency: Option<Efficiency>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iq_a: Option<f64>,
    /// Junction-to-ambient, for the package this variant is in, and the junction limit to hold.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theta_ja_c_per_w: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tj_max_c: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enable: Option<Enable>,
    /// Further inputs feeding the same output, each through its own pass element: a load-sharing
    /// path's VSYS is the input through a diode or the cell through a FET. The output's range is
    /// the union of every path's.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub also: Vec<Path>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub citation: Citation,
}

/// One more way into an output: a `switch` or `diode` from another input.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Path {
    pub from: String,
    pub topology: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dropout_v: Option<Dropout>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r_on_ohm: Option<f64>,
}

impl PowerOut {
    /// Each path into the output as an output of its own: the main one, then `also`.
    pub fn paths(&self) -> Vec<PowerOut> {
        let mut out = vec![PowerOut { also: vec![], ..self.clone() }];
        for p in &self.also {
            out.push(PowerOut { from: p.from.clone(), topology: p.topology.clone(), dropout_v: p.dropout_v.clone(), r_on_ohm: p.r_on_ohm, off_below_v: None, also: vec![], efficiency: None, ..self.clone() });
        }
        out
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Dropout {
    /// The most the input must exceed the output by, volts.
    pub max: f64,
    /// At this load current.
    pub at_a: f64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Efficiency {
    pub value: f64,
    pub at_a: f64,
    pub vin: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub citation: Option<Citation>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Enable {
    /// The function that turns the output on.
    pub function: String,
    /// The output is on with the enable left open (an internal pull-up, or the package circuit's).
    #[serde(default)]
    pub on_when_open: bool,
    /// Otherwise, what to tie it to when nothing drives it: a function of the package (`VIN`).
    /// With neither, it must be driven.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tie: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

pub enum Power {
    In(PowerIn),
    Out(PowerOut),
}

/// A function's power contract, if it has one: `Ok(None)` when it carries no `power`.
pub fn of(function: &Value) -> Result<Option<Power>, String> {
    let Some(p) = function.get("power") else { return Ok(None) };
    match function.get("kind").and_then(Value::as_str) {
        Some("power_in") => serde_json::from_value(p.clone()).map(|x| Some(Power::In(x))).map_err(|e| e.to_string()),
        Some("power_out") => serde_json::from_value(p.clone()).map(|x| Some(Power::Out(x))).map_err(|e| e.to_string()),
        k => Err(format!("power on a {} function; only power_in and power_out carry one", k.unwrap_or("kindless"))),
    }
}

fn range_ok(r: &[f64; 2]) -> bool {
    r[0].is_finite() && r[1].is_finite() && r[0] > 0.0 && r[0] <= r[1]
}

/// An input's range may start at 0: no minimum (a passive path, which takes whatever it is given
/// up to its ceiling).
fn input_range_ok(r: &[f64; 2]) -> bool {
    r[0].is_finite() && r[1].is_finite() && r[0] >= 0.0 && r[0] <= r[1] && r[1] > 0.0
}

/// What is wrong with a package's power contracts.
pub fn check(functions: &BTreeMap<String, Value>, sources: Option<&BTreeSet<String>>) -> Vec<String> {
    let mut errs = Vec::new();
    let uncited = |c: &Citation| sources.is_some_and(|s| !s.contains(&c.source));
    let ins: BTreeMap<&str, PowerIn> = functions.iter().filter_map(|(k, f)| match of(f) {
        Ok(Some(Power::In(p))) => Some((k.as_str(), p)),
        _ => None,
    }).collect();
    for (name, f) in functions {
        let p = match of(f) {
            Ok(Some(p)) => p,
            Ok(None) => continue,
            Err(e) => {
                errs.push(format!("{name}: power: {e}"));
                continue;
            }
        };
        match p {
            Power::In(p) => {
                if !input_range_ok(&p.v) {
                    errs.push(format!("{name}: power.v {:?} is not a range of volts (0 for no minimum)", p.v));
                }
                if uncited(&p.citation) {
                    errs.push(format!("{name}: power cites {}, not in sources.json", p.citation.source));
                }
                if let Some(d) = &p.draw_a {
                    if d.typ.is_none() && d.max.is_none() {
                        errs.push(format!("{name}: draw_a gives neither typ nor max"));
                    }
                    if let (Some(t), Some(m)) = (d.typ, d.max) {
                        if t > m {
                            errs.push(format!("{name}: draw_a typ {t} A is above its max {m} A"));
                        }
                    }
                    if d.typ.into_iter().chain(d.max).any(|x| !(x >= 0.0)) {
                        errs.push(format!("{name}: draw_a must be non-negative"));
                    }
                    if let Some(c) = d.citation.as_ref().filter(|c| uncited(c)) {
                        errs.push(format!("{name}: draw_a cites {}, not in sources.json", c.source));
                    }
                }
            }
            Power::Out(p) => {
                if !TOPOLOGIES.contains(&p.topology.as_str()) {
                    errs.push(format!("{name}: topology {} is not one of {}", p.topology, TOPOLOGIES.join(", ")));
                }
                // A switch or diode passes its input through: its v_out is only a ceiling.
                let pass = matches!(p.topology.as_str(), "switch" | "diode");
                if !(range_ok(&p.v_out) || pass && input_range_ok(&p.v_out)) {
                    errs.push(format!("{name}: power.v_out {:?} is not a range of positive volts", p.v_out));
                }
                if !(p.i_max_a > 0.0) {
                    errs.push(format!("{name}: i_max_a must be positive"));
                }
                if uncited(&p.citation) {
                    errs.push(format!("{name}: power cites {}, not in sources.json", p.citation.source));
                }
                if p.topology != "source" && !ins.contains_key(p.from.as_str()) {
                    errs.push(format!("{name}: from {}, which is not a power_in function with a power contract", p.from));
                }
                // How it behaves below regulation, where that is a question.
                let says = p.dropout_v.is_some() || p.r_on_ohm.is_some() || p.off_below_v.is_some();
                if matches!(p.topology.as_str(), "ldo" | "buck" | "switch" | "diode" | "charger") && !says {
                    errs.push(format!("{name}: a {} says how it behaves below regulation (dropout_v, r_on_ohm or off_below_v)", p.topology));
                }
                if p.topology == "ldo" && p.dropout_v.is_none() {
                    errs.push(format!("{name}: an ldo gives its dropout_v"));
                }
                if let Some(e) = &p.efficiency {
                    if !(e.value > 0.0 && e.value <= 1.0) {
                        errs.push(format!("{name}: efficiency {} is not a fraction", e.value));
                    }
                    if let Some(c) = e.citation.as_ref().filter(|c| uncited(c)) {
                        errs.push(format!("{name}: efficiency cites {}, not in sources.json", c.source));
                    }
                }
                for x in &p.also {
                    if !matches!(x.topology.as_str(), "switch" | "diode") {
                        errs.push(format!("{name}: also from {}: a {} path; only switch and diode paths join an output", x.from, x.topology));
                    }
                    if !ins.contains_key(x.from.as_str()) {
                        errs.push(format!("{name}: also from {}, which is not a power_in function with a power contract", x.from));
                    }
                    if x.dropout_v.is_none() && x.r_on_ohm.is_none() {
                        errs.push(format!("{name}: also from {}: give its dropout_v or r_on_ohm", x.from));
                    }
                }
                if p.theta_ja_c_per_w.is_some() != p.tj_max_c.is_some() {
                    errs.push(format!("{name}: theta_ja_c_per_w and tj_max_c come together"));
                }
                if let Some(en) = &p.enable {
                    if !functions.contains_key(&en.function) {
                        errs.push(format!("{name}: enable is {}, not a function", en.function));
                    }
                    if let Some(t) = en.tie.as_ref().filter(|t| !functions.contains_key(*t)) {
                        errs.push(format!("{name}: enable ties to {t}, not a function"));
                    }
                }
            }
        }
    }
    errs
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fns(v: Value) -> BTreeMap<String, Value> {
        serde_json::from_value(v).unwrap()
    }

    #[test]
    fn a_regulator_names_its_input_and_how_it_drops_out() {
        let c = json!({"source": "ds", "locator": "p.4"});
        let good = fns(json!({
            "IN": {"kind": "power_in", "power": {"v": [2.7, 38.0], "citation": c}},
            "EN": {"kind": "logic_in"},
            "OUT": {"kind": "power_out", "power": {"from": "IN", "topology": "ldo", "v_out": [3.247, 3.373], "i_max_a": 0.15,
                     "dropout_v": {"max": 0.48, "at_a": 0.15}, "enable": {"function": "EN", "tie": "IN"}, "citation": c}}
        }));
        let src: BTreeSet<String> = ["ds".to_string()].into();
        assert!(check(&good, Some(&src)).is_empty(), "{:?}", check(&good, Some(&src)));

        let bad = fns(json!({
            "IN": {"kind": "power_in", "power": {"v": [5.0, 2.0], "draw_a": {"when": "always"}, "citation": c}},
            "OUT": {"kind": "power_out", "power": {"from": "VIN", "topology": "ldo", "v_out": [3.3, 3.3], "i_max_a": 0.15, "citation": {"source": "other", "locator": "p.1"}}},
            "SW": {"kind": "switch", "power": {"v": [1.0, 2.0]}}
        }));
        let e = check(&bad, Some(&src)).join("\n");
        for want in ["IN: power.v", "IN: draw_a gives neither", "OUT: from VIN", "OUT: an ldo gives its dropout_v", "OUT: power cites other", "SW: power: power on a switch"] {
            assert!(e.contains(want), "missing {want:?} in\n{e}");
        }
    }
}
