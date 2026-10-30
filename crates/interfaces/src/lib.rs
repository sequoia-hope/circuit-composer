//! Typed interfaces: what a pin solver needs to connect "the IMU's I2C to the MCU" without being
//! told pins.
//!
//! - An interface **type** (`library/interfaces.json`) is a bundle of signals named from the
//!   device's own side (`spi`: SCK, SDO, SDI, CS), with the roles it can take and how two
//!   devices' signals join (one's SDO to the other's SDI).
//! - A package's **interfaces** (`ports.json`, under `interfaces`) are instances of types: the
//!   LSM6DSO's `i2c` target on pins SDA and SCL, the RP2350's `I2C0`, `SPI1`, `PWM3`. An
//!   instance names its signals in the datasheet's words and maps them to the type's (`TX` is
//!   `SDO`).
//! - An instance's signals reach pins either fixed, by pin name (per variant if need be), or
//!   through the package's **mux table** (`ports.json`, under `mux`): for each GPIO, the function
//!   selects it has, as `[function, instance, signal]`.
//!
//! `Ports::options` answers which pins each signal of an instance can take on a variant, and
//! `check` says where the data disagrees with itself or with the types.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Types {
    #[serde(default)]
    pub note: Option<String>,
    pub types: BTreeMap<String, Type>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Type {
    /// The roles an instance can take (`controller`, `target`); none for a symmetric type.
    #[serde(default)]
    pub roles: Vec<String>,
    /// Many devices share one set of lines (I2C, SPI).
    #[serde(default)]
    pub bus: bool,
    pub signals: BTreeMap<String, SignalSpec>,
    /// Signals an instance or a connection may go without.
    #[serde(default)]
    pub optional: Vec<String>,
    /// On a bus, signals each target has its own of (SPI's CS).
    #[serde(default)]
    pub per_target: Vec<String>,
    /// Which signal of one device meets which of the other's.
    #[serde(default)]
    pub joins: Vec<(String, String)>,
    /// What a connection of this type implies (I2C's pull-ups), with the numbers and citations
    /// the pin solver's interface rules read.
    #[serde(default)]
    pub rules: Option<Value>,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct SignalSpec {
    /// The controller's direction: `in`, `out`, `bidir`.
    #[serde(default)]
    pub dir: Option<String>,
    /// `push_pull`, `open_drain`, `tristate`.
    #[serde(default)]
    pub drive: Option<String>,
}

/// One interface of a package.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Interface {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub roles: Vec<String>,
    /// The instance's own signal names (the datasheet's) to the type's; the same names when absent.
    #[serde(default)]
    pub signals: BTreeMap<String, String>,
    /// Its signals' pins by name, the same on every variant.
    #[serde(default)]
    pub pins: BTreeMap<String, String>,
    /// Its signals' pins by name, where they differ by variant.
    #[serde(default)]
    pub pins_by_variant: BTreeMap<String, BTreeMap<String, String>>,
    /// Its signals reach pins through the package's mux table, under the interface's name.
    #[serde(default)]
    pub mux: bool,
    /// Each pin it reaches is a separate one-signal interface (software I/O, an ADC channel).
    #[serde(default)]
    pub per_pin: bool,
    /// The types it can act as (a PIO block can be I2C, SPI, a UART...).
    #[serde(default)]
    pub implements: Vec<String>,
    /// Interfaces of this package it can't be used together with (the same pins, another mode).
    #[serde(default)]
    pub excludes: Vec<String>,
    #[serde(default)]
    pub dir: Option<String>,
    #[serde(default)]
    pub drive: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub citation: Option<Citation>,
    /// What interface rules read: `addresses`, `max_khz`, `requires`.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Citation {
    pub source: String,
    pub locator: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Mux {
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub citation: Option<Citation>,
    /// GPIO name → `[function, instance, signal]` for each function it can select.
    pub pins: BTreeMap<String, Vec<(String, String, String)>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Pin {
    pub name: String,
    /// The key in `functions` describing it.
    #[serde(default)]
    pub function: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Variant {
    pub pins: BTreeMap<String, Pin>,
}

/// The parts of a package's `ports.json` this crate reads.
#[derive(Debug, Clone, Deserialize)]
pub struct Ports {
    /// What each pin function is: its kind, supply and, where known, logic levels.
    #[serde(default)]
    pub functions: BTreeMap<String, Value>,
    pub variants: BTreeMap<String, Variant>,
    #[serde(default)]
    pub interfaces: BTreeMap<String, Interface>,
    #[serde(default)]
    pub mux: Option<Mux>,
}

/// A pin a signal can take, and the function select that puts it there.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PinChoice {
    pub number: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function: Option<String>,
}

impl Ports {
    /// A variant's pins called `name`, or `name` then `_` and more (GPIO26 is GPIO26_ADC0):
    /// (number, full name).
    pub fn find(&self, variant: &str, name: &str) -> Vec<(String, String)> {
        let Some(v) = self.variants.get(variant) else { return vec![] };
        v.pins.iter().filter(|(_, p)| p.name == name || p.name.strip_prefix(name).is_some_and(|r| r.starts_with('_'))).map(|(n, p)| (n.clone(), p.name.clone())).collect()
    }

    /// Where each of an interface's own signals can go on a variant. A signal with no entry
    /// can't go anywhere there (PWM8 on a QFN-60).
    pub fn options(&self, variant: &str, iface: &str) -> Result<BTreeMap<String, Vec<PinChoice>>, String> {
        if !self.variants.contains_key(variant) {
            return Err(format!("no variant {variant}: {}", self.variants.keys().cloned().collect::<Vec<_>>().join(", ")));
        }
        let i = self.interfaces.get(iface).ok_or_else(|| format!("no interface {iface}: {}", self.interfaces.keys().cloned().collect::<Vec<_>>().join(", ")))?;
        let mut out: BTreeMap<String, Vec<PinChoice>> = BTreeMap::new();
        let mut fixed = i.pins.clone();
        if let Some(m) = i.pins_by_variant.get(variant) {
            fixed.extend(m.clone());
        }
        for (sig, name) in fixed {
            for (number, name) in self.find(variant, &name) {
                out.entry(sig.clone()).or_default().push(PinChoice { number, name, function: None });
            }
        }
        if let (true, Some(m)) = (i.mux, &self.mux) {
            for (gpio, entries) in &m.pins {
                for (f, _, sig) in entries.iter().filter(|e| e.1 == iface) {
                    for (number, name) in self.find(variant, gpio) {
                        out.entry(sig.clone()).or_default().push(PinChoice { number, name, function: Some(f.clone()) });
                    }
                }
            }
        }
        for v in out.values_mut() {
            v.sort_by_key(|c| (c.number.parse::<u32>().unwrap_or(u32::MAX), c.number.clone()));
            v.dedup();
        }
        Ok(out)
    }
}

impl Interface {
    /// The type's signal an own signal is.
    pub fn type_signal<'a>(&'a self, own: &'a str) -> &'a str {
        self.signals.get(own).map(String::as_str).unwrap_or(own)
    }
}

pub fn load_types(path: &Path) -> Result<Types, String> {
    read(path)
}

pub fn load_ports(path: &Path) -> Result<Ports, String> {
    read(path)
}

fn read<T: for<'de> Deserialize<'de>>(p: &Path) -> Result<T, String> {
    let text = std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", p.display()))
}

/// What is wrong with one package's interfaces and mux table, against the types and the
/// package's cited sources (when given).
pub fn check(types: &Types, package: &str, ports: &Ports, sources: Option<&BTreeSet<String>>) -> Vec<String> {
    let mut errs = Vec::new();
    let mut e = |s: String| errs.push(format!("{package}: {s}"));
    let cited = |c: &Option<Citation>| c.as_ref().and_then(|c| sources.filter(|s| !s.contains(&c.source)).map(|_| c.source.clone()));
    for (name, i) in &ports.interfaces {
        let Some(t) = types.types.get(&i.kind) else {
            e(format!("{name}: no interface type {}", i.kind));
            continue;
        };
        if t.roles.is_empty() && !i.roles.is_empty() {
            e(format!("{name}: {} has no roles, but it names {:?}", i.kind, i.roles));
        }
        if !t.roles.is_empty() && i.roles.is_empty() {
            e(format!("{name}: an {} interface takes a role: {}", i.kind, t.roles.join(", ")));
        }
        for r in i.roles.iter().filter(|r| !t.roles.contains(r)) {
            e(format!("{name}: {} has no role {r}", i.kind));
        }
        for (own, ts) in &i.signals {
            if !t.signals.contains_key(ts) {
                e(format!("{name}: its {own} is {ts}, which {} has no signal of", i.kind));
            }
        }
        for x in i.implements.iter().filter(|x| !types.types.contains_key(*x)) {
            e(format!("{name}: implements {x}, no such type"));
        }
        for x in i.excludes.iter().filter(|x| !ports.interfaces.contains_key(*x)) {
            e(format!("{name}: excludes {x}, no such interface"));
        }
        for v in i.pins_by_variant.keys().filter(|v| !ports.variants.contains_key(*v)) {
            e(format!("{name}: pins for {v}, no such variant"));
        }
        if i.per_pin && t.signals.len() != 1 {
            e(format!("{name}: one interface per pin, but {} has {} signals", i.kind, t.signals.len()));
        }
        if let Some(s) = cited(&i.citation) {
            e(format!("{name}: cites {s}, not in sources.json"));
        }
        if i.mux && ports.mux.as_ref().is_none_or(|m| !m.pins.values().flatten().any(|x| &x.1 == name)) {
            e(format!("{name}: reached through the mux, but the mux table never names it"));
        }
        if !i.mux && i.pins.is_empty() && i.pins_by_variant.is_empty() {
            e(format!("{name}: no pins and not in the mux"));
        }
        // Fixed pins exist on every variant they're given for.
        for v in ports.variants.keys() {
            let mut fixed = i.pins.clone();
            if let Some(m) = i.pins_by_variant.get(v) {
                fixed.extend(m.clone());
            } else if !i.pins_by_variant.is_empty() && i.pins.is_empty() {
                continue;
            }
            for (sig, pin) in fixed {
                if ports.find(v, &pin).is_empty() {
                    e(format!("{name}: its {sig} is on {pin}, which {v} has no pin called"));
                }
            }
            // Where an interface reaches pins at all on a variant, every signal it must have does.
            let Ok(opts) = ports.options(v, name) else { continue };
            for own in opts.keys() {
                if !t.signals.contains_key(i.type_signal(own)) {
                    e(format!("{name}: its {own} is not a {} signal; map it under signals", i.kind));
                }
            }
            if !opts.is_empty() && !i.per_pin {
                let have: BTreeSet<&str> = opts.keys().map(|s| i.type_signal(s)).collect();
                for s in t.signals.keys().filter(|s| !t.optional.contains(s) && !have.contains(s.as_str())) {
                    e(format!("{name}: on {v} no pin carries its {s}"));
                }
            }
        }
    }
    if let Some(m) = &ports.mux {
        if let Some(s) = cited(&m.citation) {
            e(format!("mux: cites {s}, not in sources.json"));
        }
        for gpio in m.pins.keys() {
            if ports.variants.keys().all(|v| ports.find(v, gpio).is_empty()) {
                e(format!("mux: {gpio} is no variant's pin"));
            }
        }
    }
    errs
}

#[derive(Debug, Default, Serialize)]
pub struct Report {
    pub ok: bool,
    pub types: usize,
    /// Package → its interfaces' names.
    pub packages: BTreeMap<String, Vec<String>>,
    pub mux_pins: BTreeMap<String, usize>,
    pub errors: Vec<String>,
}

/// Check `ROOT/library/interfaces.json` and every `ROOT/packages/*/ports.json`.
pub fn check_all(root: &Path) -> Result<Report, String> {
    let types = load_types(&root.join("library/interfaces.json"))?;
    let mut r = Report { types: types.types.len(), ..Default::default() };
    for (name, t) in &types.types {
        for s in t.optional.iter().chain(&t.per_target).chain(t.joins.iter().flat_map(|j| [&j.0, &j.1])) {
            if !t.signals.contains_key(s) {
                r.errors.push(format!("library/interfaces.json: {name} names {s}, not one of its signals"));
            }
        }
    }
    let mut dirs: Vec<_> = std::fs::read_dir(root.join("packages")).map_err(|e| e.to_string())?.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.join("ports.json").is_file()).collect();
    dirs.sort();
    for d in dirs {
        let name = d.file_name().unwrap().to_string_lossy().to_string();
        let ports = load_ports(&d.join("ports.json"))?;
        let src = d.join("sources/sources.json");
        let sources: Option<BTreeSet<String>> = if src.is_file() { Some(read::<BTreeMap<String, Value>>(&src)?.into_keys().collect()) } else { None };
        r.errors.extend(check(&types, &name, &ports, sources.as_ref()));
        if let Some(m) = &ports.mux {
            r.mux_pins.insert(name.clone(), m.pins.len());
        }
        r.packages.insert(name, ports.interfaces.keys().cloned().collect());
    }
    r.ok = r.errors.is_empty();
    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn ports(p: &str) -> Ports {
        load_ports(&root().join(format!("packages/{p}/ports.json"))).unwrap()
    }

    fn names(o: &BTreeMap<String, Vec<PinChoice>>, sig: &str) -> Vec<String> {
        o[sig].iter().map(|c| c.name.clone()).collect()
    }

    #[test]
    fn this_repositorys_interfaces_check() {
        let r = check_all(&root()).unwrap();
        assert!(r.ok, "{:#?}", r.errors);
        assert_eq!(r.mux_pins["rp2350"], 48);
    }

    #[test]
    fn i2c0_reaches_every_fourth_gpio_and_more_on_the_qfn80() {
        let p = ports("rp2350");
        let a = p.options("RP2350A", "I2C0").unwrap();
        assert_eq!(names(&a, "SDA"), ["GPIO0", "GPIO4", "GPIO8", "GPIO12", "GPIO16", "GPIO20", "GPIO24", "GPIO28_ADC2"]);
        assert!(a["SDA"].iter().all(|c| c.function.as_deref() == Some("F3")));
        let b = p.options("RP2350B", "I2C0").unwrap();
        assert_eq!(b["SCL"].len(), 12);
        // Slices 8-11 exist only on QFN-80 pins.
        assert!(p.options("RP2350A", "PWM9").unwrap().is_empty());
        assert_eq!(names(&p.options("RP2350B", "PWM9").unwrap(), "A"), ["GPIO34", "GPIO42_ADC2"]);
        // SPI's TX is the type's SDO.
        assert_eq!(p.interfaces["SPI1"].type_signal("TX"), "SDO");
    }

    #[test]
    fn fixed_pins_by_variant() {
        let p = ports("rp2350");
        assert_eq!(p.options("RP2350A", "ADC").unwrap().len(), 4);
        assert_eq!(names(&p.options("RP2350B", "ADC").unwrap(), "ADC7"), ["GPIO47_ADC7"]);
        let imu = ports("lsm6dso");
        let i2c = imu.options("LSM6DSOTR", "i2c").unwrap();
        assert_eq!((i2c["SDA"][0].number.as_str(), i2c["SCL"][0].number.as_str()), ("14", "13"));
        assert_eq!(imu.options("LSM6DSOTR", "spi").unwrap()["SDO"][0].number, "1");
    }

    #[test]
    fn bad_data_is_named() {
        let types = load_types(&root().join("library/interfaces.json")).unwrap();
        let mut p = ports("lsm6dso");
        let i = p.interfaces.get_mut("i2c").unwrap();
        i.pins.insert("SCL".into(), "NOPE".into());
        i.roles = vec!["boss".into()];
        p.interfaces.get_mut("spi").unwrap().kind = "can".into();
        let errs = check(&types, "lsm6dso", &p, Some(&BTreeSet::new()));
        let all = errs.join("\n");
        for want in ["has no pin called", "no role boss", "no interface type can", "not in sources.json"] {
            assert!(all.contains(want), "{want} missing from:\n{all}");
        }
    }
}
