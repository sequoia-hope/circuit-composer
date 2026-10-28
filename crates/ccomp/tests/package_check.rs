//! `ccomp check package` on easy2350, which composes the rp2350 package's blocks: the board is
//! in the repository, so the whole judgement runs here.

use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn check(board: &str) -> Value {
    let out = Command::new(env!("CARGO_BIN_EXE_ccomp"))
        .args(["check", "package", board, "--package", "packages/rp2350", "--binding", "boards/easy2350/rp2350.binding.json"])
        .current_dir(root())
        .output()
        .expect("ccomp runs");
    serde_json::from_slice(&out.stdout).expect("one JSON result")
}

fn findings<'a>(r: &'a Value, status: &str) -> Vec<&'a Value> {
    r["findings"].as_array().unwrap().iter().filter(|f| f["status"] == status).collect()
}

#[test]
fn easy2350_against_rp2350() {
    let r = check("boards/easy2350/input/easy2350.kicad_pcb");
    assert_eq!(r["nets"]["XIN"], "/XIN");
    assert_eq!(r["nets"]["USB_DP_PIN"], "/USB_DP_MCU");
    // easy2350 has no link resistor between QSPI_SS and the flash: one net.
    assert_eq!(r["nets"]["FLASH_SS"], "/QSPI_SS");
    // R6 is fitted the other way round from the reference: a resistor, so no finding.
    assert_eq!(r["reversed"], serde_json::json!(["R_XOUT"]));
    assert!(findings(&r, "fail").iter().all(|f| f["kind"] != "wiring"));

    // The one hard failure: the USB pair from the connector runs over the +3V3 plane on B.Cu.
    let hard: Vec<_> = findings(&r, "fail").into_iter().filter(|f| f["strength"] == "hard").collect();
    let subjects: Vec<_> = hard.iter().map(|f| f["subject"].as_str().unwrap()).collect();
    assert_eq!(subjects, ["USB_DP", "USB_DM"]);
    assert!(hard[0]["detail"].as_str().unwrap().contains("over +3V3"));
    assert_eq!(r["ok"], false);

    // The core regulator passes: easy2350 copies the vendor layout.
    let core: Vec<_> = r["findings"].as_array().unwrap().iter().filter(|f| f["block"] == "core_reg" && f["kind"] == "constraint").collect();
    assert!(core.iter().any(|f| f["check"] == "feedback_clearance_mm" && f["status"] == "pass"));
    // Values: the crystal's value names no part, so it is unverifiable, not failed.
    let xtal = r["findings"].as_array().unwrap().iter().find(|f| f["subject"] == "XTAL").unwrap();
    assert_eq!(xtal["status"], "unverifiable");
    assert!(findings(&r, "fail").iter().all(|f| f["kind"] != "value"));
}

#[test]
fn via_in_pad_is_joined() {
    // ultraroute put a via inside U5.3's pad and left on B.Cu: the length is measured, not an
    // error.
    let r = check("boards/easy2350/routed/regions-971e723/easy2350-ultraroute-971e723-regions-s1.kicad_pcb");
    let sd2 = r["findings"].as_array().unwrap().iter().find(|f| f["kind"] == "length" && f["subject"] == "QSPI_SD2").unwrap();
    assert!(sd2["measured"].as_f64().is_some(), "{sd2}");
}
