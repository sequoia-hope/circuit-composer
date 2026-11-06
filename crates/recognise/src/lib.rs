//! Draft intent from a netlist alone. Every class, attribute and block it writes is marked
//! `recognised`, with a confidence and the facts it rests on, and is soft: a guess binds nothing
//! until a person or a datasheet confirms it.
//!
//! The input is what a board file holds when nobody annotated it: reference designators, values,
//! footprint names, which pad is on which net, and net names. Pin functions are used when the
//! netlist has them (a schematic export does; the PCBench corpus does not).

mod current;
pub mod names;
pub mod parts;

use std::collections::{BTreeMap, BTreeSet};

use intent::model::*;
use intent::netlist::{Component, Net, Netlist, Node};
use parts::Kind;
use serde::Serialize;

pub const SOURCE: &str = "recogniser";
pub const VERSION: &str = "0";

/// A netlist from a KiCad board's pads: what the corpus boards give, since none of them come with
/// a schematic.
pub fn netlist_from_board(board: &pcb::Board) -> Netlist {
    let mut out = Netlist::default();
    for (r, p) in &board.parts {
        out.components.insert(r.clone(), Component { value: p.value.clone(), footprint: p.footprint.clone(), fields: BTreeMap::new() });
    }
    for pad in &board.pads {
        if pad.net.is_empty() {
            continue;
        }
        let net = out.nets.entry(pad.net.clone()).or_insert_with(Net::default);
        if !net.has_pad(&pad.part, &pad.pin) {
            net.nodes.push(Node { reference: pad.part.clone(), pin: pad.pin.clone(), function: None });
        }
    }
    out
}

#[derive(Debug, Clone, Serialize)]
pub struct Recognised {
    pub intent: IntentFile,
    pub report: Report,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Report {
    pub parts: BTreeMap<Kind, usize>,
    /// Nets joining two or more electrical pads.
    pub nets: usize,
    /// Of those, nets a designer named.
    pub named: usize,
    /// Nets given a class, per class.
    pub classes: BTreeMap<String, usize>,
    pub blocks: BTreeMap<String, usize>,
    /// Guesses that lost to a more confident one, kept so a reviewer sees the dispute.
    pub disputes: Vec<String>,
}

#[derive(Debug, Clone)]
struct Guess {
    class: String,
    confidence: f64,
    evidence: Vec<String>,
}

#[derive(Default)]
struct NetFacts {
    voltage: Option<(f64, String)>,
    impedance: Option<(Impedance, String)>,
    bandwidth: Option<(BandwidthKind, String)>,
    /// The estimate, its confidence and what it rests on.
    current: Option<(Current, f64, Vec<String>)>,
}

/// A regulator as the current estimate sees it: what it draws from, what it feeds, and the most
/// its parts allow.
struct Reg {
    part: String,
    switching: bool,
    input: Option<String>,
    output: String,
    rating: Option<(f64, String)>,
}

struct Cx<'a> {
    nl: &'a Netlist,
    kinds: BTreeMap<&'a str, (Kind, String)>,
    /// Electrical nodes per net: mechanical footprints left out.
    nodes: BTreeMap<&'a str, Vec<&'a Node>>,
    grounds: BTreeSet<&'a str>,
    guesses: BTreeMap<String, Guess>,
    facts: BTreeMap<String, NetFacts>,
    blocks: BTreeMap<String, Block>,
    disputes: Vec<String>,
    regs: Vec<Reg>,
}

impl<'a> Cx<'a> {
    fn kind(&self, r: &str) -> Kind {
        self.kinds.get(r).map_or(Kind::Other, |k| k.0)
    }

    fn why(&self, r: &str) -> String {
        self.kinds.get(r).map(|k| k.1.clone()).unwrap_or_else(|| format!("{r} is not in the netlist"))
    }

    /// The nets a part's electrical pads are on, each once.
    fn part_nets(&self, r: &str) -> Vec<&'a str> {
        let mut out: Vec<&str> = Vec::new();
        for (net, nodes) in &self.nodes {
            if nodes.iter().any(|n| n.reference == r) && !out.contains(net) {
                out.push(net);
            }
        }
        out
    }

    fn parts_on(&self, net: &str) -> Vec<&'a str> {
        let mut out: Vec<&str> = self.nodes.get(net).map(|ns| ns.iter().map(|n| n.reference.as_str()).collect()).unwrap_or_default();
        out.dedup();
        out.sort();
        out.dedup();
        out
    }

    fn kinds_on(&self, net: &str, kind: Kind) -> Vec<&'a str> {
        self.parts_on(net).into_iter().filter(|r| self.kind(r) == kind).collect()
    }

    /// For a two-terminal part on `net`, the net its other pad is on.
    fn other_side(&self, part: &str, net: &str) -> Option<&'a str> {
        let nets = self.part_nets(part);
        if nets.len() != 2 {
            return None;
        }
        nets.into_iter().find(|n| *n != net)
    }

    /// Capacitors from `net` to a ground.
    fn caps_to_ground(&self, net: &str) -> Vec<&'a str> {
        self.kinds_on(net, Kind::Capacitor).into_iter().filter(|c| self.other_side(c, net).is_some_and(|o| self.grounds.contains(o))).collect()
    }

    fn class_of(&self, net: &str) -> Option<&str> {
        self.guesses.get(net).map(|g| g.class.as_str())
    }

    /// Records a class guess; the more confident guess wins and the loser is kept as a dispute.
    fn guess(&mut self, net: &str, class: &str, confidence: f64, evidence: Vec<String>) {
        match self.guesses.get_mut(net) {
            Some(g) if g.class == class => {
                g.confidence = g.confidence.max(confidence);
                for e in evidence {
                    if !g.evidence.contains(&e) {
                        g.evidence.push(e);
                    }
                }
            }
            Some(g) if g.confidence >= confidence => {
                self.disputes.push(format!("{net}: {} ({:.2}) kept over {class} ({confidence:.2}: {})", g.class, g.confidence, evidence.join("; ")));
            }
            Some(g) => {
                self.disputes.push(format!("{net}: {class} ({confidence:.2}) replaced {} ({:.2}: {})", g.class, g.confidence, g.evidence.join("; ")));
                *g = Guess { class: class.into(), confidence, evidence };
            }
            None => {
                self.guesses.insert(net.to_string(), Guess { class: class.into(), confidence, evidence });
            }
        }
    }

    fn block(&mut self, kind: &str, anchor: &str, parts: Vec<&str>, roles: BTreeMap<String, String>, confidence: f64, evidence: Vec<String>) {
        let mut parts: Vec<String> = parts.into_iter().map(str::to_string).collect();
        parts.sort();
        parts.dedup();
        let id = format!("{kind}_{anchor}");
        self.blocks.insert(
            id,
            Block { kind: kind.into(), package: None, parts, roles, placement_goal: None, ports: BTreeMap::new(), provenance: Some(prov(confidence, evidence)), note: None },
        );
    }

    /// Pin functions of the pads on a net, when the netlist has them.
    fn functions(&self, net: &str) -> Vec<(&'a str, &'a str, &'a str)> {
        self.nodes.get(net).into_iter().flatten().filter_map(|n| n.function.as_deref().map(|f| (n.reference.as_str(), n.pin.as_str(), f))).collect()
    }
}

fn prov(confidence: f64, evidence: Vec<String>) -> Provenance {
    Provenance { source: SOURCE.into(), locator: None, kind: ClaimKind::Recognised, date: None, note: None, confidence: Some(round2(confidence)), evidence }
}

fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

fn soft<T>(value: T, confidence: f64, evidence: Vec<String>) -> Attr<T> {
    Attr { value, strength: Strength::Soft, weight: Some(0.5), provenance: Some(prov(confidence, evidence)) }
}

/// Recognises what it can on a netlist. `location` names the input in the intent's sources.
pub fn recognise(nl: &Netlist, board: &str, location: &str) -> Recognised {
    let mut kinds = BTreeMap::new();
    for (r, c) in &nl.components {
        kinds.insert(r.as_str(), parts::classify(r, &c.value, &c.footprint));
    }
    let mut nodes: BTreeMap<&str, Vec<&Node>> = BTreeMap::new();
    for (name, net) in &nl.nets {
        let ns: Vec<&Node> = net.nodes.iter().filter(|n| kinds.get(n.reference.as_str()).is_some_and(|k| k.0 != Kind::Mechanical)).collect();
        let distinct: BTreeSet<(&str, &str)> = ns.iter().map(|n| (n.reference.as_str(), n.pin.as_str())).collect();
        if distinct.len() >= 2 {
            nodes.insert(name.as_str(), ns);
        }
    }
    let mut cx = Cx { nl, kinds, nodes, grounds: BTreeSet::new(), guesses: BTreeMap::new(), facts: BTreeMap::new(), blocks: BTreeMap::new(), disputes: Vec::new(), regs: Vec::new() };

    grounds(&mut cx);
    rails_by_name(&mut cx);
    crystals(&mut cx);
    pairs(&mut cx);
    inductors(&mut cx);
    rails_by_structure(&mut cx);
    feedback(&mut cx);
    by_name_and_function(&mut cx);
    small_blocks(&mut cx);
    current::estimate(&mut cx);

    let mut report = Report::default();
    for (k, _) in cx.kinds.values() {
        *report.parts.entry(*k).or_default() += 1;
    }
    report.nets = cx.nodes.len();
    report.named = cx.nodes.keys().filter(|n| !names::is_auto(n)).count();

    let mut nets = BTreeMap::new();
    for (name, g) in &cx.guesses {
        *report.classes.entry(g.class.clone()).or_default() += 1;
        let mut net = NetIntent { class: Some(g.class.clone()), class_provenance: Some(prov(g.confidence, g.evidence.clone())), ..Default::default() };
        if let Some(f) = cx.facts.remove(name) {
            if let Some((v, why)) = f.voltage {
                net.voltage = Some(soft(Voltage { peak_v: v.abs() }, g.confidence, vec![why]));
            }
            if let Some((z, why)) = f.impedance {
                net.impedance = Some(soft(z, g.confidence, vec![why]));
            }
            if let Some((c, conf, why)) = f.current {
                net.current = Some(soft(c, conf, why));
            }
            if let Some((b, why)) = f.bandwidth {
                net.bandwidth = Some(soft(Bandwidth { kind: b, max_hz: None }, g.confidence, vec![why]));
            }
        }
        if g.class == "HV" {
            net.safety = Some(soft(Safety::HighVoltage, g.confidence, vec!["above 50 V".into()]));
        }
        nets.insert(name.clone(), net);
    }
    for b in cx.blocks.values() {
        *report.blocks.entry(b.kind.clone()).or_default() += 1;
    }
    report.disputes = cx.disputes;

    let mut sources = BTreeMap::new();
    sources.insert(SOURCE.to_string(), Source { title: format!("ccomp recognise v{VERSION}"), location: Some(location.into()), revision: None, sha256: None });
    let intent = IntentFile {
        schema_version: SCHEMA_VERSION.into(),
        board: BoardInfo { name: board.into(), kicad_project: None, layers: None, packages: BTreeMap::new() },
        sources,
        nets,
        blocks: cx.blocks,
        domains: None,
        regions: Vec::new(),
        protection_paths: Vec::new(),
        energy_sources: Vec::new(),
    };
    Recognised { intent, report }
}

/// Grounds by name; failing that, the net most capacitors return to.
fn grounds(cx: &mut Cx) {
    let named: Vec<&str> = cx.nodes.keys().copied().filter(|n| names::is_ground(n)).collect();
    for n in &named {
        cx.grounds.insert(n);
        cx.guess(n, "GROUND", 0.95, vec![format!("net name {}", names::norm(n))]);
    }
    if !named.is_empty() {
        return;
    }
    let best = cx.nodes.keys().copied().map(|n| (cx.kinds_on(n, Kind::Capacitor).len(), n)).max();
    if let Some((caps, n)) = best {
        if caps >= 3 {
            cx.grounds.insert(n);
            cx.guess(n, "GROUND", 0.6, vec![format!("no net is named as a ground; {caps} capacitors return to this one, more than to any other")]);
        }
    }
}

fn rails_by_name(cx: &mut Cx) {
    let nets: Vec<&str> = cx.nodes.keys().copied().filter(|n| !cx.grounds.contains(n)).collect();
    for n in nets {
        let v = names::rail_voltage(n);
        let word = names::is_supply_word(n);
        if v.is_none() && !word {
            continue;
        }
        // A name with a voltage in it, but on a net with no capacitor or regulator, may be a
        // signal named after its level (`LED_5V_EN`); a supply word or a decoupling cap settles it.
        let caps = cx.caps_to_ground(n).len();
        let norm = names::norm(n);
        if v.is_some() && !word && caps == 0 && norm.contains(['_']) && !norm.starts_with(['+', '-', 'V']) {
            continue;
        }
        let mut ev = vec![format!("net name {norm}")];
        if caps > 0 {
            ev.push(format!("{caps} capacitor(s) to ground"));
        }
        let conf = match (v.is_some(), caps > 0) {
            (true, true) => 0.95,
            (true, false) | (false, true) => 0.85,
            (false, false) => 0.7,
        };
        let class = if v.is_some_and(|v| v.abs() > 50.0) { "HV" } else { "POWER" };
        cx.guess(n, class, conf, ev);
        if let Some(v) = v {
            cx.facts.entry(n.to_string()).or_default().voltage = Some((v, format!("{v} V from the net name {norm}")));
        }
    }
}

/// Crystals and oscillators: their nets, load capacitors and series resistors.
fn crystals(cx: &mut Cx) {
    let xtals: Vec<&str> = cx.kinds.iter().filter(|(_, k)| k.0 == Kind::Crystal).map(|(r, _)| *r).collect();
    for x in xtals {
        let nets: Vec<&str> = cx.part_nets(x).into_iter().filter(|n| !cx.grounds.contains(n)).collect();
        if nets.is_empty() || nets.len() > 2 {
            continue;
        }
        let why = cx.why(x);
        let mut members = vec![x];
        let mut roles = BTreeMap::new();
        roles.insert("crystal".to_string(), x.to_string());
        for n in &nets {
            cx.guess(n, "CLOCK", 0.9, vec![why.clone()]);
            let caps = cx.caps_to_ground(n);
            members.extend(caps.iter().copied());
            // A series resistor between the crystal and the oscillator pin: its far net is a
            // clock too.
            for r in cx.kinds_on(n, Kind::Resistor) {
                if let Some(far) = cx.other_side(r, n) {
                    if cx.kinds_on(far, Kind::Ic).len() == 1 && cx.parts_on(far).len() == 2 {
                        members.push(r);
                        cx.guess(far, "CLOCK", 0.8, vec![format!("{r} joins crystal {x} to an IC pin")]);
                    }
                }
            }
        }
        let ev = vec![why, format!("{} load or series part(s) on its nets", members.len() - 1)];
        cx.block("crystal", x, members, roles, 0.85, ev);
    }
    let oscs: Vec<&str> = cx.kinds.iter().filter(|(_, k)| k.0 == Kind::Oscillator).map(|(r, _)| *r).collect();
    for o in oscs {
        let why = cx.why(o);
        for n in cx.part_nets(o) {
            if cx.grounds.contains(n) || cx.class_of(n).is_some_and(|c| c == "POWER") || cx.caps_to_ground(n).len() > 0 {
                continue;
            }
            cx.guess(n, "CLOCK", 0.75, vec![why.clone(), "the oscillator's only undecoupled net: its output".into()]);
        }
    }
}

/// Differential pairs by name, with USB recognised from the connector too.
fn pairs(cx: &mut Cx) {
    let named: Vec<&str> = cx.nodes.keys().copied().filter(|n| !names::is_auto(n) && !cx.grounds.contains(n)).collect();
    let mut done = BTreeSet::new();
    for n in named {
        if done.contains(n) || cx.class_of(n).is_some_and(|c| c == "POWER") {
            continue;
        }
        let Some(p) = names::partners(n).into_iter().find(|p| cx.nodes.contains_key(p.as_str())) else { continue };
        let p: &str = cx.nodes.get_key_value(p.as_str()).unwrap().0;
        let usb_conn: Vec<&str> = cx
            .parts_on(n)
            .into_iter()
            .filter(|r| cx.kind(r) == Kind::Connector && cx.parts_on(p).contains(r))
            .filter(|r| {
                let c = &cx.nl.components[*r];
                format!("{} {}", c.value, c.footprint).to_ascii_uppercase().contains("USB")
            })
            .collect();
        let norm = names::norm(n);
        let dlike = ["D+", "D-", "DP", "DM", "DN"].iter().any(|s| norm.ends_with(s));
        let kind = names::pair_kind(n, !usb_conn.is_empty() || dlike);
        let Some(kind) = kind else { continue };
        done.insert(n);
        done.insert(p);
        let mut ev = vec![format!("{} and {} differ only in their pair suffix", names::norm(n), names::norm(p))];
        let mut conf: f64 = if norm.contains("USB") { 0.9 } else { 0.75 };
        if let Some(c) = usb_conn.first() {
            ev.push(format!("both reach USB connector {c}"));
            conf = conf.max(0.85) + 0.05;
        }
        let (ohms, why) = kind.impedance();
        let class = if kind.high_speed() { "DIFF_HS" } else { "SIGNAL" };
        for (a, b) in [(n, p), (p, n)] {
            cx.guess(a, class, conf.min(0.95), ev.clone());
            cx.facts.entry(a.to_string()).or_default().impedance =
                Some((Impedance { ohms, tolerance_pct: 10.0, mode: ImpedanceMode::Differential, pair: Some(b.to_string()) }, format!("{why}; 10 % assumed")));
        }
        if kind == names::PairKind::Usb {
            let mut members: Vec<&str> = usb_conn.clone();
            for net in [n, p] {
                members.extend(cx.kinds_on(net, Kind::Suppressor));
                members.extend(cx.kinds_on(net, Kind::Resistor).into_iter().filter(|r| cx.other_side(r, net).is_some_and(|o| !cx.grounds.contains(o) && cx.class_of(o).is_none_or(|c| c != "POWER"))));
            }
            if !members.is_empty() {
                let anchor = usb_conn.first().copied().unwrap_or(n);
                let anchor = anchor.trim_start_matches('/').replace(['+', '-', '/'], "");
                cx.block("usb", &anchor, members, BTreeMap::new(), conf.min(0.9), ev.clone());
            }
        }
    }
}

/// Inductors: a switch node on one side (no capacitor to ground, an IC, diode or transistor on
/// it) and a decoupled rail on the other is a switching converter.
fn inductors(cx: &mut Cx) {
    let ls: Vec<&str> = cx.kinds.iter().filter(|(_, k)| k.0 == Kind::Inductor).map(|(r, _)| *r).collect();
    for l in ls {
        let nets = cx.part_nets(l);
        if nets.len() != 2 || nets.iter().any(|n| cx.grounds.contains(n)) {
            continue;
        }
        let active = |cx: &Cx, n: &str| cx.parts_on(n).iter().any(|r| matches!(cx.kind(r), Kind::Ic | Kind::Diode | Kind::Transistor));
        let sw: Vec<&str> = nets.iter().copied().filter(|n| cx.caps_to_ground(n).is_empty() && active(cx, n)).collect();
        if sw.len() != 1 {
            continue;
        }
        let sw = sw[0];
        let rail = nets.iter().copied().find(|n| *n != sw).unwrap();
        let cout = cx.caps_to_ground(rail);
        if cout.is_empty() {
            continue;
        }
        // An LC filter into a connector and nothing else, unnamed, is an output stage (a class-D
        // amplifier's speaker filter), not a converter feeding a rail.
        let named_rail = names::rail_voltage(rail).is_some() || names::is_supply_word(rail);
        let feeds_ics = cx.parts_on(rail).iter().any(|r| matches!(cx.kind(r), Kind::Ic | Kind::Led | Kind::Transistor));
        if !named_rail && !feeds_ics && !cx.kinds_on(rail, Kind::Connector).is_empty() {
            continue;
        }
        let mut ev = vec![cx.why(l), format!("{} has no capacitor to ground and drives {l}", names::norm(sw)), format!("{} has {} capacitor(s) to ground", names::norm(rail), cout.len())];
        let mut conf: f64 = 0.8;
        if names::switch_name(sw) || cx.functions(sw).iter().any(|(_, _, f)| names::switch_name(f)) {
            ev.push("named as a switch node".into());
            conf = 0.9;
        }
        // A diode from the switch node to ground is a buck's catch diode; to another decoupled
        // rail, a boost's output diode.
        let mut kind = "buck";
        let mut boost_out = None;
        let mut boost_diode = None;
        let mut buck_in = None;
        let mut members: Vec<&str> = vec![l];
        members.extend(cout.iter().copied());
        for d in cx.kinds_on(sw, Kind::Diode) {
            match cx.other_side(d, sw) {
                Some(o) if cx.grounds.contains(o) => {
                    members.push(d);
                    ev.push(format!("{d} catches the switch node to ground"));
                }
                Some(o) if !cx.caps_to_ground(o).is_empty() => {
                    kind = "boost";
                    boost_out = Some(o);
                    boost_diode = Some(d);
                    members.push(d);
                    ev.push(format!("{d} carries the switch node to {}, a decoupled rail", names::norm(o)));
                }
                _ => {}
            }
        }
        cx.guess(sw, "SWITCH", conf, ev.clone());
        let mut roles = BTreeMap::new();
        roles.insert("inductor".to_string(), l.to_string());
        let ic = cx.kinds_on(sw, Kind::Ic).into_iter().next();
        if let Some(ic) = ic {
            members.push(ic);
            if let Some(n) = cx.nodes[sw].iter().find(|n| n.reference == ic) {
                roles.insert("sw".into(), format!("{ic}.{}", n.pin));
            }
            for g in cx.grounds.clone() {
                if let Some(n) = cx.nodes[g].iter().find(|n| n.reference == ic) {
                    roles.insert("pgnd".into(), format!("{ic}.{}", n.pin));
                }
            }
            // The IC's other decoupled rail is its input.
            if kind == "buck" {
                for n in cx.part_nets(ic) {
                    if n != rail && n != sw && !cx.grounds.contains(n) && cx.caps_to_ground(n).len() > 0 && cx.class_of(n).is_none_or(|c| c == "POWER") {
                        if let Some(node) = cx.nodes[n].iter().find(|x| x.reference == ic) {
                            roles.insert("vin".into(), format!("{ic}.{}", node.pin));
                            let cin = cx.caps_to_ground(n);
                            if let Some(c) = cin.first() {
                                roles.insert("cin".into(), c.to_string());
                            }
                            members.extend(cin);
                            buck_in = Some(n);
                            cx.guess(n, "POWER", 0.7, vec![format!("{ic}'s decoupled input beside switch node {}", names::norm(sw))]);
                            break;
                        }
                    }
                }
            }
        }
        // A buck's inductor side is its output; a boost's is its input, and its output is past
        // the diode. A role whose part isn't found is left out rather than guessed.
        match (boost_out, boost_diode) {
            (Some(o), Some(d)) => {
                if let Some(c) = cout.first() {
                    roles.insert("cin".into(), c.to_string());
                }
                if let Some(n) = cx.nodes[o].iter().find(|n| n.reference == d) {
                    roles.insert("vout".into(), format!("{d}.{}", n.pin));
                }
                let out_caps = cx.caps_to_ground(o);
                if let Some(c) = out_caps.first() {
                    roles.insert("cout".into(), c.to_string());
                }
                members.extend(out_caps);
            }
            _ => {
                if let Some(c) = cout.first() {
                    roles.insert("cout".into(), c.to_string());
                }
            }
        }
        let side = if kind == "buck" { "output" } else { "input" };
        cx.guess(rail, "POWER", 0.75, vec![format!("{kind} {side}: {l} and {} capacitor(s) to ground", cout.len())]);
        let rating = parts::inductor_class(&cx.nl.components[l].footprint).map(|(a, mm)| (a, format!("{l}'s body, about {mm} mm, carries about {a} A")));
        let (input, output) = match (kind, boost_out) {
            ("boost", Some(o)) => (Some(rail.to_string()), o.to_string()),
            _ => (buck_in.map(str::to_string), rail.to_string()),
        };
        cx.regs.push(Reg { part: l.to_string(), switching: true, input, output, rating });
        cx.block(kind, l, members, roles, conf - 0.1, ev);
    }
}

/// Unnamed rails: a net several capacitors decouple to ground that feeds an IC.
fn rails_by_structure(cx: &mut Cx) {
    let nets: Vec<&str> = cx.nodes.keys().copied().filter(|n| !cx.grounds.contains(n) && cx.class_of(n).is_none()).collect();
    for n in nets {
        let caps = cx.caps_to_ground(n);
        let ics = cx.kinds_on(n, Kind::Ic);
        if caps.len() >= 2 && !ics.is_empty() {
            cx.guess(n, "POWER", 0.55, vec![format!("{} capacitors to ground and {} IC pin(s), but no supply name", caps.len(), ics.len())]);
        }
    }
}

/// Feedback dividers: a node of two resistors and an IC pin, one resistor to ground and the other
/// to a rail.
fn feedback(cx: &mut Cx) {
    let nets: Vec<&str> = cx.nodes.keys().copied().filter(|n| !cx.grounds.contains(n) && cx.class_of(n).is_none()).collect();
    for n in nets {
        let parts = cx.parts_on(n);
        let rs = cx.kinds_on(n, Kind::Resistor);
        let ics = cx.kinds_on(n, Kind::Ic);
        let caps = cx.kinds_on(n, Kind::Capacitor);
        if rs.len() != 2 || ics.len() != 1 || parts.len() != 3 + caps.len() {
            continue;
        }
        let far: Vec<Option<&str>> = rs.iter().map(|r| cx.other_side(r, n)).collect();
        let to_ground = far.iter().any(|f| f.is_some_and(|f| cx.grounds.contains(f)));
        let to_rail = far.iter().find_map(|f| f.filter(|f| cx.class_of(f).is_some_and(|c| c == "POWER")));
        let (true, Some(rail)) = (to_ground, to_rail) else { continue };
        let named = names::analog_name(n) == Some("feedback");
        let conf = if named { 0.85 } else { 0.65 };
        let ev = vec![format!("{} and {} divide {} to ground into {}", rs[0], rs[1], names::norm(rail), ics[0])];
        cx.guess(n, "ANALOG", conf, ev);
        cx.facts.entry(n.to_string()).or_default().bandwidth = Some((BandwidthKind::SlowPrecise, "a divider into a regulator or ADC pin".into()));
        // A converter's feedback pin.
        let ic = ics[0];
        let pin = cx.nodes[n].iter().find(|x| x.reference == ic).map(|x| x.pin.clone());
        for b in cx.blocks.values_mut() {
            if matches!(b.kind.as_str(), "buck" | "boost") && b.parts.iter().any(|p| p == ic) {
                if let Some(pin) = &pin {
                    b.roles.insert("fb".into(), format!("{ic}.{pin}"));
                }
                b.parts.extend(rs.iter().map(|r| r.to_string()));
                b.parts.sort();
                b.parts.dedup();
            }
        }
    }
}

/// What names and pin functions say, for nets nothing structural decided.
fn by_name_and_function(cx: &mut Cx) {
    let nets: Vec<&str> = cx.nodes.keys().copied().filter(|n| !cx.grounds.contains(n)).collect();
    for n in nets {
        let mut said: Vec<(String, bool)> = vec![(n.to_string(), true)];
        // A pin with several functions (`GPIO26/ADC0`) says what it can do, not what it does.
        said.extend(cx.functions(n).into_iter().filter(|(_, _, f)| !f.contains('/')).map(|(r, p, f)| (format!("{r}.{p}:{f}"), false)));
        for (s, is_name) in said {
            let text = if is_name { s.clone() } else { s.rsplit(':').next().unwrap().to_string() };
            if !is_name && names::is_auto(&text) {
                continue;
            }
            let what = if is_name { format!("net name {}", names::norm(&text)) } else { format!("pin function {s}") };
            if names::is_auto(&text) {
                continue;
            }
            if names::is_clock_name(&text) {
                cx.guess(n, "CLOCK", 0.75, vec![what.clone()]);
            }
            // A switch node runs into an inductor; `SW` on a net of buttons is a switch of another kind.
            if names::switch_name(&text) && cx.caps_to_ground(n).is_empty() && !cx.kinds_on(n, Kind::Inductor).is_empty() {
                cx.guess(n, "SWITCH", 0.6, vec![what.clone()]);
            }
            if let Some(bus) = names::fast_bus(&text) {
                cx.guess(n, "SE_HS", 0.7, vec![format!("{what}: {bus}")]);
            }
            if let Some(a) = names::analog_name(&text) {
                let conf = if a == "feedback" { 0.7 } else { 0.55 };
                cx.guess(n, "ANALOG", conf, vec![format!("{what}: {a}")]);
                if cx.class_of(n) == Some("ANALOG") {
                    let bw = if a == "analog input" { BandwidthKind::SlowPrecise } else { BandwidthKind::Dc };
                    cx.facts.entry(n.to_string()).or_default().bandwidth.get_or_insert((bw, format!("{a}, from {what}")));
                }
            }
        }
    }
}

/// Blocks a part's value or a two-part structure gives away: regulators, microcontrollers,
/// indicator LEDs, connector protection.
fn small_blocks(cx: &mut Cx) {
    let refs: Vec<&str> = cx.kinds.keys().copied().collect();
    for r in refs {
        let value = cx.nl.components[r].value.clone();
        match cx.kind(r) {
            Kind::Ic if parts::names_family(&value, parts::LDO_FAMILIES) => {
                let mut members = vec![r];
                let mut rails = Vec::new();
                for n in cx.part_nets(r) {
                    if cx.grounds.contains(n) {
                        continue;
                    }
                    let caps = cx.caps_to_ground(n);
                    if !caps.is_empty() {
                        members.extend(caps);
                        rails.push(n);
                        cx.guess(n, "POWER", 0.8, vec![format!("a decoupled pin of regulator {r} ({value})")]);
                    }
                }
                if let Some((input, output)) = ldo_sides(cx, &rails) {
                    let rating = parts::ldo_rating(&value).map(|a| (a, format!("{r} ({value}) is rated {a} A")));
                    cx.regs.push(Reg { part: r.to_string(), switching: false, input: Some(input.to_string()), output: output.to_string(), rating });
                }
                cx.block("ldo", r, members, BTreeMap::new(), 0.85, vec![format!("{r}'s value {value:?} names a linear regulator family")]);
            }
            Kind::Ic if parts::names_family(&value, parts::MCU_FAMILIES) => {
                let mut members = vec![r];
                for n in cx.part_nets(r) {
                    if cx.class_of(n) != Some("POWER") {
                        continue;
                    }
                    // Decoupling that serves this IC alone.
                    for c in cx.caps_to_ground(n) {
                        if cx.kinds_on(n, Kind::Ic).len() == 1 {
                            members.push(c);
                        }
                    }
                }
                cx.block("mcu", r, members, BTreeMap::new(), 0.85, vec![format!("{r}'s value {value:?} names a microcontroller family")]);
            }
            Kind::Led => {
                for n in cx.part_nets(r) {
                    let on = cx.parts_on(n);
                    if on.len() == 2 {
                        if let Some(res) = on.iter().copied().find(|p| cx.kind(p) == Kind::Resistor) {
                            cx.block("indicator", r, vec![r, res], BTreeMap::new(), 0.8, vec![format!("{r} and {res} share net {} and nothing else", names::norm(n))]);
                            break;
                        }
                    }
                }
            }
            Kind::Suppressor => {
                let conns: BTreeSet<&str> = cx.part_nets(r).into_iter().filter(|n| !cx.grounds.contains(n)).flat_map(|n| cx.kinds_on(n, Kind::Connector)).collect();
                if let Some(c) = conns.iter().next() {
                    let mut members = vec![r];
                    members.extend(conns.iter().copied());
                    cx.block("connector_protection", c, members, BTreeMap::new(), 0.7, vec![cx.why(r), format!("it shares a net with connector {c}")]);
                }
            }
            _ => {}
        }
    }
}

/// Which of a linear regulator's two decoupled rails is its input: the higher voltage by name,
/// else the one a connector or an input-like name feeds.
fn ldo_sides<'a>(cx: &Cx, rails: &[&'a str]) -> Option<(&'a str, &'a str)> {
    let [a, b] = rails else { return None };
    match (names::rail_voltage(a), names::rail_voltage(b)) {
        (Some(va), Some(vb)) if va > vb => return Some((a, b)),
        (Some(va), Some(vb)) if vb > va => return Some((b, a)),
        _ => {}
    }
    let inputish = |n: &str| {
        let norm = names::norm(n);
        ["VBUS", "VIN", "VBAT", "BAT", "VUSB", "VRAW", "RAW", "12V", "+5V", "5V", "VSYS"].iter().any(|k| norm.contains(k)) || !cx.kinds_on(n, Kind::Connector).is_empty()
    };
    match (inputish(a), inputish(b)) {
        (true, false) => Some((a, b)),
        (false, true) => Some((b, a)),
        _ => None,
    }
}

// ---------------------------------------------------------------------------------------------
// Comparison against intent a person wrote

#[derive(Debug, Clone, Default, Serialize)]
pub struct ClassScore {
    /// Recognised with this class, and so it is.
    pub agree: usize,
    /// Recognised with this class, but the truth says another (or none).
    pub wrong: usize,
    /// The truth has this class, and the recogniser said nothing or another.
    pub missed: usize,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Comparison {
    pub classes: BTreeMap<String, ClassScore>,
    /// Nets the truth classes and the recogniser left out (SIGNAL is the truth's default, so
    /// these are mostly SIGNAL).
    pub unclassified: BTreeMap<String, usize>,
    /// `net: recognised → truth`, for every disagreement.
    pub disagreements: Vec<String>,
    /// Blocks per type: (recognised, in the truth, recognised whose parts overlap a truth
    /// block of the same type).
    pub blocks: BTreeMap<String, (usize, usize, usize)>,
    /// `(net, recognised continuous A, recognised peak A, truth continuous A)` for every net both
    /// give a current.
    pub currents: Vec<(String, f64, Option<f64>, f64)>,
}

/// Scores a recognised intent against one written by hand, net by net (names compared without a
/// leading `/`). SIGNAL nets the recogniser leaves out count as unclassified, not missed.
pub fn compare(ours: &IntentFile, truth: &IntentFile) -> Comparison {
    let key = |n: &str| n.trim_start_matches('/').to_string();
    let t: BTreeMap<String, Option<&str>> = truth.nets.iter().map(|(n, i)| (key(n), i.class.as_deref())).collect();
    let o: BTreeMap<String, Option<&str>> = ours.nets.iter().map(|(n, i)| (key(n), i.class.as_deref())).collect();
    let mut c = Comparison::default();
    let all: BTreeSet<&String> = t.keys().chain(o.keys()).collect();
    for n in all {
        let (oc, tc) = (o.get(n).copied().flatten(), t.get(n).copied().flatten());
        match (oc, tc) {
            (Some(a), Some(b)) if a == b => c.classes.entry(a.into()).or_default().agree += 1,
            (Some(a), b) => {
                c.classes.entry(a.into()).or_default().wrong += 1;
                if let Some(b) = b {
                    c.classes.entry(b.into()).or_default().missed += 1;
                }
                c.disagreements.push(format!("{n}: {a} → {}", b.unwrap_or("(no class)")));
            }
            (None, Some(b)) if b == "SIGNAL" => *c.unclassified.entry(b.into()).or_default() += 1,
            (None, Some(b)) => {
                c.classes.entry(b.into()).or_default().missed += 1;
                c.disagreements.push(format!("{n}: (none) → {b}"));
            }
            (None, None) => {}
        }
    }
    let tcur: BTreeMap<String, f64> = truth.nets.iter().filter_map(|(n, i)| Some((key(n), i.current.as_ref()?.value.continuous_a))).collect();
    for (n, i) in &ours.nets {
        if let (Some(a), Some(b)) = (i.current.as_ref(), tcur.get(&key(n))) {
            c.currents.push((key(n), a.value.continuous_a, a.value.peak_a, *b));
        }
    }
    let types: BTreeSet<&str> = ours.blocks.values().chain(truth.blocks.values()).map(|b| b.kind.as_str()).collect();
    for ty in types {
        let ob: Vec<&Block> = ours.blocks.values().filter(|b| b.kind == ty).collect();
        let tb: Vec<&Block> = truth.blocks.values().filter(|b| b.kind == ty).collect();
        let hit = ob.iter().filter(|b| tb.iter().any(|t| t.parts.iter().any(|p| b.parts.contains(p)))).count();
        c.blocks.insert(ty.into(), (ob.len(), tb.len(), hit));
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nl(parts: &[(&str, &str, &str)], nets: &[(&str, &[(&str, &str)])]) -> Netlist {
        let mut out = Netlist::default();
        for (r, v, f) in parts {
            out.components.insert(r.to_string(), Component { value: v.to_string(), footprint: f.to_string(), fields: BTreeMap::new() });
        }
        for (n, nodes) in nets {
            out.nets.insert(
                n.to_string(),
                Net { class: None, nodes: nodes.iter().map(|(r, p)| Node { reference: r.to_string(), pin: p.to_string(), function: None }).collect() },
            );
        }
        out
    }

    /// A buck with its catch diode, feedback divider and decoupling, all nets but the rails
    /// unnamed; a crystal; a USB pair; an indicator LED.
    fn board() -> Netlist {
        nl(
            &[
                ("U1", "MP2359", "Package_TO_SOT_SMD:SOT-23-6"),
                ("L1", "10uH", "Inductor_SMD:L_1210"),
                ("D1", "SS14", "Diode_SMD:D_SMA"),
                ("C1", "10u", "Capacitor_SMD:C_0805"),
                ("C2", "22u", "Capacitor_SMD:C_0805"),
                ("C3", "100n", "Capacitor_SMD:C_0402"),
                ("R1", "100k", "Resistor_SMD:R_0402"),
                ("R2", "22k", "Resistor_SMD:R_0402"),
                ("U2", "ATmega328P", "Package_QFP:TQFP-32"),
                ("Y1", "16MHz", "Crystal:Crystal_SMD_3225-4Pin_3.2x2.5mm"),
                ("C4", "22p", "Capacitor_SMD:C_0402"),
                ("C5", "22p", "Capacitor_SMD:C_0402"),
                ("J1", "USB_B_Micro", "Connector_USB:USB_Micro-B"),
                ("D2", "LED", "LED_SMD:LED_0603"),
                ("R3", "1k", "Resistor_SMD:R_0402"),
            ],
            &[
                ("GND", &[("U1", "2"), ("D1", "2"), ("C1", "2"), ("C2", "2"), ("C3", "2"), ("R2", "2"), ("C4", "2"), ("C5", "2"), ("U2", "8"), ("J1", "5"), ("D2", "1")]),
                ("VIN", &[("U1", "5"), ("C1", "1")]),
                ("Net-(U1-SW)", &[("U1", "1"), ("L1", "1"), ("D1", "1")]),
                ("+5V", &[("L1", "2"), ("C2", "1"), ("R1", "1"), ("U2", "7"), ("C3", "1"), ("R3", "1")]),
                ("Net-(U1-FB)", &[("U1", "3"), ("R1", "2"), ("R2", "1")]),
                ("Net-(U2-XTAL1)", &[("U2", "9"), ("Y1", "1"), ("C4", "1")]),
                ("Net-(U2-XTAL2)", &[("U2", "10"), ("Y1", "3"), ("C5", "1")]),
                ("USB_D+", &[("U2", "20"), ("J1", "3")]),
                ("USB_D-", &[("U2", "21"), ("J1", "2")]),
                ("Net-(D2-A)", &[("D2", "2"), ("R3", "2")]),
            ],
        )
    }

    /// A boost: the inductor's side is the input (cin), the output is past the diode (cout,
    /// vout), as in an LED backlight boost off +5V.
    #[test]
    fn boost_roles_take_the_output_past_the_diode() {
        let n = nl(
            &[("U1", "MT3608", "SOT-23-6"), ("L1", "22uH", "L_0805"), ("D1", "SS14", "D_SMA"), ("C1", "10u", "C_0805"), ("C2", "22u", "C_0805"), ("R1", "10R", "R_0603")],
            &[
                ("+5V", &[("L1", "1"), ("C1", "1"), ("U1", "5")]),
                ("Net-(U1-SW)", &[("L1", "2"), ("U1", "1"), ("D1", "2")]),
                ("LEDA", &[("D1", "1"), ("C2", "1"), ("R1", "1")]),
                ("Net-(R1-Pad2)", &[("R1", "2"), ("U1", "3")]),
                ("GND", &[("C1", "2"), ("C2", "2"), ("U1", "2")]),
            ],
        );
        let r = recognise(&n, "t", "test");
        let b = &r.intent.blocks["boost_L1"];
        assert_eq!(b.roles["cin"], "C1");
        assert_eq!(b.roles["cout"], "C2");
        assert_eq!(b.roles["vout"], "D1.1");
        assert_eq!(b.roles["pgnd"], "U1.2");
        assert!(b.parts.contains(&"C2".to_string()));
    }

    #[test]
    fn recognises_a_small_board() {
        let r = recognise(&board(), "t", "test");
        let class = |n: &str| r.intent.nets.get(n).and_then(|n| n.class.clone()).unwrap_or_default();
        assert_eq!(class("GND"), "GROUND");
        assert_eq!(class("+5V"), "POWER");
        assert_eq!(class("VIN"), "POWER");
        assert_eq!(class("Net-(U1-SW)"), "SWITCH");
        assert_eq!(class("Net-(U1-FB)"), "ANALOG");
        assert_eq!(class("Net-(U2-XTAL1)"), "CLOCK");
        assert_eq!(class("USB_D+"), "DIFF_HS");
        assert_eq!(r.intent.nets["USB_D+"].impedance.as_ref().unwrap().value.pair.as_deref(), Some("USB_D-"));
        assert_eq!(r.intent.nets["+5V"].voltage.as_ref().unwrap().value.peak_v, 5.0);

        let buck = &r.intent.blocks["buck_L1"];
        assert_eq!(buck.roles["sw"], "U1.1");
        assert_eq!(buck.roles["fb"], "U1.3");
        assert_eq!(buck.roles["vin"], "U1.5");
        assert!(buck.parts.contains(&"D1".to_string()) && buck.parts.contains(&"R2".to_string()));
        assert!(r.intent.blocks.contains_key("crystal_Y1"));
        assert_eq!(r.intent.blocks["crystal_Y1"].parts, vec!["C4", "C5", "Y1"]);
        assert!(r.intent.blocks.contains_key("mcu_U2"));
        assert!(r.intent.blocks.contains_key("indicator_D2"));
        assert!(r.intent.blocks.contains_key("usb_J1"));

        // Currents: +5V feeds the microcontroller and the LED; the buck's input and switch node
        // carry it (no input voltage named, so taken equal); the ground returns it.
        let amps = |n: &str| r.intent.nets[n].current.as_ref().map(|c| c.value.continuous_a);
        assert_eq!(amps("+5V"), Some(0.06));
        assert_eq!(amps("VIN"), Some(0.06));
        assert_eq!(amps("Net-(U1-SW)"), Some(0.06));
        assert_eq!(amps("GND"), Some(0.06));
        let p = r.intent.nets["+5V"].current.as_ref().unwrap().provenance.as_ref().unwrap();
        assert!(p.evidence.iter().any(|e| e.contains("microcontroller")), "{:?}", p.evidence);

        // Every guess is soft, recognised, and cites its evidence.
        for (n, i) in &r.intent.nets {
            let p = i.class_provenance.as_ref().unwrap();
            assert_eq!(p.kind, ClaimKind::Recognised, "{n}");
            assert!(!p.evidence.is_empty() && p.confidence.is_some(), "{n}");
            assert!(i.voltage.as_ref().is_none_or(|v| v.strength == Strength::Soft));
        }
    }

    #[test]
    fn recognised_intent_has_no_hard_or_provenance_errors() {
        let r = recognise(&board(), "t", "test");
        let report = intent::validate::validate(&r.intent, Some(&board()));
        let bad: Vec<_> = report.findings.iter().filter(|f| matches!(f.code, "hard_from_recognition" | "unknown_source" | "bad_confidence" | "unknown_pad" | "unknown_part" | "unknown_net")).collect();
        assert!(bad.is_empty(), "{bad:?}");
    }

    #[test]
    fn compare_counts_agreement() {
        let r = recognise(&board(), "t", "test");
        let mut truth = r.intent.clone();
        truth.nets.get_mut("VIN").unwrap().class = Some("POWER_HI".into());
        let c = compare(&r.intent, &truth);
        assert_eq!(c.classes["POWER"].wrong, 1);
        assert_eq!(c.classes["POWER_HI"].missed, 1);
        assert_eq!(c.classes["GROUND"].agree, 1);
    }
}
