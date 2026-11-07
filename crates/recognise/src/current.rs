//! Current per rail, estimated from what a rail feeds and what feeds it, never from its copper:
//! a corpus board's track widths are the designer's answer, and the router is scored against them.
//!
//! Rails joined by a ferrite bead, fuse, sense resistor or series diode are one group and carry
//! one current. A group's current is the sum of its loads (a typical figure per part kind) and of
//! the inputs of the regulators it feeds, capped by the ratings of the fuses, regulators and USB
//! connectors that bound it. A group with no recognised load gets its source's rating instead.
//!
//! Connectors are loads too when they hand a rail off the board: a servo or motor, a Raspberry Pi
//! on its header, a plug-in stepper driver, a module on an expansion bus. Which connector is the
//! rail's source and which its loads is judged from what feeds the rail (a regulator's output
//! has none for a source) and from the connectors' shapes.

use super::*;

const CLASSES: &[&str] = &["POWER", "POWER_HI", "HV"];

struct Group {
    nets: Vec<String>,
    /// (amps, why) per load kind, summed.
    loads: BTreeMap<&'static str, (usize, f64)>,
    caps: Vec<(f64, String)>,
    joins: Vec<String>,
}

pub(super) fn estimate(cx: &mut Cx) {
    let power: Vec<String> = cx.guesses.iter().filter(|(_, g)| CLASSES.contains(&g.class.as_str())).map(|(n, _)| n.clone()).collect();
    let index: BTreeMap<&str, usize> = power.iter().enumerate().map(|(i, n)| (n.as_str(), i)).collect();
    let mut parent: Vec<usize> = (0..power.len()).collect();
    fn root(p: &mut [usize], mut i: usize) -> usize {
        while p[i] != i {
            p[i] = p[p[i]];
            i = p[i];
        }
        i
    }

    // Parts that pass a rail's current through rather than draw it.
    let reg_parts: BTreeSet<&str> = cx.regs.iter().map(|r| r.part.as_str()).collect();
    // A converter's own parts draw nothing, except a microcontroller that is also the IC of its
    // core regulator (the RP2350's): it is a load on every rail it touches.
    let converter_parts: BTreeSet<String> = cx
        .blocks
        .values()
        .filter(|b| matches!(b.kind.as_str(), "buck" | "boost" | "ldo"))
        .flat_map(|b| b.parts.iter().cloned())
        .filter(|r| !parts::names_family(&cx.nl.components[r].value, parts::MCU_FAMILIES))
        .collect();
    let mut passes = BTreeSet::new();
    let mut joins: Vec<(usize, String)> = Vec::new();
    let mut caps: Vec<(String, f64, String)> = Vec::new();
    for (r, (kind, _)) in &cx.kinds {
        let value = &cx.nl.components[*r].value;
        let pass = match kind {
            // A transistor between two rails is a load switch or reverse-polarity guard.
            Kind::FerriteBead | Kind::Fuse | Kind::Diode | Kind::Transistor => true,
            Kind::Resistor => parts::si_value(value, true).is_some_and(|v| v <= 1.0),
            Kind::Inductor => !reg_parts.contains(r),
            _ => false,
        };
        if !pass {
            continue;
        }
        let nets: Vec<&str> = cx.part_nets(r).into_iter().filter(|n| !cx.grounds.contains(n)).collect();
        if *kind == Kind::Fuse {
            if let Some(a) = parts::amps(value) {
                for n in &nets {
                    caps.push((n.to_string(), a, format!("fuse {r} ({value}) is rated {a} A")));
                }
            }
        }
        // The two rails it joins; a transistor's third pin is its gate.
        let mut rails: Vec<&str> = nets.iter().copied().filter(|n| index.contains_key(n)).collect();
        // A gate on a rail too (a P-FET switching between USB and battery): in the SOT-23 and
        // SOT-323 pinouts, pin 1 is the gate or base.
        if *kind == Kind::Transistor && rails.len() == 3 {
            rails.retain(|n| !cx.nodes[n].iter().any(|x| x.reference == *r && x.pin == "1"));
        }
        let [a, b] = rails[..] else { continue };
        if *kind != Kind::Transistor && nets.len() != 2 {
            continue;
        }
        let (i, j) = (index[a], index[b]);
        passes.insert(*r);
        let (ri, rj) = (root(&mut parent, i), root(&mut parent, j));
        if ri != rj {
            parent[ri] = rj;
            joins.push((j, format!("{} and {} are one rail through {r}", names::norm(a), names::norm(b))));
        }
    }

    // USB connectors bound the 5 V rail they deliver.
    for (r, (kind, _)) in &cx.kinds {
        if *kind != Kind::Connector {
            continue;
        }
        let c = &cx.nl.components[*r];
        let text = format!("{} {}", c.value, c.footprint).to_ascii_uppercase();
        if !text.contains("USB") {
            continue;
        }
        let (a, what) = if text.contains("USB_C") || text.contains("TYPE-C") || text.contains("USB-C") || text.contains("TYPE_C") { (3.0, "USB-C delivers at most 3 A at 5 V") } else { (0.5, "a USB 2.0 port delivers 500 mA") };
        for n in cx.part_nets(r) {
            if names::rail_voltage(n) == Some(5.0) && index.contains_key(n) {
                caps.push((n.to_string(), a, format!("{r}: {what}")));
            }
        }
    }

    let mut groups: BTreeMap<usize, Group> = BTreeMap::new();
    for (i, n) in power.iter().enumerate() {
        let g = root(&mut parent, i);
        groups.entry(g).or_insert_with(|| Group { nets: Vec::new(), loads: BTreeMap::new(), caps: Vec::new(), joins: Vec::new() }).nets.push(n.clone());
    }
    let group_of: BTreeMap<String, usize> = power.iter().enumerate().map(|(i, n)| (n.clone(), root(&mut parent, i))).collect();
    for (j, why) in joins {
        let g = root(&mut parent, j);
        groups.get_mut(&g).unwrap().joins.push(why);
    }
    for (n, a, why) in caps {
        if let Some(g) = group_of.get(&n) {
            groups.get_mut(g).unwrap().caps.push((a, why));
        }
    }
    for g in groups.values_mut() {
        let mut seen = BTreeSet::new();
        for n in &g.nets {
            for r in cx.parts_on(n) {
                if passes.contains(r) || converter_parts.contains(r) || !seen.insert(r) {
                    continue;
                }
                // An indicator draws through its resistor: count the LED at the resistor's far end.
                let mut drawn: Vec<&str> = vec![r];
                if cx.kind(r) == Kind::Resistor {
                    if let Some(far) = cx.other_side(r, n) {
                        drawn = cx.kinds_on(far, Kind::Led).into_iter().filter(|l| seen.insert(l)).collect();
                    }
                }
                // An FPGA draws its core current from its lowest rail, at 1.2 V or under.
                let core = g.nets.iter().filter_map(|n| names::rail_voltage(n)).any(|v| v > 0.0 && v <= 1.25);
                for d in drawn {
                    let value = &cx.nl.components[d].value;
                    let load = match parts::fpga(value) {
                        Some(c) if core => Some(c),
                        Some(_) => Some((0.05, "an FPGA's I/O and auxiliary supply, about 50 mA")),
                        // Connectors are counted by `offboard_loads`, which knows a source from a load.
                        None if cx.kind(d) == Kind::Connector => None,
                        // A stepper module draws its motor current from its highest rail only.
                        None if parts::STEPPER_MODULES.iter().any(|k| value.to_ascii_uppercase().contains(k)) => {
                            let top = cx.part_nets(d).into_iter().filter_map(|n| names::rail_voltage(n).map(|v| (v, n))).max_by(|a, b| a.0.total_cmp(&b.0));
                            match top {
                                Some((_, n)) if !g.nets.iter().any(|x| x == n) => Some((0.01, "a stepper module's logic supply, about 10 mA")),
                                _ => parts::load(cx.kind(d), value),
                            }
                        }
                        None => parts::load(cx.kind(d), value),
                    };
                    if let Some((a, what)) = load {
                        let e = g.loads.entry(what).or_default();
                        e.0 += 1;
                        e.1 += a;
                    }
                }
            }
        }
    }

    // Off-board loads: a connector on a rail that something else feeds.
    let fed_by_reg: BTreeSet<usize> = cx.regs.iter().filter_map(|r| group_of.get(&r.output).copied()).collect();
    for (g, group) in groups.iter_mut() {
        for (a, what) in offboard_loads(cx, &group.nets, fed_by_reg.contains(g)) {
            let e = group.loads.entry(what).or_default();
            e.0 += 1;
            e.1 += a;
        }
    }

    // Each regulator: the group it draws from and the group it feeds.
    let edges: Vec<(Option<usize>, usize, usize)> = cx
        .regs
        .iter()
        .enumerate()
        .filter_map(|(k, r)| Some((r.input.as_ref().and_then(|n| group_of.get(n).copied()), *group_of.get(&r.output)?, k)))
        .collect();

    let mut memo: BTreeMap<usize, Est> = BTreeMap::new();
    for &g in groups.keys() {
        let mut visiting = BTreeSet::new();
        group_current(cx, g, &groups, &edges, &mut memo, &mut visiting);
    }

    for (g, e) in &memo {
        if e.amps <= 0.0 {
            continue;
        }
        for n in &groups[g].nets {
            let c = Current { continuous_a: round2(e.amps.max(0.01)), peak_a: e.capacity.map(round2) };
            cx.facts.entry(n.clone()).or_default().current = Some((c, e.conf, e.ev.clone()));
        }
    }

    // A switch node carries the inductor's current: a buck's output, a boost's input.
    for r in &cx.regs {
        if !r.switching {
            continue;
        }
        let Some(sw) = cx.part_nets(&r.part).into_iter().find(|n| cx.class_of(n) == Some("SWITCH")) else { continue };
        let side = cx.blocks.get(&format!("boost_{}", r.part)).map_or(Some(r.output.as_str()), |_| r.input.as_deref());
        let Some(e) = side.and_then(|n| group_of.get(n)).and_then(|g| memo.get(g)) else { continue };
        if e.amps > 0.0 {
            let ev = vec![format!("the current of {}, through inductor {}", names::norm(side.unwrap()), r.part)];
            let peak = r.rating.as_ref().map(|x| round2(x.0)).filter(|p| *p > e.amps);
            cx.facts.entry(sw.to_string()).or_default().current = Some((Current { continuous_a: round2(e.amps), peak_a: peak }, e.conf, ev));
        }
    }

    // The ground returns what the board's sources deliver.
    let fed: BTreeSet<usize> = edges.iter().map(|e| e.1).collect();
    let mut total = 0.0;
    let mut roots = Vec::new();
    for (g, e) in &memo {
        if !fed.contains(g) && e.drawn > 0.0 {
            total += e.drawn;
            roots.push(format!("{} {:.2} A", names::norm(&groups[g].nets[0]), e.drawn));
        }
    }
    if total > 0.0 {
        for gnd in cx.grounds.iter().map(|g| g.to_string()).collect::<Vec<_>>() {
            let ev = vec![format!("returns what the board's input rails carry: {}", roots.join(", "))];
            cx.facts.entry(gnd).or_default().current = Some((Current { continuous_a: round2(total), peak_a: None }, 0.3, ev));
        }
    }
}

/// Words in a connector's value or footprint that mark it as where power comes in.
const POWER_ENTRY: &[&str] = &["BARREL", "DC_JACK", "DCJACK", "JACK_DC", "PWR", "POWER", "BATT", "BAT_", "XT30", "XT60", "XT90", "DEANS", "USB", "SCREW", "TERMINAL"];

/// The typical draw of each connector that takes a rail of `nets` off the board. On a rail no
/// regulator feeds, one connector is the source, and is not counted: one named for power entry,
/// else a host header (20 pins or more: the board is a shield, HAT or cape), else the one with the
/// fewest pins. A Raspberry Pi header on a board with its own power entry is the Pi's supply, not
/// the board's.
fn offboard_loads(cx: &Cx, nets: &[String], fed: bool) -> Vec<(f64, &'static str)> {
    let rail: BTreeSet<&str> = nets.iter().map(String::as_str).collect();
    let mut conns: Vec<&str> = Vec::new();
    for n in nets {
        for c in cx.kinds_on(n, Kind::Connector) {
            if !conns.contains(&c) {
                conns.push(c);
            }
        }
    }
    let text = |c: &str| {
        let p = &cx.nl.components[c];
        format!("{} {}", p.value, p.footprint).to_ascii_uppercase()
    };
    let pins = |c: &str| cx.nodes.values().flatten().filter(|x| x.reference == c).count();
    // A pin on a net a transistor switches (its drain, collector): a motor, solenoid or strip.
    let switched = |c: &str| {
        cx.part_nets(c).into_iter().any(|n| !rail.contains(n) && !cx.grounds.contains(n) && !cx.kinds_on(n, Kind::Transistor).is_empty() && cx.parts_on(n).len() <= 4)
    };
    let returns = |c: &str| cx.part_nets(c).into_iter().any(|n| cx.grounds.contains(n));
    let candidates: Vec<&str> = conns.iter().copied().filter(|c| returns(c) || switched(c)).collect();
    let entry = |c: &str| POWER_ENTRY.iter().any(|k| text(c).contains(k));
    // Named for the Pi, or a 2x20 header with half its pins in use (only wired pads are counted).
    let is_pi = |c: &str| ["RPI", "RASPBERRY"].iter().any(|k| text(c).contains(k)) || (text(c).contains("2X20") && pins(c) >= 20);
    let board_entry = cx.kinds.iter().any(|(r, (k, _))| *k == Kind::Connector && entry(r));
    let sources: Vec<&str> = candidates.iter().copied().filter(|c| !(board_entry && is_pi(c))).collect();
    let source = if fed {
        None
    } else {
        sources
            .iter()
            .copied()
            .find(|c| entry(c))
            .or_else(|| sources.iter().copied().filter(|c| pins(c) >= 20).max_by_key(|c| pins(c)))
            .or_else(|| sources.iter().copied().min_by_key(|c| pins(c)))
    };
    let mut out = Vec::new();
    for c in candidates {
        if Some(c) == source {
            continue;
        }
        let t = text(c);
        let n = pins(c);
        let signals = cx.part_nets(c).into_iter().filter(|x| !rail.contains(x) && !cx.grounds.contains(x)).count();
        // A servo is named as one, on the header or its signal net; a bare 3-pin header with one
        // signal is as often a sensor.
        let servo = t.contains("SERVO") || cx.part_nets(c).into_iter().any(|x| !rail.contains(x) && ["SERVO", "PWM", "SRV"].iter().any(|k| names::norm(x).contains(k)));
        out.push(if let Some(l) = parts::load(Kind::Connector, &cx.nl.components[c].value) {
            l
        } else if switched(c) {
            (0.5, "a switched off-board load (a motor or solenoid), about 500 mA")
        } else if servo && n <= 4 {
            (0.2, "a hobby servo, about 200 mA moving")
        } else if is_pi(c) {
            // The Pi draws on its 5 V pins (2 and 4); its 3V3 pins are its own regulator's output.
            if !nets.iter().any(|n| cx.nodes[n.as_str()].iter().any(|x| x.reference == c && (x.pin == "2" || x.pin == "4"))) {
                continue;
            }
            (0.5, "a Raspberry Pi on its 40-pin header, about 500 mA")
        } else if signals > 0 && n >= 4 {
            (0.1, "a module on an expansion header, about 100 mA")
        } else if signals > 0 {
            (0.01, "a sensor on a small header, about 10 mA")
        } else {
            (0.1, "an unknown load on a power output connector, about 100 mA (a guess)")
        });
    }
    out
}

/// A group's estimate: the typical draw summed from its loads, and the most its source allows.
struct Est {
    amps: f64,
    /// The part of `amps` summed from recognised loads: what an upstream rail inherits. A rating
    /// standing in for unknown loads stays on its own rail.
    drawn: f64,
    /// What the fuse, regulator or USB port bounding the rail allows: the figure a designer sizes
    /// copper to, when it is above the typical draw.
    capacity: Option<f64>,
    conf: f64,
    ev: Vec<String>,
}

/// A group's current, memoised; zero on a regulator loop.
fn group_current(cx: &Cx, g: usize, groups: &BTreeMap<usize, Group>, edges: &[(Option<usize>, usize, usize)], memo: &mut BTreeMap<usize, Est>, visiting: &mut BTreeSet<usize>) -> f64 {
    if let Some(m) = memo.get(&g) {
        return m.drawn;
    }
    if !visiting.insert(g) {
        return 0.0;
    }
    let group = &groups[&g];
    let mut ev: Vec<String> = Vec::new();
    let mut amps = 0.0;
    for (what, (n, a)) in &group.loads {
        amps += a;
        ev.push(if *n == 1 { format!("{what}") } else { format!("{n} × {what}") });
    }
    for &(input, output, k) in edges {
        if input != Some(g) || output == g {
            continue;
        }
        let out = group_current(cx, output, groups, edges, memo, visiting);
        if out <= 0.0 {
            continue;
        }
        let r = &cx.regs[k];
        let out_net = &groups[&output].nets[0];
        let vin = group.nets.iter().find_map(|n| names::rail_voltage(n));
        let vout = groups[&output].nets.iter().find_map(|n| names::rail_voltage(n));
        let draw = match (r.switching, vin, vout) {
            (false, _, _) => out,
            (true, Some(vi), Some(vo)) if vi > 0.0 => out * vo.abs() / (vi.abs() * 0.85),
            (true, _, _) => out,
        };
        amps += draw;
        ev.push(format!("feeds {} through {} ({:.2} A out, {:.2} A in)", names::norm(out_net), r.part, out, draw));
    }
    let mut conf = 0.35;
    // A regulator feeding this group bounds it.
    let mut bounds: Vec<(f64, String)> = group.caps.clone();
    for &(_, output, k) in edges {
        if output == g {
            if let Some(r) = &cx.regs[k].rating {
                bounds.push(r.clone());
            }
        }
    }
    let bound = bounds.iter().min_by(|a, b| a.0.total_cmp(&b.0)).cloned();
    let mut capacity = None;
    let mut fallback = 0.0;
    match &bound {
        // Nothing recognised draws from it: the most its source allows, as an upper bound.
        Some((a, why)) if amps <= 0.0 => {
            fallback = *a;
            conf = 0.2;
            ev.push(format!("no load recognised, so the source's rating as an upper bound: {why}"));
        }
        Some((a, why)) if *a < amps => {
            amps = *a;
            ev.push(format!("capped: {why}"));
        }
        Some((a, why)) => {
            capacity = Some(*a);
            ev.push(format!("peak: {why}"));
        }
        None => {}
    }
    ev.extend(group.joins.iter().cloned());
    visiting.remove(&g);
    memo.insert(g, Est { amps: amps.max(fallback), drawn: amps, capacity, conf, ev });
    amps
}
