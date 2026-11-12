//! The supply rule: a regulator for each rail that has loads and nothing feeding it, chosen from
//! the library by checks a person can read (system spec, Supply selection).
//!
//! Every power pin with a contract (`interfaces::power`) is placed on its board net. A net's
//! **range** comes from what feeds it: a `source` (a USB-C sink's VBUS), the composition's
//! `sources` (a cell on a connector), or a regulator, whose output over its input's range is
//! worked out where it leaves regulation: an LDO follows `VIN - dropout`, a buck at 100 % duty and
//! a switch `VIN - I x r_on`, and one that turns off below a voltage is off. A net's **budget** is
//! its consumers' cited draws plus what each regulator it feeds takes from it: its output current
//! (and quiescent current) for an LDO or switch, its output power over its efficiency at its lowest
//! input for a converter. Without a cited efficiency, the converter's input current is the lossless
//! one, a lower bound, and the rule says so. A draw with only a typical figure is counted at that
//! figure for the maximum too, and said. A net fed several ways (a load-sharing path's VSYS, from
//! the input through a diode or the cell through a FET) spans every way's range, and its load is
//! counted on each input, since any one of them may carry it all.
//!
//! For each rail the composition names under `rails` with consumers and no feed, every candidate
//! (each package variant and recipe with a regulating `power_out`) is judged from each input it
//! could take (`from`, or every net with a known range). A candidate fails, with the reason, when:
//! its input's range is outside what it runs on; its maximum current is below the rail's maximum
//! draw; its output, over the input's whole range, leaves a consumer's range; or its dissipation
//! takes its junction past its limit (at `ambient_c`, else 25 C, which is said). The passing ones
//! rank in a fixed order: a verified layout for the board's layer count, then lower loss at
//! typical load at the input's middle voltage (within 10 % is a tie), then fewer parts, then name.
//! The best, or the rail's `supply`, is added as an instance (`as`, else `reg_<rail>`) with its
//! output on the rail, its input on the input net, its ground on the consumers' ground, its enable
//! left open when that holds it on, and its other signal ports marked no-connect.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use compose::{Circuit, Composition};
use interfaces::power::{self, Power, PowerIn, PowerOut};
use interfaces::Ports;
use serde::Serialize;
use serde_json::{json, Value};

use crate::levels::{net_of, range};
use crate::{port_of, Package};

/// A package variant or recipe the rule may place.
pub struct Candidate {
    /// `tlv62569@0.1.0`, or a recipe's name.
    pub spec: String,
    pub variant: String,
    pub recipe: bool,
    pub pkg: Package,
    /// Its verified layouts' names (`sot23-5-4layer`).
    pub layouts: Vec<String>,
}

impl Candidate {
    pub fn name(&self) -> String {
        if self.recipe { self.spec.clone() } else { format!("{} {}", self.spec.split('@').next().unwrap_or(&self.spec), self.variant) }
    }
}

/// Every package variant in `packages` with a regulating output.
pub fn candidates(packages: &Path) -> Result<Vec<Candidate>, String> {
    let mut out = Vec::new();
    let mut dirs: Vec<_> = std::fs::read_dir(packages).map_err(|e| format!("{}: {e}", packages.display()))?.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.join("ports.json").is_file()).collect();
    dirs.sort();
    for d in dirs {
        let ports = interfaces::load_ports(&d.join("ports.json"))?;
        if outputs_of(&ports).next().is_none() {
            continue;
        }
        let manifest = std::fs::read_to_string(d.join("manifest.toml")).map_err(|e| format!("{}: {e}", d.display()))?;
        let Some(version) = manifest.lines().map(str::trim).find_map(|l| l.strip_prefix("version = \"").and_then(|v| v.strip_suffix('"'))) else { continue };
        let name = d.file_name().unwrap().to_string_lossy().to_string();
        let mut layouts: Vec<String> = std::fs::read_dir(d.join("layouts")).into_iter().flatten().filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.join("verify.json").is_file()).map(|p| p.file_name().unwrap().to_string_lossy().to_string()).collect();
        layouts.sort();
        for variant in ports.variants.keys() {
            let p = d.join(format!("schematic/circuit.{variant}.json"));
            let Ok(text) = std::fs::read_to_string(&p) else { continue };
            let circuit: Circuit = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", p.display()))?;
            out.push(Candidate { spec: format!("{name}@{version}"), variant: variant.clone(), recipe: false, pkg: Package { circuit, ports: ports.clone(), volts: BTreeMap::new() }, layouts: layouts.clone() });
        }
    }
    Ok(out)
}

/// The regulating outputs a package declares: not a source, not a charger (a charger's output is
/// a cell, not a rail to feed loads from).
fn outputs_of(ports: &Ports) -> impl Iterator<Item = (&String, PowerOut)> {
    ports.functions.iter().filter_map(|(k, f)| match power::of(f) {
        Ok(Some(Power::Out(o))) if o.topology != "source" && o.topology != "charger" => Some((k, o)),
        _ => None,
    })
}

/// The circuit ports a package's power functions are on: port → (function, contract).
fn contracts(pkg: &Package, variant: &str) -> Vec<(String, String, Power)> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    let variant = if variant.is_empty() { "recipe" } else { variant };
    if let Some(v) = pkg.ports.variants.get(variant) {
        for (num, pin) in &v.pins {
            let Some(fname) = &pin.function else { continue };
            let Some(f) = pkg.ports.functions.get(fname) else { continue };
            let Ok(Some(p)) = power::of(f) else { continue };
            let port = match &p {
                Power::Out(o) if o.port.is_some() => o.port.clone(),
                _ => port_of(&pkg.circuit, num, &pin.name),
            };
            if let Some(port) = port {
                if seen.insert((port.clone(), fname.clone())) {
                    out.push((port, fname.clone(), p));
                }
            }
        }
    }
    // Outputs made by the package circuit, on no pin (a buck past its inductor).
    for (fname, f) in &pkg.ports.functions {
        if let Ok(Some(Power::Out(o))) = power::of(f) {
            if let Some(port) = &o.port {
                if seen.insert((port.clone(), fname.clone())) {
                    out.push((port.clone(), fname.clone(), Power::Out(o)));
                }
            }
        }
    }
    out
}

/// The circuit port a package function's pins are on.
fn port_for(pkg: &Package, variant: &str, function: &str) -> Option<String> {
    let variant = if variant.is_empty() { "recipe" } else { variant };
    let v = pkg.ports.variants.get(variant)?;
    v.pins.iter().filter(|(_, p)| p.function.as_deref() == Some(function)).find_map(|(n, p)| port_of(&pkg.circuit, n, &p.name))
}

#[derive(Debug, Clone, Serialize)]
pub struct Item {
    /// `mcu.VREG_VIN`, or `reg feeding +3V3`.
    pub what: String,
    pub typ_a: f64,
    pub max_a: f64,
    pub note: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Budget {
    pub typ_a: f64,
    pub max_a: f64,
    pub items: Vec<Item>,
}

/// A consumer on a net: where, its range, its draw.
struct Consumer {
    end: String,
    v: [f64; 2],
    draw: Option<interfaces::power::Draw>,
}

/// A regulator on a net's output side.
#[derive(Clone)]
struct Feed {
    end: String,
    out: PowerOut,
    input_net: Option<String>,
    input_v: [f64; 2],
}

/// What the rule judged one option to do.
#[derive(Debug, Clone, Serialize)]
pub struct Judged {
    pub option: String,
    pub input: String,
    pub ok: bool,
    pub reasons: Vec<String>,
    /// Its output's range over the input's.
    pub out_v: [f64; 2],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub loss_w: Option<f64>,
    pub parts: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout: Option<String>,
    pub notes: Vec<String>,
    /// Its input current at the rail's typical and maximum load.
    pub input_a: [f64; 2],
}

#[derive(Debug, Clone, Serialize)]
pub struct Rail {
    pub net: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub v: Option<[f64; 2]>,
    pub fed_by: Vec<String>,
    pub budget: Budget,
    /// The consumers' common range: what any feed must stay inside.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub needs_v: Option<[f64; 2]>,
    /// Each feed judged (a drawn board's own regulators, or the rule's candidates).
    pub options: Vec<Judged>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chosen: Option<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Report {
    pub rails: Vec<Rail>,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
    pub added: Vec<String>,
}

fn fmt_v(v: f64) -> String {
    format!("{:.3}", v).trim_end_matches('0').trim_end_matches('.').to_string()
}

fn fmt_a(a: f64) -> String {
    if a >= 1.0 { format!("{} A", fmt_v(a)) } else if a >= 1e-3 { format!("{} mA", fmt_v(a * 1e3)) } else { format!("{} uA", fmt_v(a * 1e6)) }
}

/// The power picture of a composition: each net with a contract, its range, consumers and feeds.
struct Picture {
    consumers: BTreeMap<String, Vec<Consumer>>,
    feeds: BTreeMap<String, Vec<Feed>>,
    sources: BTreeMap<String, ([f64; 2], Option<f64>, String)>,
    warnings: Vec<String>,
}

fn picture(comp: &Composition, pkgs: &BTreeMap<String, Package>) -> Picture {
    let mut p = Picture { consumers: BTreeMap::new(), feeds: BTreeMap::new(), sources: BTreeMap::new(), warnings: vec![] };
    for (inst, pkg) in pkgs {
        let Some(i) = comp.instances.get(inst) else { continue };
        let cs = contracts(pkg, &i.variant);
        let ins: BTreeMap<&str, &PowerIn> = cs.iter().filter_map(|(_, f, c)| match c { Power::In(x) => Some((f.as_str(), x)), _ => None }).collect();
        for (port, fname, c) in &cs {
            let Some(net) = net_of(comp, &format!("{inst}.{port}")) else { continue };
            match c {
                Power::In(x) => p.consumers.entry(net).or_default().push(Consumer { end: format!("{inst}.{fname}"), v: x.v, draw: x.draw_a.clone() }),
                Power::Out(o) if o.topology == "source" => {
                    p.sources.insert(net, (o.v_out, Some(o.i_max_a), format!("{inst}.{fname}")));
                }
                Power::Out(o) => {
                    let paths = o.paths();
                    let several = paths.len() > 1;
                    for path in paths {
                        let input_port = port_for(pkg, &i.variant, &path.from).or_else(|| pkg.circuit.ports.contains_key(&path.from).then(|| path.from.clone()));
                        let input_net = input_port.and_then(|ip| net_of(comp, &format!("{inst}.{ip}")));
                        let input_v = ins.get(path.from.as_str()).map(|x| x.v).unwrap_or([0.0, f64::INFINITY]);
                        let end = if several { format!("{inst}.{fname} from {}", path.from) } else { format!("{inst}.{fname}") };
                        p.feeds.entry(net.clone()).or_default().push(Feed { end, out: path, input_net, input_v });
                    }
                }
            }
        }
    }
    for (net, s) in &comp.sources {
        match range(s) {
            Some((a, b)) => {
                let kind = s.get("kind").and_then(Value::as_str).unwrap_or("source").to_string();
                p.sources.insert(net.clone(), ([a, b], s.get("i_max_a").and_then(Value::as_f64), format!("{net} ({kind}, sources)")));
            }
            None => p.warnings.push(format!("sources.{net}: give its range as v: [min, max]")),
        }
    }
    p
}

/// The output range of a feed over an input range, at a load; and why it is off, if it is.
fn output(o: &PowerOut, input: [f64; 2], load_a: f64) -> ([f64; 2], Vec<String>) {
    let mut why = Vec::new();
    let pass = matches!(o.topology.as_str(), "switch" | "diode");
    let drop = match (&o.dropout_v, o.r_on_ohm) {
        (Some(d), _) => d.max,
        (None, Some(r)) => load_a * r,
        _ => 0.0,
    };
    let (lo, hi) = if pass {
        (input[0] - drop, input[1])
    } else if o.topology == "boost" {
        (o.v_out[0], o.v_out[1].max(input[1]))
    } else {
        (o.v_out[0].min(input[0] - drop), o.v_out[1].min(input[1]))
    };
    if let Some(off) = o.off_below_v {
        if input[0] < off {
            why.push(format!("turns off below {} V; its input falls to {} V", fmt_v(off), fmt_v(input[0])));
        }
    }
    ([lo, hi.max(lo)], why)
}

/// A feed's input current for an output load.
fn input_current(o: &PowerOut, input: [f64; 2], load_a: f64, notes: &mut Vec<String>) -> f64 {
    let iq = o.iq_a.unwrap_or(0.0);
    match o.topology.as_str() {
        "buck" | "boost" | "buck_boost" => {
            let vout = (o.v_out[0] + o.v_out[1]) / 2.0;
            let eff = o.efficiency.as_ref().map(|e| e.value).unwrap_or_else(|| {
                notes.push("no cited efficiency: its input current is the lossless one, a lower bound".into());
                1.0
            });
            vout * load_a / (eff * input[0].max(0.1)) + iq
        }
        // A charger takes its charge current, whatever the cell's own loads.
        "charger" => o.i_max_a.max(load_a) + iq,
        _ => load_a + iq,
    }
}

/// Judge one feed for a rail: its output against the consumers, its current, its dissipation.
#[allow(clippy::too_many_arguments)]
fn judge(option: String, o: &PowerOut, from_v: [f64; 2], input_net: &str, input: [f64; 2], budget: &Budget, needs: Option<[f64; 2]>, consumers: &[&Consumer], ambient: (f64, bool)) -> Judged {
    let mut reasons = Vec::new();
    let mut notes = Vec::new();
    if input[1] > from_v[1] + 1e-9 {
        reasons.push(format!("{input_net} reaches {} V, above its {} V maximum", fmt_v(input[1]), fmt_v(from_v[1])));
    }
    if input[0] < from_v[0] - 1e-9 {
        reasons.push(format!("{input_net} falls to {} V, below its {} V minimum", fmt_v(input[0]), fmt_v(from_v[0])));
    }
    if o.i_max_a < budget.max_a {
        reasons.push(format!("rated {}, the rail draws up to {}", fmt_a(o.i_max_a), fmt_a(budget.max_a)));
    }
    let (out_v, off) = output(o, input, budget.max_a);
    reasons.extend(off);
    // The binding limit on each side, with every consumer that sets it.
    if let Some(n) = needs {
        let who = |pick: &dyn Fn(&Consumer) -> bool| consumers.iter().filter(|c| pick(c)).map(|c| c.end.clone()).collect::<Vec<_>>().join(", ");
        if out_v[0] < n[0] - 1e-9 {
            let at = if out_v[0] < o.v_out[0] - 1e-9 { format!(" at a {} V input", fmt_v(input[0])) } else { String::new() };
            reasons.push(format!("the output falls to {} V{at}; {} need{} {} V", fmt_v(out_v[0]), who(&|c| (c.v[0] - n[0]).abs() < 1e-9), if consumers.iter().filter(|c| (c.v[0] - n[0]).abs() < 1e-9).count() == 1 { "s" } else { "" }, fmt_v(n[0])));
        }
        if out_v[1] > n[1] + 1e-9 {
            reasons.push(format!("the output reaches {} V; {} take{} at most {} V", fmt_v(out_v[1]), who(&|c| (c.v[1] - n[1]).abs() < 1e-9), if consumers.iter().filter(|c| (c.v[1] - n[1]).abs() < 1e-9).count() == 1 { "s" } else { "" }, fmt_v(n[1])));
        }
    }
    let mut n2 = Vec::new();
    let i_typ = input_current(o, input, budget.typ_a, &mut n2);
    let i_max = input_current(o, input, budget.max_a, &mut n2);
    notes.extend(n2.into_iter().collect::<BTreeSet<_>>());
    // Loss at typical load at the input's middle, and the worst dissipation for the junction.
    let mid = (input[0] + input[1]) / 2.0;
    let vout = (o.v_out[0] + o.v_out[1]) / 2.0;
    let iq = o.iq_a.unwrap_or(0.0);
    let (loss, worst) = match o.topology.as_str() {
        "ldo" => (Some((mid - vout).max(0.0) * budget.typ_a + mid * iq), Some((input[1] - o.v_out[0]).max(0.0) * budget.max_a + input[1] * iq)),
        "switch" | "diode" => {
            let r = o.r_on_ohm.unwrap_or(0.0);
            (Some(budget.typ_a * budget.typ_a * r + mid * iq), Some(budget.max_a * budget.max_a * r))
        }
        _ => match &o.efficiency {
            Some(e) => (Some(vout * budget.typ_a * (1.0 / e.value - 1.0) + mid * iq), Some(vout * budget.max_a * (1.0 / e.value - 1.0))),
            None => (None, None),
        },
    };
    if let (Some(th), Some(tj), Some(p)) = (o.theta_ja_c_per_w, o.tj_max_c, worst) {
        let t = ambient.0 + p * th;
        if !ambient.1 {
            notes.push(format!("junction judged at {} C ambient (give ambient_c)", fmt_v(ambient.0)));
        }
        if t > tj {
            reasons.push(format!("dissipates {} W at full load and the top of its input: the junction reaches {:.0} C, above {:.0} C", fmt_v(p), t, tj));
        }
    } else if o.topology == "ldo" {
        notes.push("dissipation not checked: no cited thermal resistance".into());
    }
    let out_v = out_v.map(|x| (x * 1e4).round() / 1e4);
    Judged { option, input: input_net.to_string(), ok: reasons.is_empty(), reasons, out_v, loss_w: loss, parts: 0, layout: None, notes, input_a: [i_typ, i_max] }
}

/// The rule's own fields on a rail written as an object.
fn rail_field<'a>(comp: &'a Composition, net: &str, k: &str) -> Option<&'a str> {
    comp.rails.get(net).and_then(|v| v.get(k)).and_then(Value::as_str)
}

/// Work out every net's range and budget from the loads up, judge the feeds a composition has,
/// and, with `cands`, fill each named rail that has none. `out` is the composition JSON the
/// filled regulators are written into.
pub fn run(out: &mut Value, comp: &Composition, pkgs: &BTreeMap<String, Package>, cands: Option<&[Candidate]>) -> Report {
    let mut rep = Report::default();
    let pic = picture(comp, pkgs);
    rep.warnings.extend(pic.warnings.iter().cloned());
    let ambient = match out.get("ambient_c").and_then(Value::as_f64) {
        Some(t) => (t, true),
        None => (25.0, false),
    };
    let layers = out.pointer("/intent/layers").and_then(Value::as_u64);

    // Ranges: sources, then feeds whose input is known, until nothing changes.
    let mut ranges: BTreeMap<String, [f64; 2]> = pic.sources.iter().map(|(n, s)| (n.clone(), s.0)).collect();
    // Budgets from the loads up: a net's consumers, then each feed it powers.
    let nets: BTreeSet<String> = pic.consumers.keys().chain(pic.feeds.keys()).chain(pic.sources.keys()).cloned().collect();
    let mut budgets: BTreeMap<String, Budget> = BTreeMap::new();
    fn budget_of(net: &str, pic: &Picture, ranges: &BTreeMap<String, [f64; 2]>, memo: &mut BTreeMap<String, Budget>, depth: usize) -> Budget {
        if let Some(b) = memo.get(net) {
            return b.clone();
        }
        let mut b = Budget::default();
        for c in pic.consumers.get(net).into_iter().flatten() {
            match &c.draw {
                Some(d) => {
                    let typ = d.typ.or(d.max).unwrap_or(0.0);
                    let max = d.max.unwrap_or(typ);
                    let note = if d.max.is_none() { format!("{} (typical only, so the maximum is the typical)", d.when) } else { d.when.clone() };
                    b.items.push(Item { what: c.end.clone(), typ_a: typ, max_a: max, note });
                }
                None => b.items.push(Item { what: c.end.clone(), typ_a: 0.0, max_a: 0.0, note: "no cited draw".into() }),
            }
        }
        if depth < 8 {
            for (onet, feeds) in &pic.feeds {
                for f in feeds.iter().filter(|f| f.input_net.as_deref() == Some(net)) {
                    let ob = budget_of(onet, pic, ranges, memo, depth + 1);
                    let input = ranges.get(net).copied().unwrap_or(f.input_v);
                    let mut notes = Vec::new();
                    let typ = input_current(&f.out, input, ob.typ_a, &mut notes);
                    let max = input_current(&f.out, input, ob.max_a, &mut notes);
                    let mut note = format!("feeds {onet}");
                    if let Some(n) = notes.first() {
                        note = format!("{note}; {n}");
                    }
                    b.items.push(Item { what: f.end.clone(), typ_a: typ, max_a: max, note });
                }
            }
        }
        b.typ_a = b.items.iter().map(|i| i.typ_a).sum();
        b.max_a = b.items.iter().map(|i| i.max_a).sum();
        memo.insert(net.to_string(), b.clone());
        b
    }
    for _ in 0..8 {
        let mut changed = false;
        budgets.clear();
        // A net fed several ways (a source and a charger, a diode and a FET) spans them all.
        for (net, feeds) in &pic.feeds {
            let b = budget_of(net, &pic, &ranges, &mut budgets, 0);
            let mut v: Option<[f64; 2]> = pic.sources.get(net).map(|s| s.0);
            for f in feeds {
                let Some(inp) = f.input_net.as_ref().and_then(|n| ranges.get(n)).copied() else { continue };
                let (o, _) = output(&f.out, inp, b.max_a);
                v = Some(match v {
                    None => o,
                    Some(a) => [a[0].min(o[0]), a[1].max(o[1])],
                });
            }
            if let Some(v) = v {
                if ranges.get(net) != Some(&v) {
                    ranges.insert(net.clone(), v);
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    budgets.clear();
    for net in &nets {
        budget_of(net, &pic, &ranges, &mut budgets, 0);
    }

    // Each net with consumers: its feeds judged, or a named rail filled.
    let mut added_insts: BTreeSet<String> = comp.instances.keys().cloned().collect();
    for net in pic.consumers.keys().chain(comp.rails.keys().filter(|n| !pic.consumers.contains_key(*n))).cloned().collect::<BTreeSet<_>>() {
        let consumers: Vec<&Consumer> = pic.consumers.get(&net).into_iter().flatten().collect();
        let budget = budgets.get(&net).cloned().unwrap_or_default();
        let needs = consumers.iter().fold(None, |acc: Option<[f64; 2]>, c| Some(match acc {
            None => c.v,
            Some(a) => [a[0].max(c.v[0]), a[1].min(c.v[1])],
        }));
        let mut rail = Rail { net: net.clone(), v: ranges.get(&net).map(|v| v.map(|x| (x * 1e4).round() / 1e4)), fed_by: vec![], budget: budget.clone(), needs_v: needs, options: vec![], chosen: None, notes: vec![] };
        if let Some(n) = needs.filter(|n| n[0] > n[1]) {
            rep.errors.push(format!("{net}: its consumers' ranges don't overlap ({} V needed, {} V the most taken)", fmt_v(n[0]), fmt_v(n[1])));
        }
        if let Some(s) = pic.sources.get(&net) {
            rail.fed_by.push(s.2.clone());
            if let Some(i) = s.1.filter(|i| *i < budget.max_a) {
                rep.errors.push(format!("{net}: {} gives at most {}, the rail draws up to {}", s.2, fmt_a(i), fmt_a(budget.max_a)));
            }
            for c in &consumers {
                if s.0[0] < c.v[0] - 1e-9 || s.0[1] > c.v[1] + 1e-9 {
                    rep.errors.push(format!("{net}: {} needs {}-{} V; {} gives {}-{} V", c.end, fmt_v(c.v[0]), fmt_v(c.v[1]), s.2, fmt_v(s.0[0]), fmt_v(s.0[1])));
                }
            }
        }
        for f in pic.feeds.get(&net).into_iter().flatten() {
            rail.fed_by.push(f.end.clone());
            let Some(inet) = &f.input_net else {
                rail.notes.push(format!("{}: its input is on no net", f.end));
                continue;
            };
            let Some(inp) = ranges.get(inet).copied() else {
                rail.notes.push(format!("{}: {inet}'s range isn't known (a source, or sources.{inet})", f.end));
                continue;
            };
            let j = judge(f.end.clone(), &f.out, f.input_v, inet, inp, &budget, needs, &consumers, ambient);
            if !j.ok {
                rep.errors.extend(j.reasons.iter().map(|r| format!("{net}: {}: {r}", f.end)));
            }
            rail.options.push(j);
        }
        let named = comp.rails.contains_key(&net);
        if rail.fed_by.is_empty() && named && !consumers.is_empty() {
            match cands {
                Some(cands) => fill(out, comp, pkgs, &net, &mut rail, &consumers, &budget, needs, &ranges, cands, ambient, layers, &mut added_insts, &mut rep),
                None => rail.notes.push("nothing feeds it".into()),
            }
        } else if rail.fed_by.is_empty() && !consumers.is_empty() && budget.max_a > 0.0 {
            rep.warnings.push(format!("{net}: nothing with a power contract feeds it: give its range under sources if it comes from off the board, or name it under rails for the supply rule to fill"));
        }
        rep.rails.push(rail);
    }
    rep
}

#[allow(clippy::too_many_arguments)]
fn fill(out: &mut Value, comp: &Composition, pkgs: &BTreeMap<String, Package>, net: &str, rail: &mut Rail, consumers: &[&Consumer], budget: &Budget, needs: Option<[f64; 2]>, ranges: &BTreeMap<String, [f64; 2]>, cands: &[Candidate], ambient: (f64, bool), layers: Option<u64>, taken: &mut BTreeSet<String>, rep: &mut Report) {
    let inputs: Vec<String> = match rail_field(comp, net, "from") {
        Some(f) => vec![f.to_string()],
        None => ranges.keys().filter(|n| *n != net).cloned().collect(),
    };
    if inputs.is_empty() {
        rail.notes.push("no net with a known range to feed it from (a source, or sources)".into());
    }
    // Each candidate's best input.
    let mut options: Vec<(usize, Judged)> = Vec::new();
    for (ci, c) in cands.iter().enumerate() {
        let ins: BTreeMap<String, PowerIn> = c.pkg.ports.functions.iter().filter_map(|(k, f)| match power::of(f) { Ok(Some(Power::In(x))) => Some((k.clone(), x)), _ => None }).collect();
        for (fname, o) in outputs_of(&c.pkg.ports) {
            let from_v = ins.get(&o.from).map(|x| x.v).unwrap_or([0.0, f64::INFINITY]);
            let mut best: Option<Judged> = None;
            for inet in &inputs {
                let Some(inp) = ranges.get(inet).copied() else {
                    rail.notes.push(format!("{inet}'s range isn't known (a source, or sources.{inet})"));
                    continue;
                };
                let mut j = judge(format!("{} ({fname})", c.name()), &o, from_v, inet, inp, budget, needs, consumers, ambient);
                j.parts = c.pkg.circuit.parts.len();
                j.layout = layers.and_then(|l| c.layouts.iter().find(|n| n.contains(&format!("{l}layer"))).cloned());
                let better = match &best {
                    None => true,
                    Some(b) => (j.ok && !b.ok) || (j.ok == b.ok && j.loss_w.unwrap_or(f64::INFINITY) < b.loss_w.unwrap_or(f64::INFINITY)),
                };
                if better {
                    best = Some(j);
                }
            }
            if let Some(j) = best {
                options.push((ci, j));
            }
        }
    }
    // The fixed order: passes, layout, loss (10 % buckets, unknown last), parts, name.
    let bucket = |l: Option<f64>| l.map(|w| ((w * 1e3).max(1e-6).ln() / 1.1f64.ln()).floor() as i64).unwrap_or(i64::MAX);
    options.sort_by(|(_, a), (_, b)| (!a.ok, a.layout.is_none(), bucket(a.loss_w), a.parts, a.option.clone()).cmp(&(!b.ok, b.layout.is_none(), bucket(b.loss_w), b.parts, b.option.clone())));
    let pinned = rail_field(comp, net, "supply");
    let pick = match pinned {
        Some(p) => options.iter().position(|(ci, _)| cands[*ci].spec.split('@').next() == Some(p) || cands[*ci].spec == p || cands[*ci].name() == p),
        None => options.iter().position(|(_, j)| j.ok),
    };
    rail.options = options.iter().map(|(_, j)| j.clone()).collect();
    let Some(k) = pick else {
        match pinned {
            Some(p) => rep.errors.push(format!("{net}: its supply {p} isn't a candidate ({})", options.iter().map(|(_, j)| j.option.clone()).collect::<Vec<_>>().join(", "))),
            None => rep.errors.push(format!("{net}: no candidate passes; {}", options.iter().map(|(_, j)| format!("{}: {}", j.option, j.reasons.join("; "))).collect::<Vec<_>>().join(" | "))),
        }
        return;
    };
    let (ci, j) = options[k].clone();
    let c = &cands[ci];
    if !j.ok {
        rep.errors.extend(j.reasons.iter().map(|r| format!("{net}: {} (chosen under supply): {r}", j.option)));
    }
    let mut name = rail_field(comp, net, "as").map(str::to_string).unwrap_or_else(|| format!("reg_{}", net.trim_start_matches(['+', '/']).to_lowercase().replace(|ch: char| !ch.is_ascii_alphanumeric(), "_")));
    while taken.contains(&name) {
        name.push('_');
    }
    taken.insert(name.clone());
    rail.chosen = Some(format!("{name}: {}", j.option));

    // Wire it: output, input, ground, enable; other signal ports open.
    let (ofunc, o) = outputs_of(&c.pkg.ports).find(|(f, _)| j.option.ends_with(&format!("({f})"))).map(|(f, o)| (f.clone(), o)).expect("the judged output");
    let oport = o.port.clone().or_else(|| port_for(&c.pkg, &c.variant, &ofunc)).unwrap_or(ofunc.clone());
    let iport = port_for(&c.pkg, &c.variant, &o.from).unwrap_or(o.from.clone());
    let gnd_net = consumers.iter().find_map(|cn| {
        let inst = cn.end.split('.').next()?;
        let pkg = pkgs.get(inst)?;
        pkg.circuit.ports.iter().filter(|(_, p)| p.kind == "ground").find_map(|(gp, _)| net_of(comp, &format!("{inst}.{gp}")))
    }).or_else(|| comp.nets.keys().find(|n| n.as_str() == "GND").cloned());
    let mut ends: Vec<(String, String)> = vec![(oport.clone(), net.to_string()), (iport.clone(), j.input.clone())];
    let mut open = Vec::new();
    for (pname, port) in &c.pkg.circuit.ports {
        if *pname == oport || *pname == iport {
            continue;
        }
        if port.kind == "ground" {
            match &gnd_net {
                Some(g) => ends.push((pname.clone(), g.clone())),
                None => rep.errors.push(format!("{net}: {name}.{pname} needs a ground net, and none was found")),
            }
            continue;
        }
        let en = o.enable.as_ref().filter(|e| port_for(&c.pkg, &c.variant, &e.function).as_deref() == Some(pname.as_str()) || &e.function == pname);
        match en {
            Some(e) if e.on_when_open => open.push(pname.clone()),
            Some(e) => match e.tie.as_ref().and_then(|t| port_for(&c.pkg, &c.variant, t)) {
                Some(tp) if tp == iport => ends.push((pname.clone(), j.input.clone())),
                _ => rep.errors.push(format!("{net}: {name}.{pname} turns it on and must be driven; connect it")),
            },
            None if port.kind == "supply" => rep.errors.push(format!("{net}: {name}.{pname} is a supply port the rule doesn't wire; connect it")),
            None => open.push(pname.clone()),
        }
    }
    let mut inst = json!({ "package": c.spec, "variant": c.variant });
    if c.recipe {
        inst = json!({ "recipe": c.spec });
    }
    out["instances"][&name] = inst;
    for (port, n) in &ends {
        let e = Value::String(format!("{name}.{port}"));
        match out["nets"].get_mut(n).and_then(Value::as_array_mut) {
            Some(a) => a.push(e),
            None => out["nets"][n] = json!([e]),
        }
    }
    if !open.is_empty() {
        let nc = out.as_object_mut().unwrap().entry("no_connect").or_insert(json!([]));
        if let Some(a) = nc.as_array_mut() {
            a.extend(open.iter().map(|p| Value::String(format!("{name}.{p}"))));
        }
    }
    rep.added.push(format!(
        "{net}: {name} ({}) from {}, out {}-{} V, {} of {} rated, {} other option{}",
        j.option,
        j.input,
        fmt_v(j.out_v[0]),
        fmt_v(j.out_v[1]),
        fmt_a(budget.max_a),
        fmt_a(o.i_max_a),
        options.len() - 1,
        if options.len() == 2 { "" } else { "s" }
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_ldo_follows_its_input_below_dropout() {
        let o: PowerOut = serde_json::from_value(json!({"from": "IN", "topology": "ldo", "v_out": [3.247, 3.373], "i_max_a": 0.15, "dropout_v": {"max": 0.48, "at_a": 0.15}, "citation": {"source": "s", "locator": "p"}})).unwrap();
        let (v, why) = output(&o, [3.0, 4.2], 0.1);
        assert!((v[0] - 2.52).abs() < 1e-9 && (v[1] - 3.373).abs() < 1e-9, "{v:?}");
        assert!(why.is_empty());
        let (v, _) = output(&o, [4.75, 5.5], 0.1);
        assert_eq!(v, [3.247, 3.373]);
    }

    #[test]
    fn a_buck_at_full_duty_drops_its_current_times_r_on() {
        let o: PowerOut = serde_json::from_value(json!({"from": "VIN", "topology": "buck", "v_out": [3.198, 3.441], "i_max_a": 2.0, "r_on_ohm": 0.152, "efficiency": {"value": 0.94, "at_a": 0.1, "vin": 5.0}, "citation": {"source": "s", "locator": "p"}})).unwrap();
        let (v, _) = output(&o, [3.0, 4.2], 0.5);
        assert!((v[0] - 2.924).abs() < 1e-9, "{v:?}");
        let mut notes = vec![];
        // 3.3195 V x 0.5 A / (0.94 x 3.0 V)
        assert!((input_current(&o, [3.0, 4.2], 0.5, &mut notes) - 3.3195 * 0.5 / (0.94 * 3.0)).abs() < 1e-6);
        assert!(notes.is_empty());
    }
}
