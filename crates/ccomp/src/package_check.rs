//! `ccomp check package`: a board against a package's blocks, by role.
//!
//! A binding names which of the board's parts plays each package part (`CIN` → `C14`) and which
//! variant the chip is. The package's nets (`schematic/nets.json`) are found on the board through
//! their pins, signal nets' wiring is compared pin by pin, and then every block constraint, net
//! length limit and part-value check in the package is measured and judged. A package part with
//! no board part bound to it is reported as unbound, not guessed.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use pcb::Board;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use intent::{BoundOp, IntentFile, Metric, Strength};

/// Which board part plays each package part.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    /// `name@version`; the version must match the package's.
    pub package: String,
    /// A variant of the package's `ports.json` (`RP2350A`).
    pub variant: String,
    /// The chip's reference on the board.
    pub chip: String,
    /// Package part name → board reference.
    pub parts: BTreeMap<String, String>,
    /// Free text for people; no tool reads it.
    #[serde(default)]
    #[allow(dead_code)]
    pub note: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Finding {
    /// `wiring`, `constraint`, `length` or `value`.
    pub kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block: Option<String>,
    /// The package net or part judged.
    pub subject: String,
    /// The metric or quantity.
    pub check: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub measured: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<String>,
    pub strength: &'static str,
    /// `pass`, `fail`, `not_applicable`, `unbound` or `unverifiable`.
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// The constraint's provenance or the claim's id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub ok: bool,
    pub package: String,
    pub variant: String,
    /// Package net → board net.
    pub nets: BTreeMap<String, String>,
    /// Package parts with no board part bound.
    pub unbound: Vec<String>,
    /// Two-pin resistors and capacitors fitted the other way round from the package; fine.
    pub reversed: Vec<String>,
    pub counts: BTreeMap<&'static str, usize>,
    pub findings: Vec<Finding>,
}

struct Package {
    intent: IntentFile,
    nets: BTreeMap<String, PackageNet>,
    frame_variant: String,
    rules: Value,
    ports: Value,
}

#[derive(Deserialize)]
struct NetsFile {
    frame_variant: String,
    nets: BTreeMap<String, PackageNet>,
}

#[derive(Deserialize)]
struct PackageNet {
    anchor: String,
    pins: Vec<String>,
    #[serde(default)]
    length: Option<LengthPair>,
}

#[derive(Deserialize)]
struct LengthPair {
    from: String,
    to: String,
}

fn read_json(path: &Path) -> Result<Value, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

fn load_package(dir: &Path) -> Result<Package, String> {
    let intent = intent::load(&dir.join("intent.json"))?;
    let nets_path = dir.join("schematic/nets.json");
    let nets: NetsFile = serde_json::from_value(read_json(&nets_path)?).map_err(|e| format!("{}: {e}", nets_path.display()))?;
    Ok(Package { intent, nets: nets.nets, frame_variant: nets.frame_variant, rules: read_json(&dir.join("rules.json"))?, ports: read_json(&dir.join("ports.json"))? })
}

struct Ctx<'a> {
    board: &'a Board,
    pkg: &'a Package,
    bind: &'a Binding,
    /// Two-pin resistors and capacitors fitted the other way round from the package.
    swapped: BTreeSet<String>,
}

impl Ctx<'_> {
    /// A package pin (`U1.21`, `XTAL.1`) as a board pin, when its part is bound.
    fn pin(&self, package_pin: &str) -> Option<String> {
        let (part, pin) = package_pin.split_once('.')?;
        if part == "U1" {
            return Some(format!("{}.{}", self.bind.chip, self.chip_pin(pin)?));
        }
        let pin = match (self.swapped.contains(part), pin) {
            (true, "1") => "2",
            (true, "2") => "1",
            _ => pin,
        };
        Some(format!("{}.{pin}", self.bind.parts.get(part)?))
    }

    /// A bound two-pin resistor or capacitor, whose pins are interchangeable. Inductors are
    /// not: the RP2350's has a marked side (`rp2350.vreg.inductor_orientation`).
    fn reversible(&self, package_part: &str) -> bool {
        self.bind.parts.get(package_part).and_then(|r| self.board.parts.get(r)).is_some_and(|p| p.pads.len() == 2 && (p.reference.starts_with('R') || p.reference.starts_with('C')))
    }

    /// Package nets on the board: the anchor's net, or the first bound pin's. With `fixed_only`,
    /// only from pins whose orientation is not in question.
    fn resolve(&self, fixed_only: bool) -> BTreeMap<String, String> {
        let mut nets = BTreeMap::new();
        for (net, n) in &self.pkg.nets {
            let found = std::iter::once(&n.anchor)
                .chain(&n.pins)
                .filter(|p| !fixed_only || !self.reversible(p.split_once('.').map_or("", |s| s.0)))
                .filter_map(|p| self.pin(p))
                .find_map(|p| self.board.pad(&p).map(|pad| pad.net.clone()));
            if let Some(b) = found {
                nets.insert(net.clone(), b);
            }
        }
        nets
    }

    /// A chip pin number in the frame variant, translated by name to the board's variant. A
    /// name several pins share (IOVDD) goes to the lowest-numbered of them: same net.
    fn chip_pin(&self, pin: &str) -> Option<String> {
        if self.bind.variant == self.pkg.frame_variant {
            return Some(pin.to_string());
        }
        let pins = |v: &str| self.pkg.ports["variants"][v]["pins"].as_object().cloned();
        let name = pins(&self.pkg.frame_variant)?.get(pin)?["name"].as_str()?.to_string();
        let mut nums: Vec<(u64, String)> = pins(&self.bind.variant)?
            .iter()
            .filter(|(_, p)| p["name"].as_str() == Some(name.as_str()))
            .map(|(n, _)| (n.parse().unwrap_or(u64::MAX), n.clone()))
            .collect();
        nums.sort();
        nums.into_iter().next().map(|(_, n)| n)
    }

    fn part(&self, package_part: &str) -> Option<&pcb::Part> {
        let r = if package_part == "U1" { &self.bind.chip } else { self.bind.parts.get(package_part)? };
        self.board.parts.get(r)
    }
}

pub fn check(board_path: &Path, package_dir: &Path, binding_path: &Path) -> Result<Report, String> {
    let board = Board::load(board_path)?;
    let pkg = load_package(package_dir)?;
    let bind: Binding = serde_json::from_value(read_json(binding_path)?).map_err(|e| format!("{}: {e}", binding_path.display()))?;
    let version = pkg.rules["version"].as_str().unwrap_or_default();
    let name = pkg.rules["package"].as_str().unwrap_or_default();
    if bind.package != format!("{name}@{version}") {
        return Err(format!("the binding is for {}, the package is {name}@{version}", bind.package));
    }
    if pkg.ports["variants"][&bind.variant].is_null() {
        return Err(format!("no variant `{}` in the package", bind.variant));
    }
    if !board.parts.contains_key(&bind.chip) {
        return Err(format!("the chip {} is not on the board", bind.chip));
    }
    for (p, r) in &bind.parts {
        if !board.parts.contains_key(r) {
            return Err(format!("{p} is bound to {r}, which is not on the board"));
        }
    }
    let mut cx = Ctx { board: &board, pkg: &pkg, bind: &bind, swapped: BTreeSet::new() };
    let mut findings = Vec::new();

    // Nets found through fixed pins first; they say which way round each resistor and capacitor
    // is fitted; then every net, with those parts' pins read the right way.
    let fixed = cx.resolve(true);
    let mut swapped = BTreeSet::new();
    for part in bind.parts.keys().filter(|p| cx.reversible(p)) {
        let flipped = pkg.nets.iter().any(|(net, n)| {
            let Some(want) = fixed.get(net) else { return false };
            n.pins.iter().filter_map(|p| p.split_once('.')).filter(|(q, _)| q == part).any(|(_, pin)| {
                let other = if pin == "1" { "2" } else { "1" };
                let on = |pin: &str| board.pad(&format!("{}.{pin}", bind.parts[part])).map(|p| &p.net);
                on(pin) != Some(want) && on(other) == Some(want)
            })
        });
        if flipped {
            swapped.insert(part.clone());
        }
    }
    cx.swapped = swapped;
    let nets = cx.resolve(false);

    // Wiring: every bound pin of a signal net sits on the net its anchor found. Power and ground
    // nets merge on real boards (VREG_VIN and IOVDD are one +3V3), so only their anchor counts.
    let mut unbound: BTreeSet<String> = BTreeSet::new();
    for (net, n) in &pkg.nets {
        let class = pkg.intent.nets.get(net).and_then(|i| i.class.as_deref()).unwrap_or("SIGNAL");
        let Some(board_net) = nets.get(net) else {
            findings.push(finding("wiring", None, net, "net", "hard", "unbound", Some("no pin of this net is bound to a board part".into())));
            continue;
        };
        for p in &n.pins {
            let part = p.split_once('.').map_or("", |s| s.0);
            let Some(bp) = cx.pin(p) else {
                unbound.insert(part.to_string());
                continue;
            };
            if matches!(class, "POWER" | "POWER_HI" | "GROUND") {
                continue;
            }
            match board.pad(&bp) {
                None => findings.push(finding("wiring", None, net, "pin", "hard", "fail", Some(format!("{p} is bound to {bp}, which has no such pad")))),
                Some(pad) if &pad.net != board_net => findings.push(finding(
                    "wiring",
                    None,
                    net,
                    "pin",
                    "hard",
                    "fail",
                    Some(format!("{p} ({bp}) is on {}, the net's other pins on {board_net}", pad.net)),
                )),
                Some(_) => {}
            }
        }
    }
    for blk in pkg.intent.blocks.values() {
        for p in &blk.parts {
            if !bind.parts.contains_key(p) {
                unbound.insert(p.clone());
            }
        }
    }
    unbound.remove("U1");

    // Block constraints.
    let ground = nets.get("GND").cloned().unwrap_or_else(|| "GND".into());
    let layers = board.layers.len();
    let mut buck_cache: BTreeMap<String, Result<verify::buck::BuckReport, String>> = BTreeMap::new();
    for (id, blk) in &pkg.intent.blocks {
        for c in blk.placement_goal.iter().flat_map(|g| &g.constraints) {
            let m = &c.value;
            let metric = serde_json::to_value(m.metric).unwrap().as_str().unwrap_or_default().to_string();
            let strength = strength(c.strength);
            let source = c.provenance.as_ref().map(|p| format!("{} {}", p.source, p.locator.as_deref().unwrap_or("")).trim().to_string());
            let limit = verify::Limit { op: op(m.op), value: m.limit };
            let mut push = |subject: String, measured: Option<f64>, unit: &'static str, status: &'static str, detail: Option<String>| {
                findings.push(Finding {
                    kind: "constraint",
                    block: Some(id.clone()),
                    subject,
                    check: metric.clone(),
                    measured,
                    unit: Some(unit),
                    limit: Some(format!("{} {}", limit.op, limit.value)),
                    strength,
                    status,
                    detail,
                    source: source.clone(),
                })
            };
            if let Some(claim) = layer_scope(&pkg.rules, &metric, layers) {
                push(m.nets.join(", "), None, unit_of(m.metric), "not_applicable", Some(format!("claim {claim} applies on boards with more copper layers than this one's {layers}")));
                continue;
            }
            match m.metric {
                Metric::ViaCount | Metric::UnreferencedLengthMm => {
                    for net in &m.nets {
                        let Some(b) = nets.get(net) else {
                            push(net.clone(), None, unit_of(m.metric), "unbound", Some("net not found on the board".into()));
                            continue;
                        };
                        let (v, detail) = if m.metric == Metric::ViaCount {
                            (board.vias.iter().filter(|v| &v.net == b).count() as f64, format!("board net {b}"))
                        } else {
                            let r = verify::nets::return_path(&board, b, &ground, 0.05);
                            let over: Vec<String> = r.over_other.iter().map(|(n, mm)| format!("{mm:.2} mm over {n}")).collect();
                            let detail = if over.is_empty() { format!("board net {b}") } else { format!("board net {b}; {}", over.join(", ")) };
                            (r.unreferenced_mm, detail)
                        };
                        push(net.clone(), Some(round(v)), unit_of(m.metric), judged(v, limit), Some(detail));
                    }
                }
                Metric::PartDistanceMm => {
                    let parts: Vec<_> = m.parts.iter().map(|p| (p, cx.part(p))).collect();
                    match parts.as_slice() {
                        [(_, Some(a)), (_, Some(b))] => {
                            let d = a
                                .pads
                                .iter()
                                .flat_map(|&i| b.pads.iter().map(move |&j| (i, j)))
                                .map(|(i, j)| pcb::geom::dist(board.pads[i].centre, board.pads[j].centre))
                                .fold(f64::INFINITY, f64::min);
                            push(m.parts.join(" – "), Some(round(d)), "mm", judged(d, limit), Some(format!("{} – {}, nearest pad centres", a.reference, b.reference)));
                        }
                        _ => push(m.parts.join(" – "), None, "mm", "unbound", Some("a part is not bound".into())),
                    }
                }
                Metric::DecouplingPathMm => {
                    // The supply nets the listed capacitors sit on, the ground excluded.
                    let mut supply: BTreeSet<String> = BTreeSet::new();
                    for (net, n) in &pkg.nets {
                        if net != "GND" && n.pins.iter().any(|p| m.parts.iter().any(|c| p.starts_with(&format!("{c}.")))) {
                            if let Some(b) = nets.get(net) {
                                supply.insert(b.clone());
                            }
                        }
                    }
                    let supply: Vec<String> = supply.into_iter().collect();
                    match verify::nets::decoupling(&board, &bind.chip, &supply, &ground) {
                        Ok(pins) => {
                            for p in pins {
                                let subject = format!("{}.{} ({})", bind.chip, p.pin, p.net);
                                match (p.length_mm, &p.cap) {
                                    (Some(l), Some(cap)) => push(subject, Some(round(l)), "mm", judged(l, limit), Some(format!("nearest capacitor {cap}"))),
                                    _ => push(subject, None, "mm", "fail", Some("no capacitor to ground on this pin's net".into())),
                                }
                            }
                        }
                        Err(e) => push(supply.join(", "), None, "mm", "unverifiable", Some(e)),
                    }
                }
                Metric::HotLoopAreaMm2 | Metric::OutputLoopAreaMm2 | Metric::SwitchNodeAreaMm2 | Metric::FeedbackClearanceMm | Metric::SwitchNodeUnderlapMm2 => {
                    let report = buck_cache.entry(id.clone()).or_insert_with(|| {
                        let roles: BTreeMap<String, String> = blk
                            .roles
                            .iter()
                            .map(|(k, v)| {
                                let b = if v.contains('.') { cx.pin(v) } else { bind.parts.get(v).cloned() };
                                b.map(|b| (k.clone(), b)).ok_or_else(|| format!("role {k} ({v}) is not bound"))
                            })
                            .collect::<Result<_, _>>()?;
                        verify::buck::check(&board, id, &verify::buck::BuckRoles::from_map(&roles)?, &BTreeMap::new())
                    });
                    match report {
                        Ok(r) => match r.measurements.iter().find(|x| x.check == metric) {
                            Some(x) => push(m.nets.join(", "), Some(round(x.measured)), x.unit, judged(x.measured, limit), x.method.clone()),
                            None => push(m.nets.join(", "), None, unit_of(m.metric), "unverifiable", Some("the buck check did not measure this".into())),
                        },
                        Err(e) => push(m.nets.join(", "), None, unit_of(m.metric), "unbound", Some(e.clone())),
                    }
                }
                Metric::TraceLengthMm | Metric::ClearanceMm => push(m.nets.join(", "), None, unit_of(m.metric), "unverifiable", Some("not measured by this check yet".into())),
            }
        }
    }

    // Net length limits, between the pin pair the reference was measured on.
    for (net, n) in &pkg.nets {
        let Some(max) = pkg.intent.nets.get(net).and_then(|i| i.max_length.as_ref()) else { continue };
        let Some(pair) = &n.length else { continue };
        let limit = verify::Limit { op: "<=", value: max.value.mm };
        let source = max.provenance.as_ref().map(|p| format!("{} {}", p.source, p.locator.as_deref().unwrap_or("")).trim().to_string());
        let (status, measured, detail) = match (cx.pin(&pair.from), cx.pin(&pair.to)) {
            (Some(a), Some(b)) => match verify::nets::path(&board, &a, &b) {
                Ok(p) => (judged(p.length_mm, limit), Some(round(p.length_mm)), Some(format!("{a} → {b}"))),
                Err(e) => ("fail", None, Some(e)),
            },
            _ => ("unbound", None, Some(format!("{} → {}: a part is not bound", pair.from, pair.to))),
        };
        findings.push(Finding {
            kind: "length",
            block: None,
            subject: net.clone(),
            check: "trace_length_mm".into(),
            measured,
            unit: Some("mm"),
            limit: Some(format!("<= {}", max.value.mm)),
            strength: strength(max.strength),
            status,
            detail,
            source,
        });
    }

    // Part values, from the claims' `checks`.
    for claim in pkg.rules["claims"].as_array().into_iter().flatten() {
        let id = claim["id"].as_str().unwrap_or_default();
        let strength = if claim["status"].as_str() == Some("contested") { "soft" } else { "hard" };
        for c in claim["checks"].as_array().into_iter().flatten() {
            let part = c["part"].as_str().unwrap_or_default();
            let quantity = c["quantity"].as_str().unwrap_or_default();
            // A check on a part that only matters when another is fitted (a second memory's pull-up).
            let condition = c["when_fitted"].as_str().filter(|w| cx.part(w).is_none_or(|p| not_fitted(&p.value)));
            let (status, measured, detail) = match condition {
                Some(w) => ("not_applicable", None, Some(format!("{w} is not fitted"))),
                None => value_check(cx.part(part), quantity, c),
            };
            findings.push(Finding {
                kind: "value",
                block: claim["scope"]["block"].as_str().map(String::from),
                subject: part.into(),
                check: quantity.into(),
                measured,
                unit: None,
                limit: Some(limit_text(c)),
                strength,
                status,
                detail,
                source: Some(id.into()),
            });
        }
    }

    let mut counts = BTreeMap::new();
    for f in &findings {
        *counts.entry(f.status).or_insert(0) += 1;
    }
    let ok = !findings.iter().any(|f| f.status == "fail" && f.strength == "hard");
    Ok(Report {
        ok,
        package: bind.package.clone(),
        variant: bind.variant.clone(),
        nets,
        unbound: unbound.into_iter().collect(),
        reversed: cx.swapped.iter().cloned().collect(),
        counts,
        findings,
    })
}

fn finding(kind: &'static str, block: Option<String>, subject: &str, check: &str, strength: &'static str, status: &'static str, detail: Option<String>) -> Finding {
    Finding { kind, block, subject: subject.into(), check: check.into(), measured: None, unit: None, limit: None, strength, status, detail, source: None }
}

fn strength(s: Strength) -> &'static str {
    match s {
        Strength::Hard => "hard",
        Strength::Soft => "soft",
    }
}

fn op(o: BoundOp) -> &'static str {
    match o {
        BoundOp::Le => "<=",
        BoundOp::Ge => ">=",
    }
}

fn judged(v: f64, l: verify::Limit) -> &'static str {
    // Measurements are rounded to the micrometre before judging, so a value printed equal to
    // the limit passes.
    let v = round(v);
    let pass = if l.op == ">=" { v >= l.value } else { v <= l.value };
    if pass {
        "pass"
    } else {
        "fail"
    }
}

fn round(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

fn unit_of(m: Metric) -> &'static str {
    match m {
        Metric::HotLoopAreaMm2 | Metric::OutputLoopAreaMm2 | Metric::SwitchNodeUnderlapMm2 | Metric::SwitchNodeAreaMm2 => "mm2",
        Metric::ViaCount => "vias",
        _ => "mm",
    }
}

/// The claim that limits `metric` to boards with a layer count this board does not have.
fn layer_scope(rules: &Value, metric: &str, layers: usize) -> Option<String> {
    let stem = metric.trim_end_matches("_mm2").trim_end_matches("_mm");
    rules["claims"].as_array()?.iter().find_map(|c| {
        let applies = c["attributes"].as_array()?.iter().any(|a| a.as_str() == Some(stem));
        let scope = c["scope"]["layers"].as_str()?;
        let min: usize = scope.trim_start_matches(">=").trim().parse().ok()?;
        (applies && layers < min).then(|| c["id"].as_str().unwrap_or_default().to_string())
    })
}

fn limit_text(c: &Value) -> String {
    if let Some(any) = c["any_of"].as_array() {
        return format!("one of {}", any.iter().filter_map(|a| a.as_str()).collect::<Vec<_>>().join(", "));
    }
    let mut parts = Vec::new();
    if let Some(e) = c.get("equals").filter(|e| !e.is_null()) {
        parts.push(format!("= {e}"));
    }
    if let Some(v) = c["min"].as_f64() {
        parts.push(format!(">= {v}"));
    }
    if let Some(v) = c["max"].as_f64() {
        parts.push(format!("<= {v}"));
    }
    parts.join(", ")
}

fn not_fitted(value: &str) -> bool {
    let v = value.trim().to_ascii_uppercase();
    ["DNF", "DNP", "DNI", "NC", "NF"].iter().any(|k| v == *k || v.starts_with(&format!("{k} ")) || v.starts_with(&format!("{k}_")))
}

fn value_check(part: Option<&pcb::Part>, quantity: &str, c: &Value) -> (&'static str, Option<f64>, Option<String>) {
    if quantity == "fitted" {
        let want = c["equals"].as_bool().unwrap_or(true);
        return match part {
            None if !want => ("pass", None, Some("not on the board".into())),
            None => ("unbound", None, None),
            Some(p) => {
                let fitted = !not_fitted(&p.value);
                let detail = Some(format!("{} = `{}`", p.reference, p.value));
                (if fitted == want { "pass" } else { "fail" }, None, detail)
            }
        };
    }
    let Some(p) = part else { return ("unbound", None, None) };
    let detail = format!("{} = `{}`", p.reference, p.value);
    if not_fitted(&p.value) {
        return ("fail", None, Some(format!("{detail}: not fitted")));
    }
    if quantity == "part_number" {
        let v = p.value.to_ascii_uppercase();
        let named = c["any_of"].as_array().into_iter().flatten().filter_map(|a| a.as_str()).any(|a| v.contains(&a.to_ascii_uppercase()));
        // A value that names no part number (`12MHz`) cannot be judged from the board.
        return if named { ("pass", None, Some(detail)) } else { ("unverifiable", None, Some(format!("{detail} does not name the part"))) };
    }
    let Some(v) = parse_value(&p.value, quantity) else {
        return ("unverifiable", None, Some(format!("{detail}: not a {quantity}")));
    };
    let close = |a: f64, b: f64| (a - b).abs() <= 1e-6 * b.abs().max(1e-30);
    let mut ok = true;
    if let Some(e) = c["equals"].as_f64() {
        ok &= close(v, e);
    }
    if let Some(m) = c["min"].as_f64() {
        ok &= v >= m || close(v, m);
    }
    if let Some(m) = c["max"].as_f64() {
        ok &= v <= m || close(v, m);
    }
    (if ok { "pass" } else { "fail" }, Some(v), Some(detail))
}

/// A component value in SI units: `4.7u`, `4.7uF`, `4u7`, `100n`, `27R`, `4R7`, `5.1k`, `1M`,
/// `3.3uH`. The first word counts (`100nF 16V X7R`).
pub fn parse_value(raw: &str, quantity: &str) -> Option<f64> {
    let mut t = raw.split_whitespace().next()?.replace('µ', "u").replace('Ω', "");
    for unit in ["ohms", "ohm", "Ohm"] {
        if let Some(s) = t.strip_suffix(unit) {
            t = s.to_string();
        }
    }
    let unit = match quantity {
        "capacitance" => Some('F'),
        "inductance" => Some('H'),
        _ => None,
    };
    if let Some(u) = unit {
        if t.ends_with(u) || t.ends_with(u.to_ascii_lowercase()) {
            t.pop();
        }
    }
    const MULT: [(char, f64); 10] = [('p', 1e-12), ('n', 1e-9), ('u', 1e-6), ('m', 1e-3), ('k', 1e3), ('K', 1e3), ('M', 1e6), ('G', 1e9), ('R', 1.0), ('r', 1.0)];
    for (i, ch) in t.char_indices() {
        if let Some(&(_, m)) = MULT.iter().find(|(c, _)| *c == ch) {
            let (a, b) = (&t[..i], &t[i + ch.len_utf8()..]);
            let num = if b.is_empty() { a.to_string() } else { format!("{a}.{b}") };
            return num.parse::<f64>().ok().map(|v| v * m);
        }
    }
    t.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::parse_value;

    #[test]
    fn values() {
        let cases = [
            ("4.7u", "capacitance", 4.7e-6),
            ("4.7uF", "capacitance", 4.7e-6),
            ("4u7", "capacitance", 4.7e-6),
            ("100nF 16V X7R", "capacitance", 1e-7),
            ("15p", "capacitance", 15e-12),
            ("27R", "resistance", 27.0),
            ("4R7", "resistance", 4.7),
            ("5.1k", "resistance", 5100.0),
            ("1M", "resistance", 1e6),
            ("33", "resistance", 33.0),
            ("3.3uH", "inductance", 3.3e-6),
        ];
        for (raw, q, want) in cases {
            let got = parse_value(raw, q).unwrap_or_else(|| panic!("{raw}"));
            assert!((got - want).abs() <= 1e-9 * want, "{raw}: {got} != {want}");
        }
    }
}
