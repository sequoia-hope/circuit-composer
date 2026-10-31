//! Electrical domains: whether one pin's output can drive another's input.
//!
//! A pin's function (`ports.json`, under `functions`) may give its logic levels as expressions of
//! `supply`, the voltage of its supply rail: `vih`, `vil` (what it reads as high and low), `voh`,
//! `vol` (what it drives), `vin_max` (the most it takes), and `vin_max_unpowered` with `when`
//! where that limit holds only while the chip is powered (the RP2350's 5 V-tolerant pins).
//! Levels given `at_supply_v` hold only for a rail within 5 % of it; elsewhere they aren't judged.
//!
//! A datasheet's `vol` and `voh` hold at a stated current (`iol_ma`, `ioh_ma`: the RP2350's at its
//! drive setting, 2 mA at the weakest). A receiver that says what it draws (`iin_ua`, its input
//! leakage), with the pull-ups and pull-downs its package puts on the pin, loads the driver far
//! less than that: the driver's output transistor is then in its linear region, where current
//! rises less than in proportion to the voltage across it as that voltage grows, so the drop at a
//! smaller current is at most the stated drop scaled by the ratio of currents. A 1 uA input then
//! sees a low of 0.25 mV from a pin rated 0.5 V at 2 mA, not 0.5 V.
//! A rail's voltage comes from the composition's `rails`, a recipe port's `volts`, or a package
//! pin that is a `power_out` with `nominal_v`.

use std::collections::BTreeMap;

use compose::Composition;
use serde_json::Value;

use crate::{port_of, Package};

/// Board net → (lowest, highest) volts.
pub type Rails = BTreeMap<String, (f64, f64)>;

fn range(v: &Value) -> Option<(f64, f64)> {
    match v {
        Value::Number(n) => n.as_f64().map(|x| (x, x)),
        Value::Array(a) => match &a[..] {
            [x] => x.as_f64().map(|x| (x, x)),
            [a, b] => Some((a.as_f64()?, b.as_f64()?)),
            _ => None,
        },
        _ => None,
    }
}

/// The board net an instance's port is on.
pub fn net_of(comp: &Composition, end: &str) -> Option<String> {
    comp.nets.iter().find(|(_, ps)| ps.iter().any(|p| p == end)).map(|(n, _)| n.clone())
}

/// Every rail's voltage the composition or its parts say.
pub fn rails(comp: &Composition, pkgs: &BTreeMap<String, Package>) -> Rails {
    let mut out = Rails::new();
    for (name, pk) in pkgs {
        for (port, v) in &pk.volts {
            if let Some(n) = net_of(comp, &format!("{name}.{port}")) {
                out.insert(n, *v);
            }
        }
        let Some(inst) = comp.instances.get(name) else { continue };
        let Some(variant) = pk.ports.variants.get(&inst.variant) else { continue };
        for (num, pin) in &variant.pins {
            let Some(f) = pin.function.as_ref().and_then(|f| pk.ports.functions.get(f)) else { continue };
            if f["kind"] != "power_out" {
                continue;
            }
            let (Some(v), Some(port)) = (range(&f["nominal_v"]), port_of(&pk.circuit, num, &pin.name)) else { continue };
            if let Some(n) = net_of(comp, &format!("{name}.{port}")) {
                out.insert(n, v);
            }
        }
    }
    for (n, v) in &comp.rails {
        if let Some(r) = range(v) {
            out.insert(n.clone(), r);
        }
    }
    out
}

/// One pin's levels, worst case over its rail's range.
#[derive(Debug, Clone, Default)]
pub struct Lv {
    pub vih: Option<f64>,
    pub vil: Option<f64>,
    pub voh: Option<f64>,
    pub vol: Option<f64>,
    pub vin_max: Option<f64>,
    pub vin_max_unpowered: Option<f64>,
    pub when: Option<String>,
    /// The currents `vol` and `voh` are given at, amps.
    pub iol: Option<f64>,
    pub ioh: Option<f64>,
    /// What the pin draws as an input, amps; None when its package doesn't say.
    pub iin: Option<f64>,
    /// Its package's resistors from the pin to a rail (ohms, the rail's highest volts) and to
    /// ground (ohms).
    pub pull_up: Vec<(f64, f64)>,
    pub pull_down: Vec<f64>,
    /// The supply rail: its net and range.
    pub rail: Option<(String, (f64, f64))>,
    /// What the pin's function is called, for messages.
    pub what: String,
}

impl Lv {
    /// The highest an output drives: its rail's top, or its `voh`.
    pub fn high_max(&self) -> Option<f64> {
        self.rail.as_ref().map(|r| r.1 .1).or(self.voh)
    }
}

/// The levels of an instance's port (`imu.SDA`), where its package says them. None when it says
/// nothing about levels.
pub fn of(comp: &Composition, pkgs: &BTreeMap<String, Package>, rails: &Rails, inst: &str, port: &str) -> Option<Lv> {
    let pk = pkgs.get(inst)?;
    let variant = pk.ports.variants.get(&comp.instances.get(inst)?.variant)?;
    // The chip's pin in the port (a recipe's port is its own pin).
    let p = pk.circuit.ports.get(port)?;
    let pin = p.pins.iter().find_map(|x| x.strip_prefix("U1.")).and_then(|n| variant.pins.get(n)).or_else(|| variant.pins.get(port))?;
    let fname = pin.function.clone()?;
    let f = pk.ports.functions.get(&fname)?;
    let levels = f.get("levels")?;
    // The supply rail: the net of a port holding a pin of the supply function.
    let rail = f.get("supply").and_then(Value::as_str).and_then(|s| {
        variant.pins.iter().filter(|(_, x)| x.function.as_deref() == Some(s)).find_map(|(n, x)| {
            let port = port_of(&pk.circuit, n, &x.name)?;
            let net = net_of(comp, &format!("{inst}.{port}"))?;
            rails.get(&net).map(|r| (net, *r))
        })
    });
    // Levels written for one supply voltage say nothing at another.
    if let (Some(at), Some((_, (lo, hi)))) = (levels.get("at_supply_v").and_then(Value::as_f64), &rail) {
        if (lo - at).abs() > 0.05 * at || (hi - at).abs() > 0.05 * at {
            return None;
        }
    }
    let eval = |k: &str, worst_high: bool| -> Option<f64> {
        let e = match levels.get(k)? {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            _ => return None,
        };
        let at = |v: f64| compose::expr::eval(&e, &|n| (n == "supply").then_some(v)).ok();
        match &rail {
            Some((_, (lo, hi))) => {
                let (a, b) = (at(*lo)?, at(*hi)?);
                Some(if worst_high { a.max(b) } else { a.min(b) })
            }
            // Without a rail only a constant can be judged.
            None => at(f64::NAN).filter(|x| x.is_finite()),
        }
    };
    let (pull_up, pull_down) = pulls(comp, pk, rails, inst, port);
    Some(Lv {
        iol: eval("iol_ma", false).map(|x| x * 1e-3),
        ioh: eval("ioh_ma", false).map(|x| x * 1e-3),
        iin: eval("iin_ua", true).map(|x| x * 1e-6),
        pull_up,
        pull_down,
        vih: eval("vih", true),
        vil: eval("vil", false),
        voh: eval("voh", false),
        vol: eval("vol", true),
        vin_max: eval("vin_max", false),
        vin_max_unpowered: eval("vin_max_unpowered", false),
        when: levels.get("when").and_then(Value::as_str).map(str::to_string),
        rail,
        what: format!("{inst}.{port} ({fname})"),
    })
}

/// A resistor inside an instance from one of its ports to another: its role, ohms, and the far
/// port's name and kind.
pub fn resistors<'a>(comp: &Composition, pk: &'a Package, inst: &str, port: &str) -> Vec<(String, f64, &'a str, &'a str)> {
    let Some(i) = comp.instances.get(inst) else { return vec![] };
    let Some(p) = pk.circuit.ports.get(port) else { return vec![] };
    let mut out = Vec::new();
    for pin in &p.pins {
        let Some((role, n)) = pin.split_once('.') else { continue };
        let Some(part) = pk.circuit.parts.get(role) else { continue };
        if role == "U1" || i.omit.iter().any(|o| o == role) || part.symbol != "Device:R" {
            continue;
        }
        let other = format!("{role}.{}", if n == "1" { "2" } else { "1" });
        let Some((far, q)) = pk.circuit.ports.iter().find(|(_, q)| q.pins.contains(&other)) else { continue };
        let value = i.parts.get(role).and_then(|m| m.get("value")).and_then(Value::as_str).unwrap_or(&part.value);
        if let Some(ohms) = compose::expr::parse_value(value) {
            out.push((role.to_string(), ohms, far.as_str(), q.kind.as_str()));
        }
    }
    out
}

/// The pull-ups (ohms, the rail's top) and pull-downs (ohms) a package puts on a port.
fn pulls(comp: &Composition, pk: &Package, rails: &Rails, inst: &str, port: &str) -> (Vec<(f64, f64)>, Vec<f64>) {
    let (mut up, mut down) = (Vec::new(), Vec::new());
    for (_, ohms, far, kind) in resistors(comp, pk, inst, port) {
        match kind {
            "ground" => down.push(ohms),
            "supply" => {
                if let Some(v) = net_of(comp, &format!("{inst}.{far}")).and_then(|n| rails.get(&n)) {
                    up.push((ohms, v.1));
                }
            }
            _ => {}
        }
    }
    (up, down)
}

/// Whether `d` driving `r` works: Err with the reason when it doesn't, else what to keep in mind.
/// An open-drain driver's high is its pull-up's, judged by the bus rule, not here.
pub fn drive(d: &Lv, r: &Lv, open_drain: bool) -> Result<Vec<String>, String> {
    let mut notes = Vec::new();
    // What the driver sources high and sinks low, where the receiver says what it draws.
    let load_high = r.iin.map(|i| i + d.high_max().map_or(0.0, |v| r.pull_down.iter().map(|ohms| v / ohms).sum::<f64>()));
    let load_low = r.iin.map(|i| i + r.pull_up.iter().map(|(ohms, v)| v / ohms).sum::<f64>());
    let voh = match (d.voh, d.ioh, load_high, &d.rail) {
        (Some(voh), Some(ioh), Some(i), Some((_, (lo, _)))) if i <= ioh => Some(lo - (lo - voh) * i / ioh),
        _ => d.voh,
    };
    let vol = match (d.vol, d.iol, load_low) {
        (Some(vol), Some(iol), Some(i)) if i <= iol => Some(vol * i / iol),
        _ => d.vol,
    };
    if !open_drain {
        if let (Some(voh), Some(vih)) = (voh, r.vih) {
            if voh < vih {
                return Err(format!("{}'s high ({voh:.2} V) is below what {} reads as high ({vih:.2} V)", d.what, r.what));
            }
        }
        if let (Some(h), Some(max)) = (d.high_max(), r.vin_max) {
            if h > max + 1e-9 {
                return Err(format!("{}'s high ({h:.2} V) is above what {} takes ({max:.2} V)", d.what, r.what));
            }
            if let Some(u) = r.vin_max_unpowered.filter(|u| h > *u + 1e-9) {
                notes.push(format!("{}'s high ({h:.2} V) is above {u:.2} V, which {} takes only {}", d.what, r.what, r.when.as_deref().unwrap_or("while powered")));
            }
        }
    }
    if let (Some(vol), Some(vil)) = (vol, r.vil) {
        if vol > vil {
            let at = load_low.filter(|_| vol != d.vol.unwrap_or(vol)).map(|i| format!(" at {:.2} mA", i * 1e3)).unwrap_or_default();
            return Err(format!("{}'s low ({vol:.2} V{at}) is above what {} reads as low ({vil:.2} V)", d.what, r.what));
        }
    }
    Ok(notes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gpio() -> Lv {
        Lv { voh: Some(2.62), vol: Some(0.5), iol: Some(2e-3), ioh: Some(2e-3), rail: Some(("+3V3".into(), (3.3, 3.3))), what: "mcu.GPIO0".into(), ..Default::default() }
    }

    fn on(iin: Option<f64>) -> Lv {
        Lv { vih: Some(1.1), vil: Some(0.4), iin, what: "sw.ON".into(), ..Default::default() }
    }

    #[test]
    fn a_low_scaled_to_what_the_input_draws() {
        // 0.5 V at 2 mA is at most 0.25 mV at 1 uA: well under VIL 0.4 V.
        assert!(drive(&gpio(), &on(Some(1e-6)), false).is_ok());
        // An input that doesn't say what it draws is judged at the datasheet's 0.5 V.
        let e = drive(&gpio(), &on(None), false).unwrap_err();
        assert!(e.contains("0.50 V"), "{e}");
        // A 1k pull-up to 3.3 V on the input's side is 3.3 mA: past the 2 mA VOL is given at, so
        // nothing is scaled.
        let mut r = on(Some(1e-6));
        r.pull_up = vec![(1000.0, 3.3)];
        assert!(drive(&gpio(), &r, false).is_err());
        // A 4.7k one is 0.7 mA: 0.18 V.
        r.pull_up = vec![(4700.0, 3.3)];
        assert!(drive(&gpio(), &r, false).is_ok());
    }
}
