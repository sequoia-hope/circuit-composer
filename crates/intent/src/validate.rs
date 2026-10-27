//! The validator: refuses to pass a board until every net has the fields its class requires,
//! every block has a resolved type, and every hard constraint has a provenance.
//!
//! It reports every finding at once, each naming its subject and field, so the agent can fix
//! them in one pass.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::model::*;
use crate::netlist::Netlist;
use crate::presets;

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    /// True when there are no errors.
    pub ok: bool,
    pub summary: Summary,
    pub findings: Vec<Finding>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Summary {
    pub errors: usize,
    pub warnings: usize,
    /// Netlist nets that need intent (unconnected single pins left out).
    pub nets_in_netlist: usize,
    pub nets_with_intent: usize,
    pub nets_passing: usize,
    pub blocks: usize,
    /// Netlist parts that no block claims.
    pub parts_unassigned: Vec<String>,
    pub hard_constraints: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    pub severity: Severity,
    /// Stable machine-readable code, e.g. `net_missing_field`.
    pub code: &'static str,
    /// What the finding is about: `net`, `block`, `domain`, `region`, `protection_path`,
    /// `energy_source`, `source` or `file`.
    pub subject: &'static str,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    pub message: String,
}

struct Ctx<'a> {
    intent: &'a IntentFile,
    netlist: Option<&'a Netlist>,
    findings: Vec<Finding>,
    hard: usize,
}

impl Ctx<'_> {
    fn push(&mut self, severity: Severity, code: &'static str, subject: &'static str, id: &str, field: Option<&str>, message: String) {
        self.findings.push(Finding { severity, code, subject, id: id.to_string(), field: field.map(str::to_string), message });
    }

    fn error(&mut self, code: &'static str, subject: &'static str, id: &str, field: Option<&str>, message: String) {
        self.push(Severity::Error, code, subject, id, field, message);
    }

    fn warn(&mut self, code: &'static str, subject: &'static str, id: &str, field: Option<&str>, message: String) {
        self.push(Severity::Warning, code, subject, id, field, message);
    }

    /// Whether a net name exists: in the netlist when there is one, else in the intent's nets.
    fn net_known(&self, name: &str) -> bool {
        match self.netlist {
            Some(n) => n.nets.contains_key(name),
            None => self.intent.nets.contains_key(name),
        }
    }

    fn check_net_ref(&mut self, subject: &'static str, id: &str, field: &str, net: &str) {
        if self.net_known(net) {
            return;
        }
        let hint = self.netlist.and_then(|n| n.find_net(net)).map(|k| format!("; did you mean `{k}`?")).unwrap_or_default();
        self.error("unknown_net", subject, id, Some(field), format!("net `{net}` is not on the board{hint}"));
    }

    fn check_part_ref(&mut self, subject: &'static str, id: &str, field: &str, part: &str) {
        if let Some(n) = self.netlist {
            if !n.components.contains_key(part) {
                self.error("unknown_part", subject, id, Some(field), format!("part `{part}` is not in the netlist"));
            }
        }
    }

    fn check_provenance(&mut self, subject: &'static str, id: &str, field: &str, p: &Provenance) {
        if p.source.starts_with("preset:") {
            if p.kind != ClaimKind::Preset {
                self.error("bad_provenance", subject, id, Some(field), format!("`{}` is a preset source but kind is not `preset`", p.source));
            }
        } else if !self.intent.sources.contains_key(&p.source) {
            self.error("unknown_source", subject, id, Some(field), format!("provenance cites `{}`, which is not in `sources`", p.source));
        }
    }

    fn check_attr<T>(&mut self, subject: &'static str, id: &str, field: &str, a: &Attr<T>) {
        match a.strength {
            Strength::Hard => {
                self.hard += 1;
                match &a.provenance {
                    None => self.error("hard_without_provenance", subject, id, Some(field), "hard constraint has no provenance".into()),
                    Some(p) if p.kind == ClaimKind::Preset => {
                        self.error("hard_from_preset", subject, id, Some(field), "a preset default cannot be a hard constraint".into())
                    }
                    Some(_) => {}
                }
                if a.weight.is_some() {
                    self.warn("weight_on_hard", subject, id, Some(field), "weight is ignored on a hard constraint".into());
                }
            }
            Strength::Soft => match a.weight {
                None => self.error("soft_without_weight", subject, id, Some(field), "soft constraint has no weight".into()),
                Some(w) if !(w > 0.0 && w <= 1.0) => {
                    self.error("bad_weight", subject, id, Some(field), format!("weight {w} is outside (0, 1]"))
                }
                Some(_) => {}
            },
        }
        if let Some(p) = &a.provenance {
            self.check_provenance(subject, id, field, p);
        }
    }

    fn check_positive(&mut self, subject: &'static str, id: &str, field: &str, what: &str, v: f64, allow_zero: bool) {
        let bad = !v.is_finite() || v < 0.0 || (!allow_zero && v == 0.0);
        if bad {
            let need = if allow_zero { "non-negative" } else { "positive" };
            self.error("bad_value", subject, id, Some(field), format!("{what} = {v} must be {need}"));
        }
    }
}

pub fn validate(intent: &IntentFile, netlist: Option<&Netlist>) -> Report {
    let mut cx = Ctx { intent, netlist, findings: Vec::new(), hard: 0 };
    let mut summary = Summary::default();

    if intent.schema_version != SCHEMA_VERSION {
        cx.error("schema_version", "file", "", None, format!("schema_version `{}`; this validator reads `{SCHEMA_VERSION}`", intent.schema_version));
    }
    if netlist.is_none() {
        cx.warn("no_netlist", "file", "", None, "no netlist given: nets and parts are checked against the intent file only".into());
    }

    let failing_before = |cx: &Ctx, name: &str| cx.findings.iter().filter(|f| f.subject == "net" && f.id == name && f.severity == Severity::Error).count();

    // Nets the netlist has that the intent lacks.
    if let Some(n) = netlist {
        for (name, net) in &n.nets {
            if net.is_unconnected(name) {
                continue;
            }
            summary.nets_in_netlist += 1;
            if !intent.nets.contains_key(name) {
                cx.error("net_missing", "net", name, None, format!("net has no intent ({} pads)", net.nodes.len()));
            }
        }
    }

    for (name, net) in &intent.nets {
        if let Some(n) = netlist {
            match n.nets.get(name) {
                None => {
                    let hint = n.find_net(name).map(|k| format!("; KiCad names it `{k}`")).unwrap_or_default();
                    cx.error("net_unknown", "net", name, None, format!("net is not in the netlist{hint}"));
                }
                Some(kn) => {
                    if let (Some(kc), Some(ic)) = (&kn.class, &net.class) {
                        // GROUND refines easy_pcb's and KiCad's POWER: not a disagreement.
                        if kc != ic && !(ic == "GROUND" && kc == "POWER") {
                            cx.warn("class_differs_from_kicad", "net", name, Some("class"), format!("intent says `{ic}`, the schematic's net class is `{kc}`"));
                        }
                    }
                }
            }
        }
        check_net(&mut cx, name, net);
        if failing_before(&cx, name) == 0 {
            summary.nets_passing += 1;
        }
    }
    summary.nets_with_intent = intent.nets.len();

    check_blocks(&mut cx, &mut summary);
    if let Some(root) = &intent.domains {
        check_domains(&mut cx, root);
    }
    check_regions(&mut cx);
    check_protection(&mut cx);

    let Ctx { findings, hard, .. } = cx;
    summary.errors = findings.iter().filter(|f| f.severity == Severity::Error).count();
    summary.warnings = findings.len() - summary.errors;
    summary.blocks = intent.blocks.len();
    summary.hard_constraints = hard;
    Report { ok: summary.errors == 0, summary, findings }
}

fn check_net(cx: &mut Ctx, name: &str, net: &NetIntent) {
    const S: &str = "net";
    match net.class.as_deref() {
        None => cx.error("net_no_class", S, name, Some("class"), "net has no class".into()),
        Some(c) => match presets::net_class(c) {
            None => {
                let known: Vec<_> = presets::NET_CLASSES.iter().map(|c| c.name).collect();
                cx.error("unknown_class", S, name, Some("class"), format!("class `{c}` is not a preset ({})", known.join(", ")))
            }
            Some(class) => {
                for f in class.required {
                    if !f.present(net) {
                        cx.error("net_missing_field", S, name, Some(f.name()), format!("class {} requires `{}`", class.name, f.name()));
                    }
                }
            }
        },
    }

    if let Some(a) = &net.aggressor {
        cx.check_attr(S, name, "aggressor", a);
    }
    if let Some(a) = &net.victim {
        cx.check_attr(S, name, "victim", a);
    }
    if let Some(a) = &net.bandwidth {
        cx.check_attr(S, name, "bandwidth", a);
        if let Some(hz) = a.value.max_hz {
            cx.check_positive(S, name, "bandwidth", "max_hz", hz, false);
        }
    }
    if let Some(a) = &net.voltage {
        cx.check_attr(S, name, "voltage", a);
        cx.check_positive(S, name, "voltage", "peak_v", a.value.peak_v.abs(), true);
    }
    if let Some(a) = &net.current {
        cx.check_attr(S, name, "current", a);
        cx.check_positive(S, name, "current", "continuous_a", a.value.continuous_a, true);
        if let Some(p) = a.value.peak_a {
            cx.check_positive(S, name, "current", "peak_a", p, true);
            if p < a.value.continuous_a {
                cx.error("conflict", S, name, Some("current"), format!("peak_a {p} is below continuous_a {}", a.value.continuous_a));
            }
        }
    }
    if let Some(a) = &net.fault_current {
        cx.check_attr(S, name, "fault_current", a);
        cx.check_positive(S, name, "fault_current", "amps", a.value.amps, false);
        cx.check_positive(S, name, "fault_current", "duration_ms", a.value.duration_ms, false);
        if let Some(c) = &net.current {
            if a.value.amps < c.value.peak_a.unwrap_or(c.value.continuous_a) {
                cx.error("conflict", S, name, Some("fault_current"), "fault current is below the normal peak current".into());
            }
        }
    }
    if let Some(a) = &net.impedance {
        cx.check_attr(S, name, "impedance", a);
        let z = &a.value;
        cx.check_positive(S, name, "impedance", "ohms", z.ohms, false);
        cx.check_positive(S, name, "impedance", "tolerance_pct", z.tolerance_pct, false);
        match (z.mode, &z.pair) {
            (ImpedanceMode::Differential, None) => cx.error("conflict", S, name, Some("impedance"), "differential impedance needs `pair`".into()),
            (ImpedanceMode::Differential, Some(p)) => match cx.intent.nets.get(p).and_then(|n| n.impedance.as_ref()) {
                None => cx.error("pair_not_reciprocal", S, name, Some("impedance"), format!("pair `{p}` has no impedance intent")),
                Some(other) => {
                    if other.value.pair.as_deref() != Some(name) {
                        cx.error("pair_not_reciprocal", S, name, Some("impedance"), format!("pair `{p}` does not name this net back"));
                    }
                    if other.value.ohms != z.ohms || other.value.mode != z.mode {
                        cx.error("conflict", S, name, Some("impedance"), format!("pair `{p}` has a different impedance target"));
                    }
                }
            },
            (ImpedanceMode::SingleEnded, Some(_)) => cx.warn("pair_on_single_ended", S, name, Some("impedance"), "`pair` is ignored on a single-ended target".into()),
            (ImpedanceMode::SingleEnded, None) => {}
        }
        if let Some(p) = &z.pair {
            cx.check_net_ref(S, name, "impedance.pair", p);
        }
    }
    if let Some(a) = &net.topology {
        cx.check_attr(S, name, "topology", a);
    }
    if let Some(a) = &net.match_group {
        cx.check_attr(S, name, "match_group", a);
        match (a.value.tolerance_mm, a.value.tolerance_ps) {
            (None, None) | (Some(_), Some(_)) => {
                cx.error("conflict", S, name, Some("match_group"), "give exactly one of tolerance_mm and tolerance_ps".into())
            }
            (Some(t), None) | (None, Some(t)) => cx.check_positive(S, name, "match_group", "tolerance", t, false),
        }
    }
    if let Some(a) = &net.safety {
        cx.check_attr(S, name, "safety", a);
    }
    if let Some(a) = &net.max_length {
        cx.check_attr(S, name, "max_length", a);
        cx.check_positive(S, name, "max_length", "mm", a.value.mm, false);
    }
}

fn check_blocks(cx: &mut Ctx, summary: &mut Summary) {
    const S: &str = "block";
    let mut owner: BTreeMap<&str, &str> = BTreeMap::new();
    for (id, b) in &cx.intent.blocks {
        if let Some(t) = presets::block_type(&b.kind) {
            for role in t.roles {
                if !b.roles.contains_key(*role) {
                    cx.error("block_missing_role", S, id, Some("roles"), format!("a {} block names its `{role}`", t.name));
                }
            }
        }
        for (role, who) in &b.roles {
            let field = format!("roles.{role}");
            match who.split_once('.') {
                Some((r, pin)) => {
                    if let Some(n) = cx.netlist {
                        if !n.nets.values().any(|net| net.has_pad(r, pin)) {
                            cx.error("unknown_pad", S, id, Some(&field), format!("pad {who} is on no net"));
                        }
                    }
                }
                None => {
                    cx.check_part_ref(S, id, &field, who);
                    if !b.parts.contains(who) {
                        cx.error("role_outside_block", S, id, Some(&field), format!("part `{who}` is not one of the block's parts"));
                    }
                }
            }
        }
        if presets::block_type(&b.kind).is_none() {
            let known: Vec<_> = presets::BLOCK_TYPES.iter().map(|t| t.name).collect();
            cx.error("block_unresolved_type", S, id, Some("type"), format!("type `{}` is not in the block library ({})", b.kind, known.join(", ")));
        }
        if let Some(pkg) = &b.package {
            match pkg.split_once('@') {
                None => cx.error("bad_package", S, id, Some("package"), format!("`{pkg}` is not `name@version`")),
                Some((name, ver)) => match cx.intent.board.packages.get(name) {
                    None => cx.error("package_not_locked", S, id, Some("package"), format!("package `{name}` is not in board.packages")),
                    Some(locked) if locked != ver => {
                        cx.error("package_version", S, id, Some("package"), format!("block uses {name}@{ver}, board locks {locked}"))
                    }
                    Some(_) => {}
                },
            }
        }
        if b.parts.is_empty() {
            cx.error("block_empty", S, id, Some("parts"), "block has no parts".into());
        }
        for p in &b.parts {
            cx.check_part_ref(S, id, "parts", p);
            if let Some(prev) = owner.insert(p, id) {
                cx.error("part_in_two_blocks", S, id, Some("parts"), format!("part `{p}` is also in block `{prev}`"));
            }
        }
        if let Some(goal) = &b.placement_goal {
            for o in &goal.objectives {
                for n in &o.nets {
                    cx.check_net_ref(S, id, "placement_goal.objectives", n);
                }
            }
            for c in &goal.constraints {
                cx.check_attr(S, id, "placement_goal.constraints", c);
                for n in &c.value.nets {
                    cx.check_net_ref(S, id, "placement_goal.constraints", n);
                }
                for p in &c.value.parts {
                    cx.check_part_ref(S, id, "placement_goal.constraints", p);
                }
            }
        }
        for (pname, port) in &b.ports {
            let field = format!("ports.{pname}");
            if port.nets.is_empty() {
                cx.error("port_empty", S, id, Some(&field), "port binds no nets".into());
            }
            for n in &port.nets {
                cx.check_net_ref(S, id, &field, n);
            }
            match port.kind {
                PortKind::Supply if port.voltage_v.is_none() || port.current_a.is_none() => {
                    cx.error("port_missing_field", S, id, Some(&field), "a supply port needs voltage_v and current_a".into())
                }
                PortKind::DiffPair if port.nets.len() != 2 => cx.error("port_shape", S, id, Some(&field), "a diff_pair port binds exactly two nets".into()),
                PortKind::DiffPair if port.impedance_ohm.is_none() => {
                    cx.error("port_missing_field", S, id, Some(&field), "a diff_pair port needs impedance_ohm".into())
                }
                PortKind::Bus if port.protocol.is_none() => cx.error("port_missing_field", S, id, Some(&field), "a bus port needs a protocol".into()),
                _ => {}
            }
            if let Some(c) = &port.class {
                if presets::net_class(c).is_none() {
                    cx.error("unknown_class", S, id, Some(&field), format!("class `{c}` is not a preset"));
                }
            }
        }
    }
    if let Some(n) = cx.netlist {
        summary.parts_unassigned = n.components.keys().filter(|r| !owner.contains_key(r.as_str())).cloned().collect();
    }
}

fn check_domains(cx: &mut Ctx, root: &Domain) {
    const S: &str = "domain";
    if root.kind != DomainKind::Board {
        cx.warn("domain_root", S, &root.name, Some("kind"), "the root domain should be of kind `board`".into());
    }
    let mut names = BTreeSet::new();
    let mut placed: BTreeMap<String, String> = BTreeMap::new();
    let mut stack = vec![root];
    while let Some(d) = stack.pop() {
        if !names.insert(d.name.clone()) {
            cx.error("duplicate_name", S, &d.name, Some("name"), "domain name is used twice".into());
        }
        for n in &d.supplies {
            cx.check_net_ref(S, &d.name, "supplies", n);
        }
        if let Some(g) = &d.reference_ground {
            cx.check_net_ref(S, &d.name, "reference_ground", g);
        }
        for b in &d.blocks {
            if !cx.intent.blocks.contains_key(b) {
                cx.error("unknown_block", S, &d.name, Some("blocks"), format!("block `{b}` is not defined"));
            }
            if let Some(prev) = placed.insert(b.clone(), d.name.clone()) {
                cx.error("block_in_two_domains", S, &d.name, Some("blocks"), format!("block `{b}` is also in domain `{prev}`"));
            }
        }
        for bar in &d.barriers {
            cx.check_attr(S, &d.name, "barriers", &bar.creepage_mm);
            if let Some(c) = &bar.clearance_mm {
                cx.check_attr(S, &d.name, "barriers", c);
            }
        }
        stack.extend(d.children.iter());
    }
    // Barriers name a sibling: check once every name is known.
    let mut stack = vec![root];
    while let Some(d) = stack.pop() {
        let siblings: BTreeSet<&str> = d.children.iter().map(|c| c.name.as_str()).collect();
        for c in &d.children {
            for bar in &c.barriers {
                if !siblings.contains(bar.to.as_str()) || bar.to == c.name {
                    cx.error("barrier_target", S, &c.name, Some("barriers"), format!("barrier to `{}`, which is not a sibling", bar.to));
                }
            }
        }
        stack.extend(d.children.iter());
    }
    for b in cx.intent.blocks.keys() {
        if !placed.contains_key(b) {
            cx.warn("block_without_domain", "block", b, None, "block is in no domain".into());
        }
    }
}

fn check_regions(cx: &mut Ctx) {
    const S: &str = "region";
    let mut names = BTreeSet::new();
    for r in &cx.intent.regions {
        let id = r.name.as_str();
        if !names.insert(id) {
            cx.error("duplicate_name", S, id, Some("name"), "region name is used twice".into());
        }
        match (&r.kind, &r.polygon) {
            (RegionKind::Soft, _) => match &r.block {
                None => cx.error("region_shape", S, id, Some("block"), "a soft region names the block it holds".into()),
                Some(b) if !cx.intent.blocks.contains_key(b) => cx.error("unknown_block", S, id, Some("block"), format!("block `{b}` is not defined")),
                Some(_) => {}
            },
            (_, None) => cx.error("region_shape", S, id, Some("polygon"), "region needs a polygon".into()),
            (_, Some(p)) if p.len() < 3 => cx.error("region_shape", S, id, Some("polygon"), "polygon needs at least 3 points".into()),
            _ => {}
        }
        if let Some(a) = &r.anchor {
            cx.check_part_ref(S, id, "anchor", a);
        }
        if r.rules.is_empty() {
            cx.warn("region_no_rules", S, id, Some("rules"), "region has no rules".into());
        }
        for rule in &r.rules {
            match (rule.mode, rule.weight) {
                (RuleMode::Prefer | RuleMode::Avoid, None) => cx.error("soft_without_weight", S, id, Some("rules"), "prefer/avoid needs a weight".into()),
                (RuleMode::Prefer | RuleMode::Avoid, Some(w)) if !(w > 0.0 && w <= 1.0) => {
                    cx.error("bad_weight", S, id, Some("rules"), format!("weight {w} is outside (0, 1]"))
                }
                (RuleMode::Must | RuleMode::Never, Some(_)) => cx.warn("weight_on_hard", S, id, Some("rules"), "weight is ignored on must/never".into()),
                _ => {}
            }
            match &rule.applies_to {
                Selector::Class(c) if presets::net_class(c).is_none() => cx.error("unknown_class", S, id, Some("rules"), format!("class `{c}` is not a preset")),
                Selector::Net(n) => cx.check_net_ref(S, id, "rules", n),
                _ => {}
            }
        }
        let hard = r.rules.iter().any(|x| matches!(x.mode, RuleMode::Must | RuleMode::Never));
        match &r.provenance {
            Some(p) => cx.check_provenance(S, id, "provenance", p),
            None if hard => cx.error("hard_without_provenance", S, id, Some("provenance"), "region has must/never rules but no provenance".into()),
            None => {}
        }
        if hard {
            cx.hard += 1;
        }
    }
}

fn check_protection(cx: &mut Ctx) {
    for (i, p) in cx.intent.protection_paths.iter().enumerate() {
        const S: &str = "protection_path";
        let id = format!("{}#{i}", p.net);
        cx.hard += 1;
        cx.check_net_ref(S, &id, "net", &p.net);
        cx.check_provenance(S, &id, "provenance", &p.provenance);
        if p.sequence.len() < 2 {
            cx.error("path_too_short", S, &id, Some("sequence"), "a path runs from the connector pad to at least the protection pad".into());
        }
        let net = cx.netlist.and_then(|n| n.nets.get(&p.net));
        for pad in &p.sequence {
            match pad.split_once('.') {
                None => cx.error("bad_pad", S, &id, Some("sequence"), format!("`{pad}` is not `REF.PIN`")),
                Some((r, pin)) => {
                    if let Some(net) = net {
                        if !net.has_pad(r, pin) {
                            cx.error("pad_not_on_net", S, &id, Some("sequence"), format!("pad {pad} is not on net `{}`", p.net));
                        }
                    }
                }
            }
        }
    }
    for e in &cx.intent.energy_sources {
        const S: &str = "energy_source";
        cx.check_net_ref(S, &e.net, "net", &e.net);
        match &e.protected_zone {
            // A supply input with no fuse or disconnect has no "upstream of the first disconnect"
            // to bound; a battery or capacitor bank can always dump its energy, so it needs one.
            None if matches!(e.kind, EnergyKind::Battery | EnergyKind::CapacitorBank) => {
                cx.warn("no_protected_zone", S, &e.net, Some("protected_zone"), "energy source has no protected zone".into())
            }
            None => {}
            Some(z) => match cx.intent.regions.iter().find(|r| &r.name == z) {
                None => cx.error("unknown_region", S, &e.net, Some("protected_zone"), format!("region `{z}` is not defined")),
                Some(r) if r.kind != RegionKind::ProtectedZone => {
                    cx.error("region_kind", S, &e.net, Some("protected_zone"), format!("region `{z}` is not a protected_zone"))
                }
                Some(_) => {}
            },
        }
        if !cx.intent.protection_paths.iter().any(|p| p.net == e.net) {
            cx.warn("no_protection_path", S, &e.net, None, "energy source has no protection path".into());
        }
    }
}
