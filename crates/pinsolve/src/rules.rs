//! Interface rules: what a solved connection implies beyond its wires.
//!
//! **I2C pull-ups.** Each bus needs one pull-up per line to its rail, between
//! `Rp(min) = (V - VOL) / IOL` (the weakest device must still pull the line low) and
//! `Rp(max) = tr / (0.8473 Cb)` (the line must rise in time), with VOL, IOL and tr for the bus's
//! slowest device's mode and Cb estimated from the devices on it (TI SLVA689A; the numbers are
//! the `i2c` type's `rules` in `library/interfaces.json`). Pull-ups a package brings (a resistor
//! from its bus pin to one of its supply ports) count. With none, the rule adds the
//! `i2c_pullups` recipe; with several packages' pairs in parallel below Rp(min), it omits all but
//! one pair.

use std::collections::BTreeMap;

use compose::Composition;
use interfaces::Types;
use serde_json::{json, Value};

use crate::levels::{net_of, Rails};
use crate::Package;

/// One solved I2C bus.
pub struct Bus {
    /// The controller instance and its interface (`mcu`, `I2C0`).
    pub partner: String,
    pub iface: String,
    /// The type's signal (`SDA`, `SCL`) → its net.
    pub nets: BTreeMap<String, String>,
    /// Each target: its instance, its interface, and its port on each type signal.
    pub targets: Vec<(String, String, BTreeMap<String, String>)>,
    /// The bus capacitance a connection gave, pF per line.
    pub bus_pf: Option<f64>,
}

/// A pull-up a package brings: its role, its resistance and the net its top is on.
struct Pullup {
    inst: String,
    role: String,
    ohms: f64,
    rail: Option<String>,
}

fn num(v: &Value, k: &str) -> Option<f64> {
    v.get(k).and_then(Value::as_f64)
}

/// The resistors inside an instance from `port` to one of its supply ports.
fn pullups(comp: &Composition, pk: &Package, inst: &str, port: &str) -> Vec<Pullup> {
    crate::levels::resistors(comp, pk, inst, port)
        .into_iter()
        .filter(|r| r.3 == "supply")
        .map(|(role, ohms, far, _)| Pullup { inst: inst.to_string(), role, ohms, rail: net_of(comp, &format!("{inst}.{far}")) })
        .collect()
}

fn parallel(rs: &[f64]) -> f64 {
    1.0 / rs.iter().map(|r| 1.0 / r).sum::<f64>()
}

fn ohms(r: f64) -> String {
    // Three figures: 967 ohm, 8.85k ohm.
    let mag = 10f64.powf(r.abs().log10().floor() - 2.0);
    format!("{} ohm", compose::expr::format_value((r / mag).round() * mag))
}

/// Apply the I2C rule to each bus: `out` is the solved composition, changed in place.
#[allow(clippy::too_many_arguments)]
pub fn i2c(buses: &[Bus], out: &mut Value, comp: &Composition, pkgs: &BTreeMap<String, Package>, types: &Types, rails: &Rails, warnings: &mut Vec<String>, errors: &mut Vec<String>, added: &mut Vec<String>) {
    let Some(rule) = types.types.get("i2c").and_then(|t| t.rules.as_ref()).and_then(|r| r.get("pullup")).cloned() else {
        if !buses.is_empty() {
            warnings.push("library/interfaces.json gives no i2c pull-up rule: pull-ups not checked".into());
        }
        return;
    };
    for bus in buses {
        let name = format!("{}.{}", bus.partner, bus.iface);
        // The slowest device sets the mode.
        let mut khz = f64::INFINITY;
        let mut devices = vec![(bus.partner.clone(), bus.iface.clone())];
        devices.extend(bus.targets.iter().map(|t| (t.0.clone(), t.1.clone())));
        for (inst, iface) in &devices {
            if let Some(k) = pkgs.get(inst).and_then(|p| p.ports.interfaces.get(iface)).and_then(|i| i.extra.get("max_khz")).and_then(Value::as_f64) {
                khz = khz.min(k);
            }
        }
        let modes = rule["modes"].as_array().cloned().unwrap_or_default();
        let Some(mode) = modes.iter().find(|m| num(m, "max_khz").is_some_and(|k| k >= khz)).or(modes.first()) else { continue };
        let tr = num(mode, "tr_ns").unwrap_or(1000.0) * 1e-9;
        let vol = num(mode, "vol_v").unwrap_or(0.4);
        let iol = num(mode, "iol_ma").unwrap_or(3.0) * 1e-3;
        let cb_max = num(mode, "cb_max_pf").unwrap_or(400.0);
        let cb = bus.bus_pf.unwrap_or(num(&rule, "pin_pf").unwrap_or(10.0) * devices.len() as f64 + num(&rule, "wiring_pf").unwrap_or(20.0));
        if cb > cb_max {
            errors.push(format!("{name}: {cb:.0} pF per line is more than {} mode allows ({cb_max:.0} pF)", mode["name"].as_str().unwrap_or("its")));
            continue;
        }
        // The rail: the targets' I/O supply (where their pull-ups must go).
        let target_rails: Vec<String> = bus
            .targets
            .iter()
            .filter_map(|(inst, _, ports)| ports.values().next().and_then(|p| crate::levels::of(comp, pkgs, rails, inst, p)).and_then(|l| l.rail.map(|r| r.0)))
            .collect();
        let rail = target_rails.first().cloned();
        if target_rails.iter().any(|r| Some(r) != rail.as_ref()) {
            errors.push(format!("{name}: its devices' I/O rails differ ({}); a bus across rails needs a level shifter", target_rails.join(", ")));
            continue;
        }
        let Some(rail) = rail else {
            warnings.push(format!("{name}: no rail known for its devices' I/O; pull-ups not checked"));
            continue;
        };
        let Some(&(_, v)) = rails.get(&rail) else {
            warnings.push(format!("{name}: {rail}'s voltage isn't known (give it under rails); pull-ups not checked"));
            continue;
        };
        let rmin = (v - vol) / iol;
        let rmax = tr / (0.8473 * cb * 1e-12);
        let sda_scl: Vec<(String, String)> = bus.nets.iter().map(|(s, n)| (s.clone(), n.clone())).collect();
        // Pull-ups on each line, from every target that brings them.
        let mut by_line: BTreeMap<String, Vec<Pullup>> = BTreeMap::new();
        for (inst, _, ports) in &bus.targets {
            for (sig, port) in ports {
                by_line.entry(sig.clone()).or_default().extend(pullups(comp, &pkgs[inst], inst, port));
            }
        }
        let have = by_line.values().any(|v| !v.is_empty());
        for p in by_line.values().flatten() {
            if p.rail.as_deref() != Some(rail.as_str()) {
                errors.push(format!("{name}: {}'s {} pulls up to {}, not the bus's rail {rail}", p.inst, p.role, p.rail.as_deref().unwrap_or("an unknown net")));
            }
        }
        if !have {
            let target = 4700f64.clamp(rmin * 1.2, (rmax * 0.8).max(rmin * 1.2));
            let inst = format!("{}_pu", bus.iface.to_lowercase());
            let o = out.as_object_mut().unwrap();
            o["instances"][&inst] = json!({ "recipe": "i2c_pullups", "params": { "rmin_ohm": rmin.round(), "rmax_ohm": rmax.round(), "target_ohm": target.round() } });
            for (sig, net) in &sda_scl {
                if let Some(a) = o["nets"][net].as_array_mut() {
                    a.push(json!(format!("{inst}.{sig}")));
                }
            }
            if let Some(a) = o["nets"][&rail].as_array_mut() {
                a.push(json!(format!("{inst}.VDD")));
            }
            added.push(format!("{name}: pull-ups {inst} (i2c_pullups) to {rail}, {}..{} for {:.0} kHz and ~{cb:.0} pF", ohms(rmin), ohms(rmax), khz.min(num(mode, "max_khz").unwrap_or(khz))));
            continue;
        }
        // Several pairs in parallel: keep the first target's, omit the rest.
        let mut omitted: Vec<String> = Vec::new();
        for (sig, ps) in &by_line {
            let r = parallel(&ps.iter().map(|p| p.ohms).collect::<Vec<_>>());
            if r < rmin && ps.len() > 1 {
                let keep = ps[0].inst.clone();
                for p in ps.iter().filter(|p| p.inst != keep) {
                    let o = out.as_object_mut().unwrap();
                    let omit = o["instances"][&p.inst].as_object_mut().unwrap().entry("omit").or_insert(json!([]));
                    if let Some(a) = omit.as_array_mut() {
                        if !a.iter().any(|x| x == &json!(p.role)) {
                            a.push(json!(p.role));
                        }
                    }
                    omitted.push(format!("{}.{}", p.inst, p.role));
                }
                let r = ps[0].ohms;
                if r < rmin {
                    errors.push(format!("{name}: {sig}'s pull-up {} is below Rp(min) {}", ohms(r), ohms(rmin)));
                }
            } else if r < rmin {
                errors.push(format!("{name}: {sig}'s pull-up {} is below Rp(min) {} at {v:.2} V", ohms(r), ohms(rmin)));
            } else if r > rmax {
                warnings.push(format!("{name}: {sig}'s pull-up {} is above Rp(max) {} for ~{cb:.0} pF: the line rises too slowly at {:.0} kHz", ohms(r), ohms(rmax), khz));
            }
        }
        if !omitted.is_empty() {
            added.push(format!("{name}: one pull-up pair is enough; omitted {}", omitted.join(", ")));
        }
    }
}
