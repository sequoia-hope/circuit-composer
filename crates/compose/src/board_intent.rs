//! A composed board's intent, assembled from its packages' intent files.
//!
//! Each package's `intent.json` is written once, in the package's own names: its nets (`SW`,
//! `XIN`), its parts by role (`CIN`, `U1.49`). Composing places those names on the board. A
//! package net is found on the board through its pins, the anchor of `schematic/nets.json` first
//! when the package has one, then its circuit's net or port; package parts become designators
//! through the instance's binding. Blocks, regions and domains are named `INSTANCE/NAME`, as the
//! floorplanner names blocks.
//!
//! Two package nets can land on one board net (the buck's VOUT and the MCU's IOVDD are both
//! +3V3). Their intent is merged attribute by attribute: the higher voltage and current, the
//! shorter length limit, and otherwise the first instance's. Each merge is reported.
//!
//! The composition's own `intent` comes last: board nets no package covers (VBUS, a battery
//! connector), its own blocks and regions, and a `default_net` for every net still uncovered.
//! A field it sets wins over the packages'.
//!
//! The intent file is KiCad's sidecar, so its nets are named as KiCad names them on the board
//! `compose_kicad.py` writes ([`kicad_net_name`]); the composition's `intent` uses the composed
//! names, as its `nets` do.

use std::collections::BTreeMap;
use std::path::Path;

use intent::{Block, Domain, DomainKind, EnergySource, IntentFile, NetIntent, ProtectionPath, Region, Selector, Source};
use serde::{Deserialize, Serialize};

use crate::{package_dir, Circuit, Composed, Composition};

/// The composition's own intent, in board names.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Overlay {
    /// Copper layer count.
    #[serde(default)]
    pub layers: Option<u32>,
    #[serde(default)]
    pub sources: BTreeMap<String, Source>,
    #[serde(default)]
    pub nets: BTreeMap<String, NetIntent>,
    /// Intent for every board net nothing else covers.
    #[serde(default)]
    pub default_net: Option<NetIntent>,
    #[serde(default)]
    pub blocks: BTreeMap<String, Block>,
    #[serde(default)]
    pub regions: Vec<Region>,
    #[serde(default)]
    pub protection_paths: Vec<ProtectionPath>,
    #[serde(default)]
    pub energy_sources: Vec<EnergySource>,
}

#[derive(Debug, Serialize)]
pub struct BoardIntent {
    pub ok: bool,
    pub intent: IntentFile,
    /// `INSTANCE.NET` → the board net a package net landed on.
    pub nets: BTreeMap<String, String>,
    /// Package nets with no pin on the board (their parts omitted), as `INSTANCE.NET`.
    pub unplaced: Vec<String>,
    /// Board nets two sources gave intent to, and how they were merged.
    pub merged: Vec<String>,
    /// Board nets that took `default_net`.
    pub defaulted: Vec<String>,
    /// Board nets with no intent at all.
    pub uncovered: Vec<String>,
    pub errors: Vec<String>,
}

#[derive(Deserialize)]
struct PackageNets {
    nets: BTreeMap<String, PackageNet>,
}

#[derive(Deserialize)]
struct PackageNet {
    anchor: String,
    pins: Vec<String>,
}

/// A composed net's name in KiCad: `compose_kicad.py` draws every net as a local label, and
/// an instance's `/INSTANCE/NET` as `INSTANCE_NET`, so KiCad prefixes the root sheet's `/`.
pub fn kicad_net_name(composed: &str) -> String {
    format!("/{}", composed.trim_start_matches('/').replace('/', "_"))
}

/// One instance's names on the board.
struct Place<'a> {
    name: &'a str,
    refs: BTreeMap<String, String>,
    reversed: &'a [String],
    nets: BTreeMap<String, String>,
}

impl Place<'_> {
    fn part(&self, role: &str) -> Option<String> {
        self.refs.get(role).cloned()
    }

    /// `U1.49` → `U4.49`; a part role (`CIN`) → its designator.
    fn pin_or_part(&self, s: &str) -> Option<String> {
        match s.split_once('.') {
            Some((role, pin)) => {
                let pin = match (self.reversed.iter().any(|r| r == role), pin) {
                    (true, "1") => "2",
                    (true, "2") => "1",
                    _ => pin,
                };
                Some(format!("{}.{pin}", self.part(role)?))
            }
            None => self.part(s),
        }
    }

    fn net(&self, n: &str) -> Option<String> {
        self.nets.get(n).cloned()
    }

    fn scoped(&self, n: &str) -> String {
        format!("{}/{n}", self.name)
    }
}

/// The composed board's intent. `composed` is `compose_with`'s result for `c` and `circuits`.
pub fn board_intent(c: &Composition, circuits: &BTreeMap<String, Circuit>, composed: &Composed, packages: &Path) -> BoardIntent {
    let mut out = BoardIntent {
        ok: false,
        intent: IntentFile {
            schema_version: intent::SCHEMA_VERSION.into(),
            board: intent::BoardInfo { name: c.board.clone(), kicad_project: None, layers: c.intent.as_ref().and_then(|o| o.layers), packages: BTreeMap::new() },
            sources: BTreeMap::new(),
            nets: BTreeMap::new(),
            blocks: BTreeMap::new(),
            domains: None,
            regions: Vec::new(),
            protection_paths: Vec::new(),
            energy_sources: Vec::new(),
        },
        nets: BTreeMap::new(),
        unplaced: Vec::new(),
        merged: Vec::new(),
        defaulted: Vec::new(),
        uncovered: Vec::new(),
        errors: Vec::new(),
    };
    let on: BTreeMap<&str, String> = composed.nets.iter().flat_map(|(n, ps)| ps.iter().map(move |p| (p.as_str(), kicad_net_name(n)))).collect();
    let board_nets: Vec<String> = composed.nets.keys().map(|n| kicad_net_name(n)).collect();
    let mut root = Domain { name: c.board.clone(), kind: DomainKind::Board, supplies: Vec::new(), reference_ground: None, barriers: Vec::new(), switchable: false, blocks: Vec::new(), children: Vec::new() };

    for (name, inst) in c.instances.iter().filter(|(_, i)| i.recipe.is_none()) {
        let Some(binding) = composed.bindings.get(name) else { continue };
        let Ok(dir) = package_dir(packages, &inst.package) else { continue };
        let path = dir.join("intent.json");
        if !path.exists() {
            continue;
        }
        let pkg = match intent::load(&path) {
            Ok(p) => p,
            Err(e) => {
                out.errors.push(e);
                continue;
            }
        };
        let (pkg_name, version) = inst.package.split_once('@').unwrap_or((&inst.package, ""));
        out.intent.board.packages.insert(pkg_name.to_string(), version.to_string());
        let mut refs: BTreeMap<String, String> = binding.parts.clone();
        refs.insert("U1".into(), binding.chip.clone());
        let mut place = Place { name, refs, reversed: &inst.reversed, nets: BTreeMap::new() };

        // Package nets onto board nets, through their pins.
        let anchored: Option<PackageNets> = std::fs::read_to_string(dir.join("schematic/nets.json")).ok().and_then(|t| serde_json::from_str(&t).ok());
        let circ = &circuits[name];
        for net in pkg.nets.keys() {
            let mut pins: Vec<&String> = Vec::new();
            if let Some(n) = anchored.as_ref().and_then(|a| a.nets.get(net)) {
                pins.push(&n.anchor);
                pins.extend(&n.pins);
            }
            pins.extend(circ.nets.get(net).into_iter().flatten());
            pins.extend(circ.ports.get(net).into_iter().flat_map(|p| &p.pins));
            match pins.iter().filter_map(|p| place.pin_or_part(p)).find_map(|p| on.get(p.as_str()).cloned()) {
                Some(b) => {
                    place.nets.insert(net.clone(), b.clone());
                    out.nets.insert(format!("{name}.{net}"), b);
                }
                None => out.unplaced.push(format!("{name}.{net}")),
            }
        }

        for (id, s) in &pkg.sources {
            match out.intent.sources.get(id) {
                Some(have) if serde_json::to_value(have).ok() != serde_json::to_value(s).ok() => out.errors.push(format!("{name}: source {id} differs from another package's")),
                Some(_) => {}
                None => {
                    out.intent.sources.insert(id.clone(), s.clone());
                }
            }
        }
        for (net, n) in &pkg.nets {
            if let Some(b) = place.net(net) {
                let mut n = n.clone();
                if let Some(pair) = n.impedance.as_mut().and_then(|i| i.value.pair.as_mut()) {
                    match place.net(pair) {
                        Some(p) => *pair = p,
                        None => out.errors.push(format!("{name}.{net}: its pair {pair} is not on the board")),
                    }
                }
                merge_into(&mut out.intent.nets, &mut out.merged, &b, &n, &format!("{name}.{net}"));
            }
        }
        for (id, b) in &pkg.blocks {
            match map_block(&place, b, &inst.package) {
                Ok(nb) => {
                    out.intent.blocks.insert(place.scoped(id), nb);
                }
                Err(e) => out.errors.push(format!("{name}: block {id}: {e}")),
            }
        }
        if let Some(d) = &pkg.domains {
            let mut d = map_domain(&place, d, &out.intent.blocks);
            d.kind = DomainKind::Group;
            root.reference_ground = root.reference_ground.or(d.reference_ground.clone());
            for s in &d.supplies {
                if !root.supplies.contains(s) {
                    root.supplies.push(s.clone());
                }
            }
            root.children.push(d);
        }
        for r in &pkg.regions {
            match map_region(&place, r) {
                Ok(r) => out.intent.regions.push(r),
                Err(e) => out.errors.push(format!("{name}: region {}: {e}", r.name)),
            }
        }
        for p in &pkg.protection_paths {
            let seq: Option<Vec<String>> = p.sequence.iter().map(|s| place.pin_or_part(s)).collect();
            match (place.net(&p.net), seq) {
                (Some(net), Some(sequence)) => out.intent.protection_paths.push(ProtectionPath { net, sequence, ..p.clone() }),
                _ => out.errors.push(format!("{name}: protection path on {} is not on the board", p.net)),
            }
        }
        for e in &pkg.energy_sources {
            match place.net(&e.net) {
                Some(net) => out.intent.energy_sources.push(EnergySource { net, kind: e.kind, protected_zone: e.protected_zone.as_ref().map(|z| place.scoped(z)) }),
                None => out.errors.push(format!("{name}: energy source on {} is not on the board", e.net)),
            }
        }
    }

    // The composition's own intent, then the default for whatever is left.
    let overlay = c.intent.clone().unwrap_or_default();
    for (id, s) in overlay.sources {
        out.intent.sources.insert(id, s);
    }
    for (net, n) in &overlay.nets {
        if !composed.nets.contains_key(net) {
            out.errors.push(format!("intent: net {net} is not on the board"));
            continue;
        }
        let e = out.intent.nets.entry(kicad_net_name(net)).or_default();
        overlay_net(e, n);
    }
    // The composition's blocks, regions and paths name nets as it does; KiCad's names here.
    let k = |ns: &[String]| -> Vec<String> { ns.iter().map(|n| kicad_net_name(n)).collect() };
    for (id, mut b) in overlay.blocks {
        for port in b.ports.values_mut() {
            port.nets = k(&port.nets);
        }
        if let Some(g) = &mut b.placement_goal {
            for o in &mut g.objectives {
                o.nets = k(&o.nets);
            }
            for c in &mut g.constraints {
                c.value.nets = k(&c.value.nets);
            }
        }
        if out.intent.blocks.insert(id.clone(), b).is_some() {
            out.errors.push(format!("intent: block {id} is also a package's"));
        }
    }
    for id in out.intent.blocks.keys().filter(|id| !id.contains('/')) {
        root.blocks.push(id.clone());
    }
    for mut r in overlay.regions {
        for rule in &mut r.rules {
            if let Selector::Net(n) = &mut rule.applies_to {
                *n = kicad_net_name(n);
            }
        }
        out.intent.regions.push(r);
    }
    out.intent.protection_paths.extend(overlay.protection_paths.into_iter().map(|p| ProtectionPath { net: kicad_net_name(&p.net), ..p }));
    out.intent.energy_sources.extend(overlay.energy_sources.into_iter().map(|e| EnergySource { net: kicad_net_name(&e.net), ..e }));
    for net in &board_nets {
        if out.intent.nets.contains_key(net) {
            continue;
        }
        match &overlay.default_net {
            Some(d) => {
                out.intent.nets.insert(net.clone(), d.clone());
                out.defaulted.push(net.clone());
            }
            None => out.uncovered.push(net.clone()),
        }
    }
    if !root.children.is_empty() || !root.blocks.is_empty() {
        out.intent.domains = Some(root);
    }
    out.ok = out.errors.is_empty() && out.uncovered.is_empty();
    out
}

/// A package net's intent onto a board net, merging with what is there.
fn merge_into(nets: &mut BTreeMap<String, NetIntent>, merged: &mut Vec<String>, board: &str, n: &NetIntent, from: &str) {
    let Some(have) = nets.get_mut(board) else {
        nets.insert(board.to_string(), n.clone());
        return;
    };
    let mut how = Vec::new();
    if have.class != n.class {
        how.push(format!("class {} kept over {}", have.class.as_deref().unwrap_or("none"), n.class.as_deref().unwrap_or("none")));
    }
    match (&mut have.voltage, &n.voltage) {
        (Some(a), Some(b)) if b.value.peak_v > a.value.peak_v => {
            *a = b.clone();
            how.push(format!("voltage {} V", b.value.peak_v));
        }
        (a @ None, Some(b)) => *a = Some(b.clone()),
        _ => {}
    }
    match (&mut have.current, &n.current) {
        (Some(a), Some(b)) if b.value.continuous_a > a.value.continuous_a => {
            *a = b.clone();
            how.push(format!("current {} A", b.value.continuous_a));
        }
        (a @ None, Some(b)) => *a = Some(b.clone()),
        _ => {}
    }
    match (&mut have.max_length, &n.max_length) {
        (Some(a), Some(b)) if b.value.mm < a.value.mm => {
            *a = b.clone();
            how.push(format!("max_length {} mm", b.value.mm));
        }
        (a @ None, Some(b)) => *a = Some(b.clone()),
        _ => {}
    }
    let fill = |a: &mut NetIntent, b: &NetIntent| {
        macro_rules! take {
            ($($f:ident),*) => { $( if a.$f.is_none() { a.$f = b.$f.clone(); } )* };
        }
        take!(class, aggressor, victim, bandwidth, fault_current, impedance, topology, match_group, safety, note);
    };
    fill(have, n);
    merged.push(format!("{board} += {from}{}", if how.is_empty() { String::new() } else { format!(": {}", how.join(", ")) }));
}

/// The composition's fields win.
fn overlay_net(a: &mut NetIntent, b: &NetIntent) {
    macro_rules! put {
        ($($f:ident),*) => { $( if b.$f.is_some() { a.$f = b.$f.clone(); } )* };
    }
    put!(class, aggressor, victim, bandwidth, voltage, current, fault_current, impedance, topology, match_group, safety, max_length, note);
}

fn map_block(p: &Place, b: &Block, package: &str) -> Result<Block, String> {
    // Parts the composition omits drop out of the block; a role they played is an error.
    let parts = b.parts.iter().filter_map(|r| p.part(r)).collect();
    let mut roles = BTreeMap::new();
    for (role, who) in &b.roles {
        roles.insert(role.clone(), p.pin_or_part(who).ok_or_else(|| format!("role {role} ({who}) is not on the board"))?);
    }
    let nets = |ns: &[String]| -> Vec<String> { ns.iter().filter_map(|n| p.net(n)).collect() };
    let placement_goal = b.placement_goal.clone().map(|mut g| {
        for o in &mut g.objectives {
            o.nets = nets(&o.nets);
        }
        for c in &mut g.constraints {
            c.value.nets = nets(&c.value.nets);
            c.value.parts = c.value.parts.iter().filter_map(|r| p.pin_or_part(r)).collect();
        }
        g
    });
    let ports = b.ports.iter().map(|(k, port)| (k.clone(), intent::Port { nets: nets(&port.nets), ..port.clone() })).collect();
    Ok(Block { kind: b.kind.clone(), package: Some(package.to_string()), parts, roles, placement_goal, ports, provenance: None, note: b.note.clone() })
}

fn map_domain(p: &Place, d: &Domain, blocks: &BTreeMap<String, Block>) -> Domain {
    let mut supplies: Vec<String> = Vec::new();
    for s in d.supplies.iter().filter_map(|s| p.net(s)) {
        if !supplies.contains(&s) {
            supplies.push(s);
        }
    }
    Domain {
        name: p.scoped(&d.name),
        kind: d.kind,
        supplies,
        reference_ground: d.reference_ground.as_ref().and_then(|g| p.net(g)),
        barriers: d.barriers.iter().map(|b| intent::Barrier { to: p.scoped(&b.to), ..b.clone() }).collect(),
        switchable: d.switchable,
        blocks: d.blocks.iter().map(|b| p.scoped(b)).filter(|b| blocks.contains_key(b)).collect(),
        children: d.children.iter().map(|c| map_domain(p, c, blocks)).collect(),
    }
}

fn map_region(p: &Place, r: &Region) -> Result<Region, String> {
    if r.polygon.is_some() && r.anchor.is_none() {
        return Err("a polygon in board coordinates cannot come from a package; anchor it to a part".into());
    }
    let anchor = match &r.anchor {
        Some(a) => Some(p.part(a).ok_or_else(|| format!("anchor {a} is not on the board"))?),
        None => None,
    };
    let mut rules = Vec::new();
    for rule in &r.rules {
        let applies_to = match &rule.applies_to {
            Selector::Net(n) => Selector::Net(p.net(n).ok_or_else(|| format!("net {n} is not on the board"))?),
            s => s.clone(),
        };
        rules.push(intent::RegionRule { applies_to, ..rule.clone() });
    }
    Ok(Region { name: p.scoped(&r.name), anchor, block: r.block.as_ref().map(|b| p.scoped(b)), rules, ..r.clone() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compose_with;
    use serde_json::json;

    fn circuit() -> Circuit {
        serde_json::from_value(json!({
            "variant": "V",
            "parts": {
                "U1": { "symbol": "s:U", "value": "chip", "footprint": "f", "reference_ref": "U1" },
                "COUT": { "symbol": "Device:C", "value": "10u", "footprint": "f", "reference_ref": "C1" },
                "L1": { "symbol": "Device:L", "value": "2u2", "footprint": "f", "reference_ref": "L1" }
            },
            "nets": { "SW": ["U1.3", "L1.1"] },
            "ports": {
                "GND": { "kind": "ground", "pins": ["U1.2", "COUT.2"] },
                "VOUT": { "kind": "supply", "pins": ["L1.2", "COUT.1"] }
            }
        }))
        .unwrap()
    }

    fn package(dir: &Path, voltage: f64, current: f64, len: f64) {
        let p = dir.join("p");
        std::fs::create_dir_all(&p).unwrap();
        std::fs::write(p.join("manifest.toml"), "version = \"1\"\n").unwrap();
        let attr = |v: serde_json::Value| json!({ "value": v, "strength": "hard", "provenance": { "source": "ds", "kind": "datasheet" } });
        let intent = json!({
            "schema_version": "0.1",
            "board": { "name": "p" },
            "sources": { "ds": { "title": "datasheet" } },
            "nets": {
                "SW": { "class": "SWITCH", "voltage": attr(json!({ "peak_v": voltage })), "current": attr(json!({ "continuous_a": current })), "max_length": attr(json!({ "mm": len })) },
                "VOUT": { "class": "POWER", "voltage": attr(json!({ "peak_v": 3.3 })), "current": attr(json!({ "continuous_a": current })) },
                "GND": { "class": "GROUND", "current": attr(json!({ "continuous_a": current })) }
            },
            "blocks": { "reg": { "type": "buck", "parts": ["U1", "L1", "COUT"], "roles": { "sw": "U1.3", "inductor": "L1", "cout": "COUT" } } },
            "domains": { "name": "p", "kind": "board", "supplies": ["VOUT"], "reference_ground": "GND", "blocks": ["reg"] },
            "regions": [{ "name": "lx", "kind": "polygon", "anchor": "L1", "polygon": [[0, 0], [1, 0], [1, 1]], "rules": [{ "applies_to": { "net": "SW" }, "mode": "must" }] }]
        });
        std::fs::write(p.join("intent.json"), intent.to_string()).unwrap();
    }

    #[test]
    fn package_intent_lands_on_board_names() {
        let dir = std::env::temp_dir().join(format!("compose-intent-{}", std::process::id()));
        package(&dir, 5.0, 0.6, 4.0);
        let c: Composition = serde_json::from_value(json!({
            "board": "b",
            "instances": { "a": { "package": "p@1", "variant": "V" }, "b": { "package": "p@1", "variant": "V" } },
            "parts": { "J1": { "symbol": "Conn:J", "value": "J", "footprint": "f" } },
            "nets": { "GND": ["a.GND", "b.GND", "J1.2"], "+3V3": ["a.VOUT", "b.VOUT", "J1.1"], "SPARE": ["J1.3"] },
            "intent": {
                "sources": { "brief": { "title": "the board's brief" } },
                "nets": { "+3V3": { "note": "both regulators share the rail" } },
                "default_net": { "class": "SIGNAL", "voltage": { "value": { "peak_v": 3.3 }, "strength": "hard", "provenance": { "source": "brief", "kind": "requirement" } } }
            }
        }))
        .unwrap();
        let circs: BTreeMap<String, Circuit> = [("a".to_string(), circuit()), ("b".to_string(), circuit())].into();
        let composed = compose_with(&c, &circs);
        assert!(composed.ok, "{:?}", composed.errors);
        let r = board_intent(&c, &circs, &composed, &dir);
        std::fs::remove_dir_all(&dir).ok();
        assert!(r.ok, "{:?} {:?}", r.errors, r.uncovered);
        let i = &r.intent;
        // Each instance's switch node is its own board net; the rails are shared and merged.
        assert_eq!(r.nets["a.SW"], "/a_SW");
        assert_eq!(r.nets["b.VOUT"], "/+3V3");
        assert_eq!(i.nets["/b_SW"].class.as_deref(), Some("SWITCH"));
        assert_eq!(i.nets["/+3V3"].note.as_deref(), Some("both regulators share the rail"));
        assert!(r.merged.iter().any(|m| m.starts_with("/+3V3 += b.VOUT")));
        assert_eq!(r.defaulted, ["/SPARE"]);
        // Blocks, domains and regions in board names.
        let reg = &i.blocks["b/reg"];
        assert_eq!(reg.package.as_deref(), Some("p@1"));
        assert_eq!(reg.roles["sw"], format!("{}.3", composed.bindings["b"].chip));
        assert_eq!(reg.roles["inductor"], composed.bindings["b"].parts["L1"]);
        let d = i.domains.as_ref().unwrap();
        assert_eq!(d.children.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["a/p", "b/p"]);
        assert_eq!(d.children[1].blocks, ["b/reg"]);
        assert_eq!(d.supplies, ["/+3V3"]);
        let lx = i.regions.iter().find(|r| r.name == "b/lx").unwrap();
        assert_eq!(lx.anchor.as_deref(), Some(composed.bindings["b"].parts["L1"].as_str()));
        assert_eq!(lx.rules[0].applies_to, Selector::Net("/b_SW".into()));
        assert_eq!(i.board.packages["p"], "1");
    }

    #[test]
    fn merged_nets_keep_the_stricter_values() {
        fn attr<T: serde::de::DeserializeOwned>(v: serde_json::Value) -> intent::Attr<T> {
            serde_json::from_value(json!({ "value": v, "strength": "hard" })).unwrap()
        }
        let mut nets = BTreeMap::new();
        let mut merged = Vec::new();
        let a = NetIntent { class: Some("POWER".into()), voltage: Some(attr(json!({ "peak_v": 3.3 }))), current: Some(attr(json!({ "continuous_a": 0.1 }))), ..Default::default() };
        let b = NetIntent { class: Some("POWER".into()), voltage: Some(attr(json!({ "peak_v": 3.6 }))), current: Some(attr(json!({ "continuous_a": 0.6 }))), max_length: Some(attr(json!({ "mm": 9.0 }))), ..Default::default() };
        merge_into(&mut nets, &mut merged, "+3V3", &a, "mcu.IOVDD");
        merge_into(&mut nets, &mut merged, "+3V3", &b, "reg.VOUT");
        let n = &nets["+3V3"];
        assert_eq!((n.voltage.as_ref().unwrap().value.peak_v, n.current.as_ref().unwrap().value.continuous_a), (3.6, 0.6));
        assert_eq!(n.max_length.as_ref().unwrap().value.mm, 9.0);
        assert_eq!(merged, ["+3V3 += reg.VOUT: voltage 3.6 V, current 0.6 A"]);
    }
}
