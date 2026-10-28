//! The composer: a board as package instances, the board's own parts, and the nets that join
//! them.
//!
//! A package's circuit (`schematic/circuit.VARIANT.json`) names its parts by role, its internal
//! nets by role pin, and its ports. A composition connects ports (`mcu.+3V3`) and the board's
//! own part pins (`J1.3`) into named nets. Composing gives every part a designator, names each
//! instance's internal nets `/INSTANCE/NET` as a KiCad hierarchical sheet would, and refuses a
//! ground or supply port left unconnected, or a pin put on two nets.
//!
//! Two departures from a package are allowed, and reported: a board net may name an internal
//! net (`mcu.DVDD`, a test point on the core supply) as a tap; and an instance may join internal
//! nets that an omitted link part separated (`join: [["QSPI_SS", "FLASH_SS"]]`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Deserialize)]
pub struct Composition {
    pub board: String,
    #[serde(default)]
    pub note: Option<String>,
    pub instances: BTreeMap<String, Instance>,
    /// The board's own parts by designator: connectors, regulators not in a package, holes.
    #[serde(default)]
    pub parts: BTreeMap<String, PartSpec>,
    /// Net name → endpoints: `INSTANCE.PORT` or `REF.PIN`.
    pub nets: BTreeMap<String, Vec<String>>,
    /// Signal ports deliberately left open (`mcu.GPIO7`); others are listed as unconnected.
    #[serde(default)]
    pub no_connect: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Instance {
    /// `name@version`, as in a binding.
    pub package: String,
    pub variant: String,
    /// Roles not fitted on this board.
    #[serde(default)]
    pub omit: Vec<String>,
    /// Roles whose part differs from the package's (another value, footprint or LCSC number).
    #[serde(default)]
    pub parts: BTreeMap<String, BTreeMap<String, Value>>,
    /// Internal nets made one, named by the first: where an omitted part linked them.
    #[serde(default)]
    pub join: Vec<Vec<String>>,
    /// Two-pin parts fitted the other way round (pins 1 and 2 swapped).
    #[serde(default)]
    pub reversed: Vec<String>,
    /// Designators to keep, by role: an existing board recomposed keeps its annotation.
    #[serde(default)]
    pub designators: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartSpec {
    pub symbol: String,
    pub value: String,
    pub footprint: String,
    /// Anything else the schematic carries (LCSC, MPN).
    #[serde(flatten)]
    pub fields: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Circuit {
    pub variant: String,
    pub parts: BTreeMap<String, CircuitPart>,
    pub nets: BTreeMap<String, Vec<String>>,
    pub ports: BTreeMap<String, Port>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CircuitPart {
    #[serde(default)]
    pub block: Option<String>,
    pub symbol: String,
    pub value: String,
    pub footprint: String,
    /// The designator on the reference board; its letters prefix the composed one.
    pub reference_ref: String,
    /// Fields the schematic carries (`lcsc`, `mpn`); the rest of the record is for people.
    #[serde(default)]
    pub lcsc: Option<String>,
    #[serde(default)]
    pub mpn: Option<String>,
    /// A placement hint: the role pin this part should sit next to (`U1.1`).
    #[serde(default)]
    pub near: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Port {
    pub kind: String,
    pub pins: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ComposedPart {
    pub symbol: String,
    pub value: String,
    pub footprint: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block: Option<String>,
    /// The board pin this part should sit next to (`U1.1`): a placement hint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub near: Option<String>,
    #[serde(flatten)]
    pub fields: BTreeMap<String, Value>,
}

/// What `ccomp check package` reads to judge one instance.
#[derive(Debug, Clone, Serialize)]
pub struct Binding {
    pub package: String,
    pub variant: String,
    pub chip: String,
    pub parts: BTreeMap<String, String>,
    pub note: String,
}

#[derive(Debug, Default, Serialize)]
pub struct Composed {
    pub ok: bool,
    pub board: String,
    pub parts: BTreeMap<String, ComposedPart>,
    pub nets: BTreeMap<String, Vec<String>>,
    /// Signal ports nothing connects to, as `INSTANCE.PORT`.
    pub unconnected_ports: Vec<String>,
    /// Internal nets a board net reaches into, as `INSTANCE.NET` → board net.
    pub taps: BTreeMap<String, String>,
    /// Internal nets an instance joins, as `INSTANCE.NET` → the net they became.
    pub joined: BTreeMap<String, String>,
    pub bindings: BTreeMap<String, Binding>,
    pub errors: Vec<String>,
}

/// The package folder and its version, from `PACKAGES/NAME/manifest.toml`.
fn package_dir(packages: &Path, spec: &str) -> Result<PathBuf, String> {
    let (name, version) = spec.split_once('@').ok_or_else(|| format!("{spec}: expected name@version"))?;
    let dir = packages.join(name);
    let manifest = std::fs::read_to_string(dir.join("manifest.toml")).map_err(|e| format!("{}: {e}", dir.display()))?;
    let found = manifest.lines().map(str::trim).find_map(|l| l.strip_prefix("version = \"").and_then(|v| v.strip_suffix('"'))).ok_or_else(|| format!("{}: no version", dir.display()))?;
    if found != version {
        return Err(format!("{spec}: the package in {} is {found}", dir.display()));
    }
    Ok(dir)
}

pub fn load_circuit(packages: &Path, inst: &Instance) -> Result<Circuit, String> {
    let p = package_dir(packages, &inst.package)?.join(format!("schematic/circuit.{}.json", inst.variant));
    let text = std::fs::read_to_string(&p).map_err(|e| format!("{}: {e} (no circuit for variant {})", p.display(), inst.variant))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", p.display()))
}

fn prefix(r: &str) -> String {
    r.chars().take_while(|c| c.is_ascii_alphabetic()).collect()
}

/// Compose with each instance's circuit already loaded (`circuits` by instance name).
pub fn compose_with(c: &Composition, circuits: &BTreeMap<String, Circuit>) -> Composed {
    let mut out = Composed { board: c.board.clone(), ..Default::default() };
    let mut next: BTreeMap<String, u32> = BTreeMap::new();
    let mut taken: std::collections::BTreeSet<String> = c.parts.keys().cloned().collect();
    for (name, inst) in &c.instances {
        for (role, d) in &inst.designators {
            if !taken.insert(d.clone()) {
                out.errors.push(format!("{name}: {role} keeps designator {d}, which is already taken"));
            }
        }
    }
    for (r, p) in &c.parts {
        out.parts.insert(r.clone(), ComposedPart { symbol: p.symbol.clone(), value: p.value.clone(), footprint: p.footprint.clone(), instance: None, role: None, block: None, near: None, fields: p.fields.clone() });
    }

    // Designators: each instance's parts in role order, after the board's own.
    let mut refs: BTreeMap<(String, String), String> = BTreeMap::new();
    for (name, inst) in &c.instances {
        let circ = &circuits[name];
        for o in &inst.omit {
            if !circ.parts.contains_key(o) {
                out.errors.push(format!("{name}: omits {o}, which the package does not have"));
            }
        }
        for r in inst.parts.keys().filter(|r| !circ.parts.contains_key(*r)) {
            out.errors.push(format!("{name}: changes part {r}, which the package does not have"));
        }
        for (role, part) in circ.parts.iter().filter(|(r, _)| !inst.omit.contains(r)) {
            let des = match inst.designators.get(role) {
                Some(d) => d.clone(),
                None => {
                    let pre = prefix(&part.reference_ref);
                    let n = next.entry(pre.clone()).or_insert(0);
                    let d = loop {
                        *n += 1;
                        let d = format!("{pre}{n}");
                        if !taken.contains(&d) {
                            break d;
                        }
                    };
                    taken.insert(d.clone());
                    d
                }
            };
            let mut fields = BTreeMap::new();
            for (k, v) in [("lcsc", &part.lcsc), ("mpn", &part.mpn)] {
                if let Some(v) = v {
                    fields.insert(k.to_string(), Value::String(v.clone()));
                }
            }
            let mut cp = ComposedPart { symbol: part.symbol.clone(), value: part.value.clone(), footprint: part.footprint.clone(), instance: Some(name.clone()), role: Some(role.clone()), block: part.block.clone(), near: part.near.clone(), fields };
            for (k, v) in inst.parts.get(role).into_iter().flatten() {
                match (k.as_str(), v.as_str()) {
                    ("value", Some(s)) => cp.value = s.to_string(),
                    ("footprint", Some(s)) => cp.footprint = s.to_string(),
                    ("symbol", Some(s)) => cp.symbol = s.to_string(),
                    _ => {
                        cp.fields.insert(k.clone(), v.clone());
                    }
                }
            }
            out.parts.insert(des.clone(), cp);
            refs.insert((name.clone(), role.clone()), des);
        }
        let chip = refs.get(&(name.clone(), "U1".to_string())).cloned().unwrap_or_default();
        let parts = refs.iter().filter(|((i, r), _)| i == name && r != "U1").map(|((_, r), d)| (r.clone(), d.clone())).collect();
        out.bindings.insert(name.clone(), Binding { package: inst.package.clone(), variant: circ.variant.clone(), chip, parts, note: format!("Written by ccomp compose for instance {name} of board {}.", c.board) });
    }

    for (name, inst) in &c.instances {
        for r in inst.reversed.iter().filter(|r| !circuits[name].parts.contains_key(*r)) {
            out.errors.push(format!("{name}: reverses {r}, which the package does not have"));
        }
    }
    // Placement hints name role pins; on the board they are designator pins.
    for p in out.parts.values_mut() {
        if let (Some(inst), Some(near)) = (p.instance.clone(), p.near.take()) {
            p.near = near.split_once('.').and_then(|(role, pin)| refs.get(&(inst.clone(), role.to_string())).map(|d| format!("{d}.{pin}")));
        }
    }
    // A role pin as a board pin; None when its part is omitted.
    let pin = |inst: &str, rp: &str| -> Option<String> {
        let (role, p) = rp.split_once('.')?;
        let p = match (c.instances[inst].reversed.iter().any(|r| r == role), p) {
            (true, "1") => "2",
            (true, "2") => "1",
            _ => p,
        };
        refs.get(&(inst.to_string(), role.to_string())).map(|d| format!("{d}.{p}"))
    };
    let mut on: BTreeMap<String, String> = BTreeMap::new();
    let mut put = |out: &mut Composed, net: &str, pins: Vec<String>| {
        for p in pins {
            match on.get(&p) {
                Some(other) if other != net => out.errors.push(format!("{p} is on both {other} and {net}")),
                Some(_) => {}
                None => {
                    on.insert(p.clone(), net.to_string());
                    out.nets.entry(net.to_string()).or_default().push(p);
                }
            }
        }
    };

    // Taps: a board net that names an internal net takes it over.
    let mut internal_name: BTreeMap<(String, String), String> = BTreeMap::new();
    for (name, inst) in &c.instances {
        let circ = &circuits[name];
        for group in &inst.join {
            for n in group.iter().filter(|n| !circ.nets.contains_key(*n)) {
                out.errors.push(format!("{name}: joins {n}, which is not an internal net"));
            }
            if let Some(first) = group.first() {
                for n in group {
                    internal_name.insert((name.clone(), n.clone()), format!("/{name}/{first}"));
                    if n != first {
                        out.joined.insert(format!("{name}.{n}"), format!("/{name}/{first}"));
                    }
                }
            }
        }
    }
    for (net, ends) in &c.nets {
        for e in ends {
            if let Some((head, tail)) = e.split_once('.') {
                if circuits.get(head).is_some_and(|c| !c.ports.contains_key(tail) && c.nets.contains_key(tail)) {
                    // Every net joined with the tapped one goes with it.
                    let was = internal_name.get(&(head.to_string(), tail.to_string())).cloned();
                    for ((i, _), v) in internal_name.iter_mut() {
                        if i == head && was.as_ref() == Some(v) {
                            *v = net.clone();
                        }
                    }
                    internal_name.insert((head.to_string(), tail.to_string()), net.clone());
                    out.taps.insert(e.clone(), net.clone());
                }
            }
        }
    }
    for name in c.instances.keys() {
        let circ = &circuits[name];
        for (net, pins) in &circ.nets {
            let ps: Vec<String> = pins.iter().filter_map(|p| pin(name, p)).collect();
            let to = internal_name.get(&(name.clone(), net.clone())).cloned().unwrap_or_else(|| format!("/{name}/{net}"));
            put(&mut out, &to, ps);
        }
    }
    let mut used_ports: std::collections::BTreeSet<(String, String)> = std::collections::BTreeSet::new();
    for (net, ends) in &c.nets {
        let mut ps = Vec::new();
        for e in ends {
            let Some((head, tail)) = e.split_once('.') else {
                out.errors.push(format!("{net}: {e} is neither INSTANCE.PORT nor REF.PIN"));
                continue;
            };
            if let Some(circ) = circuits.get(head) {
                match circ.ports.get(tail) {
                    Some(port) => {
                        if !used_ports.insert((head.to_string(), tail.to_string())) {
                            out.errors.push(format!("{e} is named twice"));
                        }
                        ps.extend(port.pins.iter().filter_map(|p| pin(head, p)));
                    }
                    // A tap: its pins are on this net already.
                    None if circ.nets.contains_key(tail) => {}
                    None => out.errors.push(format!("{net}: {head} has no port or net {tail}")),
                }
            } else if c.parts.contains_key(head) {
                ps.push(e.clone());
            } else {
                out.errors.push(format!("{net}: {head} is neither an instance nor a part"));
            }
        }
        put(&mut out, net, ps);
    }
    // Ports nothing names: power must be connected; a signal port keeps its pins together.
    for name in c.instances.keys() {
        for (pname, port) in &circuits[name].ports {
            if used_ports.contains(&(name.clone(), pname.clone())) {
                continue;
            }
            let id = format!("{name}.{pname}");
            if port.kind != "signal" {
                out.errors.push(format!("{id}: {} port not connected", port.kind));
                continue;
            }
            let ps: Vec<String> = port.pins.iter().filter_map(|p| pin(name, p)).collect();
            if ps.len() > 1 {
                put(&mut out, &format!("/{name}/{pname}"), ps);
            }
            if !c.no_connect.contains(&id) {
                out.unconnected_ports.push(id);
            }
        }
    }
    for nc in &c.no_connect {
        let known = nc.split_once('.').is_some_and(|(i, p)| circuits.get(i).is_some_and(|c| c.ports.contains_key(p)));
        if !known {
            out.errors.push(format!("no_connect {nc}: no such port"));
        }
    }
    out.nets.retain(|_, ps| !ps.is_empty());
    for ps in out.nets.values_mut() {
        ps.sort_by_key(|p| natural(p));
    }
    out.ok = out.errors.is_empty();
    out
}

/// `C10.2` after `C9.2`.
fn natural(s: &str) -> (String, u64, String) {
    let (r, p) = s.split_once('.').unwrap_or((s, ""));
    let pre = prefix(r);
    (pre.clone(), r[pre.len()..].parse().unwrap_or(0), p.to_string())
}

/// Load each instance's package from `packages` and compose.
pub fn compose(c: &Composition, packages: &Path) -> Result<Composed, String> {
    let mut circuits = BTreeMap::new();
    for (name, inst) in &c.instances {
        circuits.insert(name.clone(), load_circuit(packages, inst)?);
    }
    Ok(compose_with(c, &circuits))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn circuit() -> Circuit {
        serde_json::from_value(json!({
            "variant": "V",
            "parts": {
                "U1": { "symbol": "s:U", "value": "chip", "footprint": "f", "reference_ref": "U1" },
                "C1": { "symbol": "Device:C", "value": "100n", "footprint": "f", "reference_ref": "C7" },
                "R1": { "symbol": "Device:R", "value": "27", "footprint": "f", "reference_ref": "R3" }
            },
            "nets": { "PIN": ["U1.2", "R1.2"] },
            "ports": {
                "GND": { "kind": "ground", "pins": ["U1.1", "C1.2"] },
                "VDD": { "kind": "supply", "pins": ["U1.3", "C1.1"] },
                "IO": { "kind": "signal", "pins": ["R1.1"] },
                "SPARE": { "kind": "signal", "pins": ["U1.4"] }
            }
        }))
        .unwrap()
    }

    fn comp(nets: Value) -> Composition {
        serde_json::from_value(json!({
            "board": "b",
            "instances": { "a": { "package": "p@1", "variant": "V" }, "b": { "package": "p@1", "variant": "V" } },
            "parts": { "J1": { "symbol": "Conn:J", "value": "J", "footprint": "f" }, "C1": { "symbol": "Device:C", "value": "10u", "footprint": "f" } },
            "nets": nets,
            "no_connect": ["a.SPARE"]
        }))
        .unwrap()
    }

    fn circuits() -> BTreeMap<String, Circuit> {
        [("a".to_string(), circuit()), ("b".to_string(), circuit())].into()
    }

    #[test]
    fn two_instances_share_rails_and_keep_their_own_nets() {
        let c = comp(json!({ "GND": ["a.GND", "b.GND", "J1.2", "C1.2"], "+3V3": ["a.VDD", "b.VDD", "J1.1", "C1.1"], "IO_A": ["a.IO", "J1.3"] }));
        let r = compose_with(&c, &circuits());
        assert!(r.ok, "{:?}", r.errors);
        // The board's C1 keeps its designator; the instances' capacitors are numbered after it.
        assert_eq!(r.bindings["a"].parts["C1"], "C2");
        assert_eq!(r.bindings["b"].parts["C1"], "C3");
        assert_eq!(r.bindings["a"].chip, "U1");
        assert_eq!(r.bindings["b"].chip, "U2");
        assert_eq!(r.nets["GND"], ["C1.2", "C2.2", "C3.2", "J1.2", "U1.1", "U2.1"]);
        assert_eq!(r.nets["/a/PIN"], ["R1.2", "U1.2"]);
        assert_eq!(r.nets["/b/PIN"], ["R2.2", "U2.2"]);
        // b's IO and SPARE are open and not declared so.
        assert_eq!(r.unconnected_ports, ["b.IO", "b.SPARE"]);
    }

    #[test]
    fn power_must_be_connected_and_pins_not_shorted() {
        let c = comp(json!({ "GND": ["a.GND", "b.GND"], "+3V3": ["a.VDD"], "X": ["J1.1", "a.IO"], "Y": ["J1.1"] }));
        let r = compose_with(&c, &circuits());
        assert!(!r.ok);
        let all = r.errors.join("\n");
        assert!(all.contains("b.VDD: supply port not connected"), "{all}");
        assert!(all.contains("J1.1 is on both X and Y"), "{all}");
    }

    #[test]
    fn taps_and_joins() {
        let mut circs = circuits();
        for c in circs.values_mut() {
            c.nets.insert("SEL".into(), vec!["U1.5".into()]);
        }
        let mut c = comp(json!({ "GND": ["a.GND", "b.GND"], "+3V3": ["a.VDD", "b.VDD"], "TP": ["a.PIN", "J1.4"] }));
        c.instances.get_mut("a").unwrap().join.push(vec!["PIN".into(), "SEL".into()]);
        let r = compose_with(&c, &circs);
        assert!(r.ok, "{:?}", r.errors);
        // a's PIN and SEL are one net, and the board's test point takes it over by name.
        assert_eq!(r.nets["TP"], ["J1.4", "R1.2", "U1.2", "U1.5"]);
        assert!(!r.nets.contains_key("/a/PIN") && !r.nets.contains_key("/a/SEL"));
        assert_eq!(r.taps["a.PIN"], "TP");
        assert_eq!(r.joined["a.SEL"], "/a/PIN");
        assert_eq!(r.nets["/b/PIN"], ["R2.2", "U2.2"]);
    }

    #[test]
    fn omitted_and_changed_parts() {
        let mut c = comp(json!({ "GND": ["a.GND", "b.GND"], "+3V3": ["a.VDD", "b.VDD"] }));
        let a = c.instances.get_mut("a").unwrap();
        a.omit.push("R1".into());
        a.parts.insert("C1".into(), [("value".to_string(), json!("1u")), ("lcsc".to_string(), json!("C52923"))].into());
        let r = compose_with(&c, &circuits());
        assert!(r.ok, "{:?}", r.errors);
        assert!(!r.bindings["a"].parts.contains_key("R1"));
        assert_eq!(r.nets["/a/PIN"], ["U1.2"]);
        let c2 = &r.parts[&r.bindings["a"].parts["C1"]];
        assert_eq!((c2.value.as_str(), c2.fields["lcsc"].as_str()), ("1u", Some("C52923")));
        // a omits its resistor, so b's is R1; fitted the other way round, its pin 1 is on PIN.
        c.instances.get_mut("b").unwrap().reversed.push("R1".into());
        let r = compose_with(&c, &circuits());
        assert_eq!(r.nets["/b/PIN"], ["R1.1", "U2.2"]);
        // Kept designators are taken first; the rest are numbered round them.
        c.instances.get_mut("b").unwrap().designators.insert("U1".into(), "U1".into());
        let r = compose_with(&c, &circuits());
        assert_eq!((r.bindings["a"].chip.as_str(), r.bindings["b"].chip.as_str()), ("U2", "U1"));
        c.instances.get_mut("a").unwrap().designators.insert("C1".into(), "J1".into());
        assert!(compose_with(&c, &circuits()).errors.iter().any(|e| e.contains("J1, which is already taken")));
    }
}
