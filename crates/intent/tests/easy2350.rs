//! The hand-annotated benchmark board passes, and each kind of damage to it is refused with the
//! code the agent would act on.

use std::path::Path;

use intent::netlist::Netlist;
use intent::validate::{validate, Report, Severity};
use intent::*;

fn board() -> (IntentFile, Netlist) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../boards/easy2350");
    let file = intent::load(&dir.join("intent.json")).unwrap();
    let netlist = Netlist::load(&dir.join("input/easy2350.net")).unwrap();
    (file, netlist)
}

fn errors(r: &Report) -> Vec<(&'static str, String)> {
    r.findings.iter().filter(|f| f.severity == Severity::Error).map(|f| (f.code, f.id.clone())).collect()
}

#[test]
fn easy2350_passes() {
    let (file, netlist) = board();
    let r = validate(&file, Some(&netlist));
    assert!(r.ok, "{:#?}", errors(&r));
    assert_eq!(r.summary.nets_passing, r.summary.nets_in_netlist);
    assert!(r.summary.parts_unassigned.is_empty());
}

#[test]
fn missing_net_is_refused() {
    let (mut file, netlist) = board();
    file.nets.remove("/XIN");
    assert!(errors(&validate(&file, Some(&netlist))).contains(&("net_missing", "/XIN".into())));
}

#[test]
fn required_field_is_refused() {
    let (mut file, netlist) = board();
    file.nets.get_mut("/VREG_LX").unwrap().current = None;
    let r = validate(&file, Some(&netlist));
    assert!(r.findings.iter().any(|f| f.code == "net_missing_field" && f.id == "/VREG_LX" && f.field.as_deref() == Some("current")));
}

#[test]
fn hard_without_provenance_is_refused() {
    let (mut file, netlist) = board();
    file.nets.get_mut("+3V3").unwrap().voltage.as_mut().unwrap().provenance = None;
    assert!(errors(&validate(&file, Some(&netlist))).contains(&("hard_without_provenance", "+3V3".into())));
}

#[test]
fn uncited_source_is_refused() {
    let (mut file, netlist) = board();
    file.sources.remove("rp2350_hdg");
    assert!(errors(&validate(&file, Some(&netlist))).iter().any(|(c, _)| *c == "unknown_source"));
}

#[test]
fn unresolved_block_type_is_refused() {
    let (mut file, netlist) = board();
    file.blocks.get_mut("core_reg").unwrap().kind = "switcher".into();
    assert!(errors(&validate(&file, Some(&netlist))).contains(&("block_unresolved_type", "core_reg".into())));
}

#[test]
fn part_in_two_blocks_is_refused() {
    let (mut file, netlist) = board();
    file.blocks.get_mut("crystal").unwrap().parts.push("L1".into());
    assert!(errors(&validate(&file, Some(&netlist))).iter().any(|(c, _)| *c == "part_in_two_blocks"));
}

#[test]
fn broken_pair_is_refused() {
    let (mut file, netlist) = board();
    file.nets.get_mut("/USB_DM").unwrap().impedance.as_mut().unwrap().value.pair = Some("/USB_DM_C".into());
    let codes: Vec<_> = errors(&validate(&file, Some(&netlist))).into_iter().map(|(c, _)| c).collect();
    assert!(codes.contains(&"pair_not_reciprocal"), "{codes:?}");
}

#[test]
fn protection_pad_off_net_is_refused() {
    let (mut file, netlist) = board();
    file.protection_paths[0].sequence[1] = "U2.2".into();
    assert!(errors(&validate(&file, Some(&netlist))).iter().any(|(c, _)| *c == "pad_not_on_net"));
}

#[test]
fn peak_below_continuous_is_a_conflict() {
    let (mut file, netlist) = board();
    let c = file.nets.get_mut("VBUS").unwrap().current.as_mut().unwrap();
    c.value.peak_a = Some(c.value.continuous_a / 2.0);
    assert!(errors(&validate(&file, Some(&netlist))).contains(&("conflict", "VBUS".into())));
}

#[test]
fn soft_needs_weight() {
    let (mut file, netlist) = board();
    file.nets.get_mut("/QSPI_SCLK").unwrap().aggressor.as_mut().unwrap().weight = None;
    assert!(errors(&validate(&file, Some(&netlist))).contains(&("soft_without_weight", "/QSPI_SCLK".into())));
}

#[test]
fn unknown_field_is_refused_at_load() {
    let text = r#"{"schema_version":"0.1","board":{"name":"x"},"nets":{"A":{"clas":"SIGNAL"}}}"#;
    assert!(serde_json::from_str::<IntentFile>(text).is_err());
}

#[test]
fn round_trips() {
    let (file, _) = board();
    let text = serde_json::to_string(&file).unwrap();
    let again: IntentFile = serde_json::from_str(&text).unwrap();
    assert_eq!(serde_json::to_value(&file).unwrap(), serde_json::to_value(&again).unwrap());
}

#[test]
fn committed_schema_is_current() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schema/intent.schema.json");
    let committed: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(committed, intent::json_schema(), "run `ccomp schema -o schema/intent.schema.json`");
}
