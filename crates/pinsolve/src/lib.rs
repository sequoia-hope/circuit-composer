//! The pin solver: a composition written as loose connections ("the IMU's I2C to the MCU",
//! "the LED to the MCU") into one with nets pin by pin.
//!
//! A loose composition is a composition (`compose.json`) with a `connect` list:
//!
//! ```json
//! { "from": "imu.i2c", "to": "mcu" }
//! { "from": "imu.int1", "to": "mcu", "name": "IMU_INT1" }
//! { "from": "R4.1", "to": "mcu", "type": "logic", "name": "LED" }
//! { "from": "imu.i2c", "to": "mcu.I2C1", "pins": { "SDA": "GPIO6" } }
//! ```
//!
//! `from` is a device's interface (`INSTANCE.INTERFACE`, from its package's `ports.json`) or a
//! board part's pin with the `type` it carries; `to` is the instance that serves it, or one of
//! its interfaces. `pins` fixes the partner's pin for some of the device's signals.
//!
//! For each connection the solver finds the partner's interfaces that can serve it: the same type
//! in the complementary role (an `i2c` target needs an `i2c` controller), or a block that
//! implements the type (PIO, at a price). Signals pair by the type's joins (one side's SDO meets
//! the other's SDI), and each of the partner's signals takes a pin its mux table allows. Buses
//! are shared: a second I2C target goes on the same controller's two pins. Then it searches every
//! assignment, exactly up to a node limit, for the cheapest one:
//!
//! - a soft block (PIO) costs 100, a hardware bus block opened for the first time 4;
//! - an ADC-capable pin used for something other than analog costs 3;
//! - a connection's pins spread over the package cost their spread in pin numbers, so a bus's
//!   lines sit together.
//!
//! Electrical domains (a 5 V output into a 3.3 V input) are not judged yet.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use compose::{Circuit, Composition};
use interfaces::{Interface, Ports, Types};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Connect {
    pub from: String,
    pub to: String,
    /// The net's name, or its prefix when there are several signals (`NAME_SDA`).
    #[serde(default)]
    pub name: Option<String>,
    /// For a board part's pin: the interface type it carries (`logic`, `analog`, `pwm`).
    #[serde(default, rename = "type")]
    pub kind: Option<String>,
    /// The device's role, when its interface can take more than one.
    #[serde(default)]
    pub role: Option<String>,
    /// The partner's pin (by name) for some of the device's signals.
    #[serde(default)]
    pub pins: BTreeMap<String, String>,
    #[serde(default)]
    pub note: Option<String>,
}

/// One instance's package, as the solver reads it.
pub struct Package {
    pub circuit: Circuit,
    pub ports: Ports,
}

/// One pin the solver chose.
#[derive(Debug, Clone, Serialize)]
pub struct Row {
    /// The connection, as `from -> to`.
    pub connection: String,
    pub net: String,
    /// The device's endpoint (`imu.SDA`, `R4.1`).
    pub device: String,
    /// The partner's endpoint (`mcu.GPIO4`).
    pub partner: String,
    /// The partner's interface and its signal there (`I2C0 SDA`).
    pub via: String,
    /// The pin's number and the function select to set in firmware (`7`, `F3`).
    pub pin: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Solved {
    pub ok: bool,
    /// The composition with the connections as nets, and no `connect`.
    pub composition: Value,
    pub pinout: Vec<Row>,
    pub cost: f64,
    /// Whether the search covered every assignment (false: it stopped at its node limit).
    pub exhaustive: bool,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

/// A role's counterpart.
fn complement(role: &str) -> &str {
    match role {
        "controller" => "target",
        "target" => "controller",
        "device" => "host",
        "host" => "device",
        "debugger" => "target",
        _ => "",
    }
}

/// One way to serve a connection: a partner interface, and for each device signal the partner
/// signals it may meet there.
#[derive(Debug, Clone)]
struct Way {
    iface: String,
    kind: String,
    soft: bool,
    /// The partner's interface is a bus here: its signals can be shared.
    bus: bool,
    /// The partner's own signals each target needs its own of (SPI's CSn).
    per_target: Vec<String>,
    /// Every pin is its own interface (SIO, PIO, ADC channels): only the pins are exclusive.
    per_pin: bool,
    /// Device signal (own name) → partner own signals it may use.
    pairs: Vec<(String, Vec<String>)>,
    /// Several connections may use the instance, each on signals of its own (PWM channels).
    shared_by_signal: bool,
}

/// One concrete choice for a connection: the way, and each device signal's partner signal and
/// pin.
#[derive(Debug, Clone)]
struct Pick {
    way: usize,
    /// (device signal, partner signal, partner pin number, partner pin name, function)
    sigs: Vec<(String, String, String, String, Option<String>)>,
    cost: f64,
}

struct Conn<'a> {
    c: &'a Connect,
    /// The device's endpoint per device signal: `imu.SDA` or `R4.1`.
    ends: BTreeMap<String, String>,
    partner: String,
    picks: Vec<Pick>,
    ways: Vec<Way>,
    analog: bool,
}

/// Load the packages of a loose composition's instances.
pub fn load(c: &Composition, packages: &Path) -> Result<BTreeMap<String, Package>, String> {
    let mut out = BTreeMap::new();
    for (name, inst) in &c.instances {
        let circuit = compose::load_circuit(packages, inst)?;
        let ports = interfaces::load_ports(&compose::package_dir(packages, &inst.package)?.join("ports.json"))?;
        out.insert(name.clone(), Package { circuit, ports });
    }
    Ok(out)
}

/// The packages from JSON, for a caller with no package directory (the app): instance name →
/// `{ "circuit": <circuit.VARIANT.json>, "ports": <ports.json> }`.
pub fn from_json(v: &Value) -> Result<BTreeMap<String, Package>, String> {
    let m = v.as_object().ok_or("packages: expected an object of instances")?;
    m.iter()
        .map(|(k, x)| {
            let circuit = serde_json::from_value(x["circuit"].clone()).map_err(|e| format!("{k}'s circuit: {e}"))?;
            let ports = serde_json::from_value(x["ports"].clone()).map_err(|e| format!("{k}'s ports: {e}"))?;
            Ok((k.clone(), Package { circuit, ports }))
        })
        .collect()
}

/// The circuit's port on the chip's pin `number` (the chip is the circuit's part `U1`), or a
/// port named `name` (a pin behind a series part, `USB_DP`).
fn port_of(circuit: &Circuit, number: &str, name: &str) -> Option<String> {
    let pin = format!("U1.{number}");
    circuit.ports.iter().find(|(_, p)| p.pins.contains(&pin)).map(|(n, _)| n.clone()).or_else(|| circuit.ports.contains_key(name).then(|| name.to_string()))
}

/// What a port already has on it inside the package, besides the chip: None when a fitted part
/// (a series resistor, a switch) makes it taken; otherwise what it costs, 2 when an unfitted link
/// (`DNF`) shares its pad.
fn inside(circuit: &Circuit, port: &str, omit: &[String]) -> Option<f64> {
    let p = circuit.ports.get(port)?;
    let mut cost = 0.0;
    for pin in p.pins.iter().filter(|x| !x.starts_with("U1.")) {
        let part = pin.split('.').next().unwrap_or("");
        if omit.iter().any(|o| o == part) {
            continue;
        }
        match circuit.parts.get(part) {
            Some(x) if x.value == "DNF" => cost += 2.0,
            _ => return None,
        }
    }
    Some(cost)
}

/// Ports a circuit offers for signals: not ground or supply.
fn signal_port(circuit: &Circuit, port: &str) -> bool {
    circuit.ports.get(port).is_some_and(|p| p.kind != "ground" && p.kind != "supply")
}

/// Solve a loose composition (`compose.json` with `connect`).
pub fn solve(loose: &Value, types: &Types, pkgs: &BTreeMap<String, Package>, node_limit: usize) -> Result<Solved, String> {
    let comp: Composition = serde_json::from_value(loose.clone()).map_err(|e| format!("composition: {e}"))?;
    let connects: Vec<Connect> = comp.connect.iter().map(|v| serde_json::from_value(v.clone()).map_err(|e| format!("connect {v}: {e}"))).collect::<Result<_, _>>()?;
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    // Ports already taken: on a net, or deliberately open.
    let mut taken: BTreeSet<String> = comp.nets.values().flatten().cloned().collect();
    taken.extend(comp.no_connect.iter().cloned());

    let mut conns: Vec<Conn> = Vec::new();
    for c in &connects {
        match prepare(c, &comp, types, pkgs) {
            Ok(conn) => conns.push(conn),
            Err(e) => errors.push(format!("{} -> {}: {e}", c.from, c.to)),
        }
    }
    // A device interface used twice, or with one it excludes.
    let mut used: BTreeMap<String, String> = BTreeMap::new();
    for c in &connects {
        if let Some((inst, iface)) = c.from.split_once('.').filter(|(i, _)| comp.instances.contains_key(*i)) {
            if let Some(prev) = used.insert(c.from.clone(), c.to.clone()) {
                errors.push(format!("{} is connected twice (to {prev} and {})", c.from, c.to));
            }
            if let Some(i) = pkgs.get(inst).and_then(|p| p.ports.interfaces.get(iface)) {
                for x in &i.excludes {
                    if connects.iter().any(|o| o.from == format!("{inst}.{x}")) {
                        errors.push(format!("{} and {inst}.{x} can't both be used", c.from));
                    }
                }
            }
        }
    }
    if !errors.is_empty() {
        return Ok(Solved { ok: false, composition: Value::Null, pinout: vec![], cost: 0.0, exhaustive: true, warnings, errors });
    }

    // Every connection's concrete picks, cheapest first.
    for conn in conns.iter_mut() {
        let pk = &pkgs[&conn.partner];
        let variant = comp.instances[&conn.partner].variant.clone();
        let mut picks = Vec::new();
        for (w, way) in conn.ways.iter().enumerate() {
            let opts = pk.ports.options(&variant, &way.iface)?;
            // Each device signal's candidates: (partner signal, number, name, function), on a
            // port the partner's circuit offers and nothing else has.
            let mut cands: Vec<Vec<(String, String, String, Option<String>)>> = Vec::new();
            for (dsig, psigs) in &way.pairs {
                let mut v = Vec::new();
                for ps in psigs {
                    for ch in opts.get(ps).into_iter().flatten() {
                        let Some(port) = port_of(&pk.circuit, &ch.number, &ch.name) else { continue };
                        if !signal_port(&pk.circuit, &port) || taken.contains(&format!("{}.{port}", conn.partner)) {
                            continue;
                        }
                        let Some(_) = inside(&pk.circuit, &port, &comp.instances[&conn.partner].omit) else { continue };
                        if let Some(want) = conn.c.pins.get(dsig) {
                            if &ch.name != want && !ch.name.starts_with(&format!("{want}_")) && &port != want {
                                continue;
                            }
                        }
                        v.push((ps.clone(), ch.number.clone(), ch.name.clone(), ch.function.clone()));
                    }
                }
                cands.push(v);
            }
            if cands.iter().any(Vec::is_empty) {
                continue;
            }
            // The product of each signal's candidates, distinct pins, cheapest kept.
            let mut acc: Vec<Vec<(String, String, String, Option<String>)>> = vec![vec![]];
            for cs in &cands {
                let mut next = Vec::new();
                for a in &acc {
                    for c in cs {
                        if a.iter().any(|x| x.1 == c.1) {
                            continue;
                        }
                        let mut b = a.clone();
                        b.push(c.clone());
                        next.push(b);
                    }
                }
                acc = next;
                if acc.len() > 20_000 {
                    acc.sort_by(|a, b| spread(a).total_cmp(&spread(b)));
                    acc.truncate(20_000);
                }
            }
            for a in acc {
                let mut cost = if way.soft { 100.0 } else { 0.0 } + spread(&a);
                for (_, num, name, _) in &a {
                    if name.contains("_ADC") && !conn.analog {
                        cost += 3.0;
                    }
                    let port = port_of(&pk.circuit, num, name).unwrap_or_default();
                    cost += inside(&pk.circuit, &port, &comp.instances[&conn.partner].omit).unwrap_or(0.0);
                    cost += num.parse::<f64>().unwrap_or(0.0) * 1e-4;
                }
                let sigs = way.pairs.iter().zip(a).map(|((d, _), (ps, num, name, f))| (d.clone(), ps, num, name, f)).collect();
                picks.push(Pick { way: w, sigs, cost });
            }
        }
        if picks.is_empty() {
            let kinds: Vec<String> = conn.ways.iter().map(|w| w.iface.clone()).collect();
            errors.push(format!("{} -> {}: no free pins on {}", conn.c.from, conn.c.to, if kinds.is_empty() { "any interface".into() } else { kinds.join(", ") }));
        }
        picks.sort_by(|a, b| a.cost.total_cmp(&b.cost));
        conn.picks = picks;
    }
    if !errors.is_empty() {
        return Ok(Solved { ok: false, composition: Value::Null, pinout: vec![], cost: 0.0, exhaustive: true, warnings, errors });
    }

    // Fewest choices first.
    let mut order: Vec<usize> = (0..conns.len()).collect();
    order.sort_by_key(|&i| (conns[i].picks.len(), i));
    // What the connections still to place will cost at least: their cheapest picks.
    let mut rest = vec![0.0; order.len() + 1];
    for d in (0..order.len()).rev() {
        rest[d] = rest[d + 1] + conns[order[d]].picks[0].cost;
    }
    let mut s = Search { conns: &conns, order: &order, rest: &rest, best: None, nodes: 0, limit: node_limit, chosen: vec![usize::MAX; conns.len()] };
    s.go(0, 0.0, &mut State::default());
    let exhaustive = s.nodes < s.limit;
    let Some((cost, choice)) = s.best else {
        errors.push("no assignment gives every connection its own pins: free some, or connect fewer".into());
        return Ok(Solved { ok: false, composition: Value::Null, pinout: vec![], cost: 0.0, exhaustive, warnings, errors });
    };

    // Nets: a shared bus signal is one net for all its devices.
    let mut nets: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut net_of: BTreeMap<(String, String, String), String> = BTreeMap::new();
    let mut pinout = Vec::new();
    for (k, conn) in conns.iter().enumerate() {
        let p = &conn.picks[choice[k]];
        let way = &conn.ways[p.way];
        let many = p.sigs.len() > 1;
        for (dsig, psig, num, name, f) in &p.sigs {
            let port = port_of(&pkgs[&conn.partner].circuit, num, name).unwrap();
            let partner = format!("{}.{port}", conn.partner);
            let shared = way.bus && !way.per_target.contains(psig) && !way.soft;
            let key = (conn.partner.clone(), way.iface.clone(), psig.clone());
            let net = match (shared, net_of.get(&key)) {
                (true, Some(n)) => n.clone(),
                _ => {
                    let base = match (&conn.c.name, shared) {
                        (Some(n), _) => n.clone(),
                        (None, true) => way.iface.clone(),
                        (None, false) => conn.c.from.replace('.', "_").to_uppercase(),
                    };
                    let n = if many || (shared && conn.c.name.is_none()) { format!("{base}_{}", dsig.to_uppercase()) } else { base };
                    if shared {
                        net_of.insert(key, n.clone());
                    }
                    n
                }
            };
            let e = nets.entry(net.clone()).or_default();
            if !e.contains(&partner) {
                e.push(partner.clone());
            }
            e.push(conn.ends[dsig].clone());
            pinout.push(Row { connection: format!("{} -> {}", conn.c.from, conn.c.to), net, device: conn.ends[dsig].clone(), partner, via: format!("{} {psig}", way.iface), pin: num.clone(), function: f.clone() });
        }
        if way.soft {
            warnings.push(format!("{} -> {}: on {}, in software (no hardware {} was free)", conn.c.from, conn.c.to, way.iface, way.kind));
        }
    }
    // I2C targets sharing a bus need distinct addresses.
    let mut on_bus: BTreeMap<(String, String), Vec<(String, BTreeSet<String>)>> = BTreeMap::new();
    for (k, conn) in conns.iter().enumerate() {
        let way = &conn.ways[conn.picks[choice[k]].way];
        if way.kind == "i2c" && way.bus && !way.soft {
            let addrs = conn
                .c
                .from
                .split_once('.')
                .and_then(|(i, f)| pkgs.get(i)?.ports.interfaces.get(f))
                .and_then(|i| i.extra.get("addresses"))
                .and_then(Value::as_object)
                .map(|m| m.keys().cloned().collect())
                .unwrap_or_default();
            on_bus.entry((conn.partner.clone(), way.iface.clone())).or_default().push((conn.c.from.clone(), addrs));
        }
    }
    for ((inst, bus), devs) in &on_bus {
        if devs.len() > 1 {
            let all: BTreeSet<&String> = devs.iter().flat_map(|d| &d.1).collect();
            let names: Vec<&str> = devs.iter().map(|d| d.0.as_str()).collect();
            if all.len() < devs.len() {
                errors.push(format!("{inst}.{bus}: {} share it, but have only {} addresses between them", names.join(", "), all.len()));
            } else {
                warnings.push(format!("{inst}.{bus}: {} share it; check their address straps differ", names.join(", ")));
            }
        }
    }
    let mut out = loose.clone();
    let obj = out.as_object_mut().ok_or("the composition is not an object")?;
    obj.remove("connect");
    let n = obj.entry("nets").or_insert(Value::Object(Default::default())).as_object_mut().ok_or("nets is not an object")?;
    for (k, v) in &nets {
        if n.contains_key(k) {
            errors.push(format!("{k}: the solver's net has a name the composition already uses; name the connection"));
        }
        n.insert(k.clone(), serde_json::to_value(v).unwrap());
    }
    Ok(Solved { ok: errors.is_empty(), composition: out, pinout, cost, exhaustive, warnings, errors })
}

/// How far apart a connection's pins are, in pin numbers.
fn spread(a: &[(String, String, String, Option<String>)]) -> f64 {
    let ns: Vec<f64> = a.iter().filter_map(|x| x.1.parse::<f64>().ok()).collect();
    if ns.len() < 2 {
        return 0.0;
    }
    ns.iter().cloned().fold(f64::NEG_INFINITY, f64::max) - ns.iter().cloned().fold(f64::INFINITY, f64::min)
}

/// A connection's device side, and every way its partner can serve it.
fn prepare<'a>(c: &'a Connect, comp: &Composition, types: &Types, pkgs: &BTreeMap<String, Package>) -> Result<Conn<'a>, String> {
    let (partner, only) = match c.to.split_once('.') {
        Some((i, f)) => (i.to_string(), Some(f.to_string())),
        None => (c.to.clone(), None),
    };
    let pk = pkgs.get(&partner).ok_or_else(|| format!("no instance {partner}"))?;
    // The device: an instance's interface, or a part's pin.
    let (kind, roles, dir, dsigs, ends): (String, Vec<String>, Option<String>, Vec<String>, BTreeMap<String, String>) = match c.from.split_once('.') {
        Some((inst, iface)) if comp.instances.contains_key(inst) => {
            let dp = &pkgs[inst];
            let variant = &comp.instances[inst].variant;
            let i = dp.ports.interfaces.get(iface).ok_or_else(|| format!("{inst} has no interface {iface}: {}", dp.ports.interfaces.keys().cloned().collect::<Vec<_>>().join(", ")))?;
            let opts = dp.ports.options(variant, iface)?;
            let mut ends = BTreeMap::new();
            for (sig, pins) in &opts {
                let [ch] = &pins[..] else { return Err(format!("{inst}.{iface}'s {sig} could be on several pins; a device's interface must be on fixed pins")) };
                let port = port_of(&dp.circuit, &ch.number, &ch.name).ok_or_else(|| format!("{inst}.{iface}'s {sig} is on pin {} ({}), which the package's circuit doesn't offer as a port", ch.number, ch.name))?;
                if !signal_port(&dp.circuit, &port) {
                    return Err(format!("{inst}.{iface}'s {sig} is on pin {} ({}), which the package ties to {port}; this circuit can't use {iface}", ch.number, ch.name));
                }
                ends.insert(sig.clone(), format!("{inst}.{port}"));
            }
            let roles = match &c.role {
                Some(r) if i.roles.contains(r) => vec![r.clone()],
                Some(r) => return Err(format!("{inst}.{iface} can't be {r}: {}", i.roles.join(", "))),
                None => i.roles.clone(),
            };
            (i.kind.clone(), roles, i.dir.clone(), ends.keys().cloned().collect(), ends)
        }
        Some((r, _)) => {
            if !comp.parts.contains_key(r) {
                return Err(format!("{r} is neither an instance nor a board part"));
            }
            let kind = c.kind.clone().ok_or("a part's pin needs the type it carries (\"type\": \"logic\")")?;
            let t = types.types.get(&kind).ok_or_else(|| format!("no interface type {kind}"))?;
            let sig = if kind == "pwm" { "A|B".to_string() } else if t.signals.len() == 1 { t.signals.keys().next().unwrap().clone() } else { return Err(format!("a single pin can't carry {kind}")) };
            (kind, vec![], None, vec![sig.clone()], [(sig, c.from.clone())].into())
        }
        None => return Err("from must be INSTANCE.INTERFACE or REF.PIN".into()),
    };
    let t = types.types.get(&kind).ok_or_else(|| format!("no interface type {kind}"))?;
    let mut ways = Vec::new();
    for (name, i) in &pk.ports.interfaces {
        if only.as_ref().is_some_and(|o| o != name) {
            continue;
        }
        let soft = i.kind != kind && i.implements.contains(&kind);
        if i.kind != kind && !soft {
            continue;
        }
        if !soft && !t.roles.is_empty() && !roles.iter().any(|r| i.roles.iter().any(|x| x == complement(r))) {
            continue;
        }
        if !soft && !dir_ok(dir.as_deref(), i) {
            continue;
        }
        let pairs: Vec<(String, Vec<String>)> = dsigs
            .iter()
            .map(|d| {
                let want: Vec<String> = if d == "A|B" {
                    vec!["A".into(), "B".into()]
                } else if soft {
                    vec!["IO".into()]
                } else {
                    let dt = c.from.split_once('.').and_then(|(inst, f)| pkgs.get(inst)?.ports.interfaces.get(f)).map(|x| x.type_signal(d).to_string()).unwrap_or(d.clone());
                    let pt = t.joins.iter().find(|j| j.0 == dt).map(|j| j.1.clone()).unwrap_or(dt);
                    own_signals(i, &pt, pk, &comp.instances[&partner].variant, name)
                };
                (d.clone(), want)
            })
            .collect();
        if pairs.iter().any(|p| p.1.is_empty()) {
            continue;
        }
        let own = pk.ports.options(&comp.instances[&partner].variant, name).unwrap_or_default();
        let per_target = own.keys().filter(|o| t.per_target.iter().any(|x| x == i.type_signal(o))).cloned().collect();
        ways.push(Way { iface: name.clone(), kind: kind.clone(), soft, bus: t.bus, per_target, per_pin: i.per_pin, pairs, shared_by_signal: kind == "pwm" });
    }
    if ways.is_empty() {
        return Err(format!("{partner} has no interface that serves {kind}{}", if roles.is_empty() { String::new() } else { format!(" as {}", roles.iter().map(|r| complement(r)).collect::<Vec<_>>().join(" or ")) }));
    }
    Ok(Conn { c, ends, partner, picks: vec![], ways, analog: kind == "analog" })
}

/// A partner interface's own signals that are the type's `ts`.
fn own_signals(i: &Interface, ts: &str, pk: &Package, variant: &str, name: &str) -> Vec<String> {
    let opts = pk.ports.options(variant, name).unwrap_or_default();
    opts.keys().filter(|own| i.type_signal(own) == ts).cloned().collect()
}

/// Whether a logic line's directions meet: an output needs something that listens.
fn dir_ok(device: Option<&str>, partner: &Interface) -> bool {
    match (device, partner.dir.as_deref()) {
        (Some("out"), Some("out")) | (Some("in"), Some("in")) => false,
        _ => true,
    }
}

#[derive(Default, Clone)]
struct State {
    /// Partner port → what holds it: (interface, partner signal), so a bus signal can be shared.
    pins: BTreeMap<(String, String), (String, String)>,
    /// (partner, interface, partner signal) → pin number, for a bus's shared signals.
    bus: BTreeMap<(String, String, String), String>,
    /// Instances held whole by one connection (a UART), and those holding any signal.
    whole: BTreeSet<(String, String)>,
    opened: BTreeSet<(String, String)>,
    /// (partner, interface, signal) held by a connection on a shared-by-signal block.
    signal: BTreeSet<(String, String, String)>,
}

struct Search<'a> {
    conns: &'a [Conn<'a>],
    order: &'a [usize],
    rest: &'a [f64],
    best: Option<(f64, Vec<usize>)>,
    nodes: usize,
    limit: usize,
    chosen: Vec<usize>,
}

impl Search<'_> {
    fn go(&mut self, depth: usize, cost: f64, st: &mut State) {
        if self.best.as_ref().is_some_and(|b| cost + self.rest[depth] >= b.0 - 1e-9) || self.nodes >= self.limit {
            return;
        }
        if depth == self.order.len() {
            self.best = Some((cost, self.chosen.clone()));
            return;
        }
        let k = self.order[depth];
        let conn = &self.conns[k];
        for (pi, p) in conn.picks.iter().enumerate() {
            self.nodes += 1;
            if self.nodes >= self.limit {
                return;
            }
            let way = &conn.ways[p.way];
            let inst = (conn.partner.clone(), way.iface.clone());
            let Some(extra) = fits(conn, way, p, st) else { continue };
            // Picks come cheapest first: once one can't beat the best, none after it can.
            if self.best.as_ref().is_some_and(|b| cost + p.cost + self.rest[depth + 1] >= b.0 - 1e-9) {
                break;
            }
            let mut next = st.clone();
            take(conn, way, p, &mut next);
            let opened = !st.opened.contains(&inst) && way.bus && !way.soft;
            let c = cost + p.cost + extra + if opened { 4.0 } else { 0.0 };
            self.chosen[k] = pi;
            self.go(depth + 1, c, &mut next);
        }
    }
}

/// Whether a pick fits what is already taken; the extra cost if so.
fn fits(conn: &Conn, way: &Way, p: &Pick, st: &State) -> Option<f64> {
    let inst = (conn.partner.clone(), way.iface.clone());
    if st.whole.contains(&inst) {
        return None;
    }
    let exclusive = !way.bus && !way.shared_by_signal && !way.per_pin;
    if exclusive && st.opened.contains(&inst) {
        return None;
    }
    for (_, ps, num, _, _) in &p.sigs {
        let shareable = way.bus && !way.soft && !way.per_target.iter().any(|t| t == ps);
        let key = (conn.partner.clone(), way.iface.clone(), ps.clone());
        if shareable {
            if let Some(n) = st.bus.get(&key) {
                if n != num {
                    return None;
                }
                continue;
            }
        } else if way.shared_by_signal && !way.per_pin && st.signal.contains(&key) {
            return None;
        }
        if st.pins.contains_key(&(conn.partner.clone(), num.clone())) {
            return None;
        }
    }
    Some(0.0)
}

fn take(conn: &Conn, way: &Way, p: &Pick, st: &mut State) {
    let inst = (conn.partner.clone(), way.iface.clone());
    if !way.bus && !way.shared_by_signal && !way.per_pin {
        st.whole.insert(inst.clone());
    }
    st.opened.insert(inst);
    for (_, ps, num, _, _) in &p.sigs {
        let key = (conn.partner.clone(), way.iface.clone(), ps.clone());
        if way.bus && !way.soft && !way.per_target.iter().any(|t| t == ps) {
            st.bus.insert(key.clone(), num.clone());
        }
        st.signal.insert(key);
        st.pins.insert((conn.partner.clone(), num.clone()), (way.iface.clone(), ps.clone()));
    }
}
