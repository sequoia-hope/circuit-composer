//! The supply rule on the battery demo board with its regulator taken out: it must find one, and
//! say why the others fail.

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The loose board without `reg`, its +3V3 named for the rule to fill from VSYS, and the cell
/// declared as a source down to `cutoff_v`.
fn without_reg(cutoff_v: f64) -> Value {
    let mut d: Value = serde_json::from_str(&std::fs::read_to_string(root().join("boards/rp2350-battery/loose.json")).unwrap()).unwrap();
    d["instances"].as_object_mut().unwrap().remove("reg");
    for v in d["nets"].as_object_mut().unwrap().values_mut() {
        v.as_array_mut().unwrap().retain(|e| !e.as_str().unwrap().starts_with("reg."));
    }
    d["no_connect"].as_array_mut().unwrap().retain(|e| !e.as_str().unwrap().starts_with("reg."));
    d["rails"] = json!({ "+3V3": { "v": 3.3, "from": "VSYS" } });
    d["sources"] = json!({ "VBAT": { "v": [cutoff_v, 4.2], "kind": "lipo" } });
    d["intent"] = json!({ "layers": 4 });
    d
}

fn run(loose: &Value) -> (pinsolve::Solved, pinsolve::supply::Report) {
    let c: compose::Composition = serde_json::from_value(loose.clone()).unwrap();
    let pkgs = pinsolve::load(&c, &root().join("packages")).unwrap();
    let types = interfaces::load_types(&root().join("library/interfaces.json")).unwrap();
    let mut s = pinsolve::solve(loose, &types, &pkgs, 2_000_000).unwrap();
    assert!(s.ok, "{:?}", s.errors);
    let sc: compose::Composition = serde_json::from_value(s.composition.clone()).unwrap();
    let cands = pinsolve::supply::candidates(&root().join("packages")).unwrap();
    let rep = pinsolve::supply::run(&mut s.composition, &sc, &pkgs, Some(&cands));
    (s, rep)
}

fn ends(v: &Value, inst: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = v["nets"].as_object().unwrap().iter().flat_map(|(n, es)| es.as_array().unwrap().iter().filter_map(|e| e.as_str().unwrap().strip_prefix(&format!("{inst}.")).map(|p| (n.clone(), p.to_string())))).collect();
    out.sort();
    out
}

#[test]
fn a_cell_cut_off_at_3v3_gets_the_buck_wired_as_the_drawn_board() {
    let (s, rep) = run(&without_reg(3.3));
    assert!(rep.errors.is_empty(), "{:?}", rep.errors);
    let rail = rep.rails.iter().find(|r| r.net == "+3V3").unwrap();
    assert_eq!(rail.chosen.as_deref(), Some("reg_3v3: tlv62569 TLV62569DBV (VOUT)"));
    // The RP2350 at hello_usb and the IMU, from their cited draws.
    assert!((rail.budget.typ_a - 0.0163).abs() < 2e-4, "{}", rail.budget.typ_a);
    let first = &rail.options[0];
    assert!(first.ok && first.layout.as_deref() == Some("sot23-5-4layer"));
    // The LDO's dropout takes it below USB_OTP_VDD's floor at the bottom of the cell.
    let ldo = rail.options.iter().find(|o| o.option.starts_with("ncp730")).unwrap();
    assert!(!ldo.ok && ldo.reasons.iter().any(|r| r.contains("2.82 V") && r.contains("USB_OTP_VDD")), "{:?}", ldo.reasons);
    // Wired as the hand-drawn board wires its reg: input on VSYS, output on +3V3, EN left open.
    let drawn: Value = serde_json::from_str(&std::fs::read_to_string(root().join("boards/rp2350-battery/loose.json")).unwrap()).unwrap();
    assert_eq!(ends(&s.composition, "reg_3v3"), ends(&drawn, "reg"));
    assert_eq!(s.composition["instances"]["reg_3v3"], drawn["instances"]["reg"]);
    assert!(s.composition["no_connect"].as_array().unwrap().iter().any(|x| x == "reg_3v3.EN"));
    let c: compose::Composition = serde_json::from_value(s.composition.clone()).unwrap();
    assert!(compose::compose(&c, &root().join("packages")).unwrap().ok);
}

#[test]
fn a_cell_taken_to_3v0_has_no_supply_and_says_why() {
    let (_, rep) = run(&without_reg(3.0));
    let rail = rep.rails.iter().find(|r| r.net == "+3V3").unwrap();
    assert!(rail.chosen.is_none());
    assert!(rail.options.iter().all(|o| !o.ok));
    let buck = rail.options.iter().find(|o| o.option.starts_with("tlv62569")).unwrap();
    assert_eq!(buck.reasons, vec!["the output falls to 2.997 V at a 3 V input; mcu.USB_OTP_VDD needs 3.135 V".to_string()]);
    assert!(rep.errors.iter().any(|e| e.starts_with("+3V3: no candidate passes")));
}

#[test]
fn a_pinned_supply_is_placed_with_its_failures() {
    let mut d = without_reg(3.3);
    d["rails"]["+3V3"]["supply"] = json!("ncp730");
    d["rails"]["+3V3"]["as"] = json!("ldo");
    let (s, rep) = run(&d);
    assert_eq!(s.composition["instances"]["ldo"]["package"], "ncp730@0.1.0");
    assert!(rep.errors.iter().any(|e| e.contains("chosen under supply") && e.contains("USB_OTP_VDD")), "{:?}", rep.errors);
}
