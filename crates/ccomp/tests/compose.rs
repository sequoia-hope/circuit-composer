//! `ccomp compose` rebuilds easy2350 from the rp2350 package and the board's own parts, and the
//! result is the board's netlist, net for net.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn compose(path: &str) -> Value {
    let out = Command::new(env!("CARGO_BIN_EXE_ccomp")).args(["compose", path]).current_dir(root()).output().expect("ccomp runs");
    serde_json::from_slice(&out.stdout).expect("one JSON result")
}

#[test]
fn easy2350_recomposed_is_its_netlist() {
    let r = compose("boards/easy2350/compose.json");
    assert_eq!(r["ok"], true, "{:#}", r["errors"]);
    // Where easy2350 departs from the package, the composition says so.
    assert_eq!(r["taps"]["mcu.DVDD"], "+1V1");
    assert_eq!(r["joined"]["mcu.FLASH_SS"], "/mcu/QSPI_SS");

    let board = intent::netlist::Netlist::load(&root().join("boards/easy2350/input/easy2350.net")).unwrap();
    let theirs: BTreeSet<BTreeSet<String>> = board.nets.iter().filter(|(n, net)| !net.is_unconnected(n)).map(|(_, net)| net.nodes.iter().map(|n| format!("{}.{}", n.reference, n.pin)).collect()).collect();
    let ours: BTreeSet<BTreeSet<String>> = r["nets"].as_object().unwrap().values().map(|ps| ps.as_array().unwrap().iter().map(|p| p.as_str().unwrap().to_string()).collect::<BTreeSet<_>>()).filter(|s| s.len() > 1).collect();
    assert_eq!(theirs.len(), 65);
    let missing: Vec<_> = theirs.difference(&ours).collect();
    let extra: Vec<_> = ours.difference(&theirs).collect();
    assert!(missing.is_empty() && extra.is_empty(), "only on the board: {missing:?}\nonly composed: {extra:?}");

    // The binding the composer writes is the one written by hand, less the two parts easy2350
    // wires in the other order (they are the board's own in the composition).
    let hand: Value = serde_json::from_str(&std::fs::read_to_string(root().join("boards/easy2350/rp2350.binding.json")).unwrap()).unwrap();
    let mine = &r["bindings"]["mcu"];
    assert_eq!(mine["chip"], hand["chip"]);
    for (role, des) in mine["parts"].as_object().unwrap() {
        assert_eq!(&hand["parts"][role], des, "{role}");
    }
}

#[test]
fn the_minimal_board_composes() {
    let r = compose("packages/rp2350/reference/rp2350a_minimal/compose.json");
    assert_eq!(r["ok"], true, "{:#}", r["errors"]);
    assert_eq!(r["nets"].as_object().unwrap().values().filter(|p| p.as_array().unwrap().len() > 1).count(), 55);
    assert_eq!(r["nets"]["/mcu/XIN"], serde_json::json!(["C3.1", "U1.21", "Y1.1"]));
}
