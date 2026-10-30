//! Recipes: board-level blocks a person names by function (a USB-C sink, a LiPo load-sharing
//! path, an indicator LED, an LED chain), as small circuits with parameters.
//!
//! A recipe (`library/recipes/NAME.json`) is written like a package's circuit: parts by role,
//! internal nets by role pin, ports. What a package can't do, a recipe can:
//!
//! - **Parameters** with defaults (`current_a`), and parameters with named choices (`led`:
//!   green or red), whose fields a part takes (`"from": "led"`) and whose numbers expressions
//!   read (`led.vf_v`).
//! - **Computed values**: a part with `choose` takes the stocked part (`library/stock.json`)
//!   nearest `target` between `min` and `max`, all expressions of the parameters.
//! - **Repetition**: a part, net or port pin with `for: "1..n"` is written once per `i`, and
//!   `{expr}` in a name is the expression's value (`D{i+1}`).
//!
//! An instance of a recipe in a composition is `{ "recipe": "led", "params": {...} }`. It expands
//! to a [`Circuit`] (so it composes as a package does) and a `ports.json`-shaped value (so the pin
//! solver can connect its interfaces).

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::expr::{eval, format_value};
use crate::{Circuit, CircuitPart, Port};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Recipe {
    pub name: String,
    pub summary: String,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub params: BTreeMap<String, Param>,
    pub parts: BTreeMap<String, RPart>,
    #[serde(default)]
    pub nets: BTreeMap<String, RNet>,
    pub ports: BTreeMap<String, RPort>,
    /// As in a package's `ports.json`, with pins named by port.
    #[serde(default)]
    pub interfaces: BTreeMap<String, Value>,
    /// Checks on the parameters: an expression that must hold, and what it means when it doesn't.
    #[serde(default)]
    pub requires: Vec<Require>,
    #[serde(default)]
    pub citations: Vec<Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Param {
    #[serde(default)]
    pub default: Option<Value>,
    #[serde(default)]
    pub unit: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
    /// Named choices, each a record of part fields (`symbol`, `value`, `footprint`, `lcsc`, `mpn`)
    /// and numbers (`vf_v`).
    #[serde(default)]
    pub choices: Option<BTreeMap<String, Map<String, Value>>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RPart {
    #[serde(rename = "for", default)]
    pub range: Option<String>,
    #[serde(default)]
    pub symbol: Option<String>,
    #[serde(default)]
    pub value: Option<String>,
    #[serde(default)]
    pub footprint: Option<String>,
    pub reference_ref: String,
    #[serde(default)]
    pub lcsc: Option<String>,
    #[serde(default)]
    pub mpn: Option<String>,
    #[serde(default)]
    pub near: Option<String>,
    #[serde(default)]
    pub block: Option<String>,
    /// A parameter with choices whose chosen record gives this part's fields.
    #[serde(default)]
    pub from: Option<String>,
    #[serde(default)]
    pub choose: Option<Choose>,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Choose {
    /// A table in `library/stock.json`.
    pub stock: String,
    pub target: String,
    #[serde(default)]
    pub min: Option<String>,
    #[serde(default)]
    pub max: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum RNet {
    Pins(Vec<String>),
    For {
        #[serde(rename = "for")]
        range: String,
        pins: Vec<String>,
    },
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RPort {
    pub kind: String,
    /// Role pins; `D{i}.1 for 1..n` is one per `i`.
    pub pins: Vec<String>,
    /// The rail's voltage range, as numbers or expressions, when the recipe sets it.
    #[serde(default)]
    pub volts: Option<Vec<Value>>,
    /// Logic levels, as in a package's `functions` (`vih`, `voh`, `vin_max`: expressions of
    /// `supply`), and the port whose rail `supply` is.
    #[serde(default)]
    pub levels: Option<Value>,
    #[serde(default)]
    pub supply: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Require {
    pub expr: String,
    pub message: String,
}

/// `library/stock.json`: parts a recipe can choose by value.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Stock {
    #[serde(default)]
    pub note: Option<String>,
    #[serde(flatten)]
    pub tables: BTreeMap<String, StockTable>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StockTable {
    #[serde(default)]
    pub note: Option<String>,
    pub symbol: String,
    pub footprint: String,
    #[serde(default)]
    pub checked: Option<String>,
    pub parts: Vec<StockPart>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StockPart {
    pub value: String,
    /// The value in SI units (ohms, farads).
    pub si: f64,
    pub lcsc: String,
    #[serde(default)]
    pub mpn: Option<String>,
    /// JLC's library type: basic, preferred, extended.
    #[serde(default, rename = "type")]
    pub kind: Option<String>,
}

/// One recipe instance, expanded.
#[derive(Debug, Clone, Serialize)]
pub struct Expanded {
    #[serde(skip)]
    pub circuit: Circuit,
    /// Shaped like a package's `ports.json`: one variant (`recipe`) whose pins are the ports.
    pub ports: Value,
    /// Each port's voltage range, where the recipe sets it.
    pub volts: BTreeMap<String, (f64, f64)>,
    /// Values the recipe computed, by role: what it wanted and what it took.
    pub chosen: BTreeMap<String, String>,
}

pub fn load_recipes(dir: &Path) -> Result<BTreeMap<String, Recipe>, String> {
    let mut out = BTreeMap::new();
    let Ok(rd) = std::fs::read_dir(dir) else { return Ok(out) };
    let mut paths: Vec<_> = rd.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|x| x == "json")).collect();
    paths.sort();
    for p in paths {
        let text = std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?;
        let r: Recipe = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", p.display()))?;
        out.insert(r.name.clone(), r);
    }
    Ok(out)
}

pub fn load_stock(path: &Path) -> Result<Stock, String> {
    if !path.is_file() {
        return Ok(Stock::default());
    }
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

/// The parameters' values: given, or defaulted. A choice parameter is its choice's name.
fn resolve(r: &Recipe, given: &BTreeMap<String, Value>) -> Result<BTreeMap<String, Value>, String> {
    if let Some(k) = given.keys().find(|k| !r.params.contains_key(*k)) {
        return Err(format!("{}: no parameter {k} ({})", r.name, r.params.keys().cloned().collect::<Vec<_>>().join(", ")));
    }
    let mut out = BTreeMap::new();
    for (k, p) in &r.params {
        let v = given.get(k).or(p.default.as_ref()).ok_or_else(|| format!("{}: parameter {k} has no default; give it", r.name))?;
        if let Some(ch) = &p.choices {
            let name = v.as_str().ok_or_else(|| format!("{}: {k} is one of {}", r.name, ch.keys().cloned().collect::<Vec<_>>().join(", ")))?;
            if !ch.contains_key(name) {
                return Err(format!("{}: {k} is one of {}, not {name}", r.name, ch.keys().cloned().collect::<Vec<_>>().join(", ")));
            }
        }
        out.insert(k.clone(), v.clone());
    }
    Ok(out)
}

struct Env<'a> {
    r: &'a Recipe,
    vals: BTreeMap<String, Value>,
    i: Option<f64>,
}

impl Env<'_> {
    fn num(&self, name: &str) -> Option<f64> {
        if name == "i" {
            return self.i;
        }
        if let Some((p, field)) = name.split_once('.') {
            let choice = self.vals.get(p)?.as_str()?;
            return self.r.params.get(p)?.choices.as_ref()?.get(choice)?.get(field)?.as_f64();
        }
        self.vals.get(name)?.as_f64()
    }
    fn eval(&self, s: &str) -> Result<f64, String> {
        eval(s, &|n| self.num(n)).map_err(|e| format!("{}: {e}", self.r.name))
    }
    /// `{expr}` replaced: a string parameter by its value, anything else by its number.
    fn fill(&self, s: &str) -> Result<String, String> {
        let mut out = String::new();
        let mut rest = s;
        while let Some(a) = rest.find('{') {
            let b = rest[a..].find('}').ok_or_else(|| format!("{}: unclosed {{ in {s}", self.r.name))? + a;
            out.push_str(&rest[..a]);
            let e = &rest[a + 1..b];
            match self.vals.get(e).and_then(Value::as_str) {
                Some(v) => out.push_str(v),
                None => {
                    let v = self.eval(e)?;
                    if (v - v.round()).abs() < 1e-9 {
                        out.push_str(&format!("{}", v.round() as i64));
                    } else {
                        out.push_str(&format!("{v}"));
                    }
                }
            }
            rest = &rest[b + 1..];
        }
        out.push_str(rest);
        Ok(out)
    }
    fn range(&self, s: &str) -> Result<Vec<f64>, String> {
        let (a, b) = s.split_once("..").ok_or_else(|| format!("{}: a range is A..B, not {s}", self.r.name))?;
        let (a, b) = (self.eval(a)?.round() as i64, self.eval(b)?.round() as i64);
        Ok((a..=b).map(|x| x as f64).collect())
    }
    fn with_i(&self, i: f64) -> Env<'_> {
        Env { r: self.r, vals: self.vals.clone(), i: Some(i) }
    }
}

/// Expand a recipe with the given parameters.
pub fn expand(r: &Recipe, given: &BTreeMap<String, Value>, stock: &Stock) -> Result<Expanded, String> {
    let env = Env { r, vals: resolve(r, given)?, i: None };
    for q in &r.requires {
        if env.eval(&q.expr)? == 0.0 {
            return Err(format!("{}: {} ({})", r.name, env.fill(&q.message)?, q.expr));
        }
    }
    let mut parts = BTreeMap::new();
    let mut chosen = BTreeMap::new();
    for (role, p) in &r.parts {
        let is: Vec<Option<f64>> = match &p.range {
            Some(rg) => env.range(rg)?.into_iter().map(Some).collect(),
            None => vec![None],
        };
        for i in is {
            let e = match i {
                Some(i) => env.with_i(i),
                None => Env { r, vals: env.vals.clone(), i: None },
            };
            let role = e.fill(role)?;
            let mut f: Map<String, Value> = Map::new();
            if let Some(from) = &p.from {
                let choice = e.vals.get(from).and_then(Value::as_str).ok_or_else(|| format!("{}: {role} takes its part from {from}, which is not a choice", r.name))?;
                f = r.params[from].choices.as_ref().and_then(|c| c.get(choice)).cloned().unwrap_or_default();
            }
            for (k, v) in [("symbol", &p.symbol), ("value", &p.value), ("footprint", &p.footprint), ("lcsc", &p.lcsc), ("mpn", &p.mpn)] {
                if let Some(v) = v {
                    f.insert(k.into(), Value::String(e.fill(v)?));
                }
            }
            if let Some(c) = &p.choose {
                let t = stock.tables.get(&c.stock).ok_or_else(|| format!("{}: {role} chooses from {}, which library/stock.json has no table of", r.name, c.stock))?;
                let target = e.eval(&c.target)?;
                let lo = c.min.as_deref().map(|m| e.eval(m)).transpose()?.unwrap_or(0.0);
                let hi = c.max.as_deref().map(|m| e.eval(m)).transpose()?.unwrap_or(f64::INFINITY);
                let best = t.parts.iter().filter(|s| s.si >= lo * (1.0 - 1e-9) && s.si <= hi * (1.0 + 1e-9)).min_by(|a, b| (a.si / target).ln().abs().total_cmp(&(b.si / target).ln().abs()));
                let s = best.ok_or_else(|| format!("{}: {role} wants {} between {} and {}; {} stocks none", r.name, format_value(target), format_value(lo), format_value(hi), c.stock))?;
                f.insert("symbol".into(), json!(t.symbol));
                f.insert("footprint".into(), json!(t.footprint));
                f.insert("value".into(), json!(s.value));
                f.insert("lcsc".into(), json!(s.lcsc));
                if let Some(m) = &s.mpn {
                    f.insert("mpn".into(), json!(m));
                }
                chosen.insert(role.clone(), format!("{} (wanted {}, {}..{})", s.value, format_value(target), format_value(lo), if hi.is_finite() { format_value(hi) } else { "".into() }));
            }
            let get = |k: &str| f.get(k).and_then(Value::as_str).map(str::to_string);
            let need = |k: &str| get(k).ok_or_else(|| format!("{}: {role} has no {k}", r.name));
            let part = CircuitPart {
                block: p.block.clone(),
                symbol: need("symbol")?,
                value: need("value")?,
                footprint: need("footprint")?,
                reference_ref: e.fill(&p.reference_ref)?,
                lcsc: get("lcsc").filter(|s| !s.is_empty()),
                mpn: get("mpn").filter(|s| !s.is_empty()),
                near: p.near.as_deref().map(|n| e.fill(n)).transpose()?,
            };
            parts.insert(role, part);
        }
    }
    let pin_list = |pins: &[String], e: &Env| -> Result<Vec<String>, String> {
        let mut out = Vec::new();
        for p in pins {
            match p.split_once(" for ") {
                Some((pat, rg)) => {
                    for i in e.range(rg)? {
                        out.push(e.with_i(i).fill(pat)?);
                    }
                }
                None => out.push(e.fill(p)?),
            }
        }
        Ok(out)
    };
    let mut nets: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (name, n) in &r.nets {
        match n {
            RNet::Pins(ps) => {
                nets.insert(env.fill(name)?, pin_list(ps, &env)?);
            }
            RNet::For { range, pins } => {
                for i in env.range(range)? {
                    let e = env.with_i(i);
                    nets.insert(e.fill(name)?, pin_list(pins, &e)?);
                }
            }
        }
    }
    let mut ports = BTreeMap::new();
    let mut volts = BTreeMap::new();
    let mut pins_json = Map::new();
    let mut functions = Map::new();
    for (name, p) in &r.ports {
        ports.insert(name.clone(), Port { kind: p.kind.clone(), pins: pin_list(&p.pins, &env)? });
        let mut func = json!({ "kind": p.kind });
        if let Some(v) = &p.volts {
            let n: Vec<f64> = v.iter().map(|x| match x {
                Value::Number(n) => Ok(n.as_f64().unwrap_or(0.0)),
                Value::String(s) => env.eval(s),
                _ => Err(format!("{}: {name}'s volts are numbers or expressions", r.name)),
            }).collect::<Result<_, _>>()?;
            let (lo, hi) = match n[..] {
                [x] => (x, x),
                [a, b] => (a, b),
                _ => return Err(format!("{}: {name}'s volts are [V] or [min, max]", r.name)),
            };
            volts.insert(name.clone(), (lo, hi));
            func["nominal_v"] = json!([lo, hi]);
        }
        if let Some(l) = &p.levels {
            func["levels"] = l.clone();
        }
        if let Some(sp) = &p.supply {
            if !r.ports.contains_key(sp) {
                return Err(format!("{}: {name}'s supply is {sp}, which is not a port", r.name));
            }
            func["supply"] = json!(sp);
        }
        functions.insert(name.clone(), func);
        pins_json.insert(name.clone(), json!({ "name": name, "function": name }));
    }
    // Every role pin a net or port names is a part's.
    for (what, pins) in nets.iter().map(|(n, p)| (format!("net {n}"), p)).chain(ports.iter().map(|(n, p)| (format!("port {n}"), &p.pins))) {
        for pin in pins {
            let role = pin.split('.').next().unwrap_or("");
            if !parts.contains_key(role) {
                return Err(format!("{}: {what} names {pin}, but there is no part {role}", r.name));
            }
        }
    }
    let ports_json = json!({
        "note": format!("Recipe {}: one variant, whose pins are the recipe's ports.", r.name),
        "functions": functions,
        "variants": { "recipe": { "pins": pins_json } },
        "interfaces": r.interfaces,
    });
    Ok(Expanded { circuit: Circuit { variant: "recipe".into(), parts, nets, ports }, ports: ports_json, volts, chosen })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn led() -> Recipe {
        serde_json::from_value(json!({
            "name": "led", "summary": "an LED",
            "params": {
                "rail_v": { "default": 3.3 },
                "current_a": { "default": 0.002 },
                "led": { "default": "green", "choices": { "green": { "symbol": "Device:LED", "value": "GREEN", "footprint": "LED_0603", "lcsc": "C1", "vf_v": 2.0 } } }
            },
            "requires": [{ "expr": "rail_v > led.vf_v", "message": "the rail is below the LED's {led} forward voltage" }],
            "parts": {
                "R1": { "reference_ref": "R1", "choose": { "stock": "r", "target": "(rail_v - led.vf_v) / current_a" } },
                "D1": { "reference_ref": "D1", "from": "led" }
            },
            "nets": { "A": ["R1.2", "D1.2"] },
            "ports": { "IN": { "kind": "signal", "pins": ["R1.1"] }, "GND": { "kind": "ground", "pins": ["D1.1"] } },
            "interfaces": { "in": { "type": "logic", "dir": "in", "pins": { "IO": "IN" } } }
        }))
        .unwrap()
    }

    fn stock() -> Stock {
        serde_json::from_value(json!({ "r": { "symbol": "Device:R", "footprint": "R_0402", "parts": [
            { "value": "470", "si": 470.0, "lcsc": "C470" }, { "value": "680", "si": 680.0, "lcsc": "C680" }, { "value": "1k", "si": 1000.0, "lcsc": "C1k" }
        ] } }))
        .unwrap()
    }

    #[test]
    fn a_resistor_from_the_parameters() {
        let x = expand(&led(), &BTreeMap::new(), &stock()).unwrap();
        // (3.3 - 2.0) / 2 mA = 650 ohm: 680 is nearest.
        assert_eq!(x.circuit.parts["R1"].value, "680");
        assert_eq!(x.circuit.parts["R1"].lcsc.as_deref(), Some("C680"));
        assert_eq!(x.circuit.parts["D1"].value, "GREEN");
        assert_eq!(x.ports["variants"]["recipe"]["pins"]["IN"]["name"], "IN");
        let x = expand(&led(), &[("current_a".to_string(), json!(0.001))].into(), &stock()).unwrap();
        assert_eq!(x.circuit.parts["R1"].value, "1k");
        let e = expand(&led(), &[("rail_v".to_string(), json!(1.8))].into(), &stock()).unwrap_err();
        assert!(e.contains("below the LED's green forward voltage"), "{e}");
        assert!(expand(&led(), &[("led".to_string(), json!("blue"))].into(), &stock()).is_err());
    }

    #[test]
    fn repetition() {
        let r: Recipe = serde_json::from_value(json!({
            "name": "chain", "summary": "a chain",
            "params": { "n": { "default": 3 } },
            "parts": { "D{i}": { "for": "1..n", "reference_ref": "D{i}", "symbol": "s", "value": "v", "footprint": "f" } },
            "nets": { "D{i}_DO": { "for": "1..n-1", "pins": ["D{i}.2", "D{i+1}.4"] } },
            "ports": {
                "DIN": { "kind": "signal", "pins": ["D1.4"] },
                "VDD": { "kind": "supply", "pins": ["D{i}.1 for 1..n"], "volts": [3.7, 5.5] }
            }
        }))
        .unwrap();
        let x = expand(&r, &BTreeMap::new(), &Stock::default()).unwrap();
        assert_eq!(x.circuit.parts.len(), 3);
        assert_eq!(x.circuit.nets["D2_DO"], ["D2.2", "D3.4"]);
        assert!(!x.circuit.nets.contains_key("D3_DO"));
        assert_eq!(x.circuit.ports["VDD"].pins, ["D1.1", "D2.1", "D3.1"]);
        assert_eq!(x.volts["VDD"], (3.7, 5.5));
    }
}
