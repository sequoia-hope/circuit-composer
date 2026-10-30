//! `ccomp floorplan` places two composed boards: easy2350 inside its own outline, where it must
//! be legal and need less wire than the person's placement, and the battery demo board, sized by
//! the floorplanner, where every edge and separation rule must hold.

use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

fn ccomp(args: &[&str]) -> Value {
    let out = Command::new(env!("CARGO_BIN_EXE_ccomp")).args(args).current_dir(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")).output().expect("ccomp runs");
    serde_json::from_slice(&out.stdout).expect("one JSON result")
}

#[test]
fn easy2350_beats_its_own_placement() {
    let base = ["floorplan", "boards/easy2350/floorplan/composed.kicad_pcb", "--composed", "boards/easy2350/floorplan/composed.json", "--spec", "boards/easy2350/floorplan/spec.json"];
    let human = ccomp(&[&base[..], &["--evaluate", "boards/easy2350/input/easy2350.kicad_pcb"]].concat());
    let ours = ccomp(&[&base[..], &["--seed", "3", "--moves", "200000"]].concat());
    assert_eq!(ours["ok"], true, "{:#}", ours["metrics"]);
    let (h, o) = (human["metrics"]["wire_mm"].as_f64().unwrap(), ours["metrics"]["wire_mm"].as_f64().unwrap());
    assert!(o < h, "ours {o:.0} mm, the person's {h:.0} mm");
    // J1 on the top edge, facing out (rotated 180: its mouth faces +y at 0).
    assert_eq!(ours["parts"]["J1"]["rotation"], 180.0);
    // The core regulator moved as one: L1 keeps its offset from U1.
    let (u, l) = (&ours["parts"]["U1"], &ours["parts"]["L1"]);
    let d2 = (l["x"].as_f64().unwrap() - u["x"].as_f64().unwrap()).hypot(l["y"].as_f64().unwrap() - u["y"].as_f64().unwrap());
    assert!((d2 - 1.6f64.hypot(7.2375)).abs() < 0.01, "{d2}");
}

#[test]
fn the_battery_board_keeps_its_rules() {
    let b = ccomp(&["floorplan", "boards/rp2350-battery/composed.kicad_pcb", "--composed", "boards/rp2350-battery/composed.json", "--spec", "boards/rp2350-battery/floorplan.json", "--seed", "2", "--moves", "200000"]);
    assert_eq!(b["ok"], true, "{:#}", b["metrics"]);
    assert_eq!(b["metrics"]["overlaps"], 0);
    assert!(b["metrics"]["separation_misses"].as_array().unwrap().is_empty());
    assert!(b["metrics"]["area_mm2"].as_f64().unwrap() < 2000.0, "{:#}", b["metrics"]);
}

/// Any KiCad board is a floorplan problem with its own nets and outline, and a saved problem
/// solves to the same placement as one built in place.
#[test]
fn a_board_by_itself_is_a_problem_and_a_saved_problem_solves_the_same() {
    let board = "boards/easy2350/input/easy2350.kicad_pcb";
    let pr = ccomp(&["floorplan", board, "--nets-from-board", "--outline-from-board", "--problem"]);
    assert!(pr["parts"].as_array().unwrap().len() >= 56, "{}", pr["parts"].as_array().unwrap().len());
    assert!(pr["nets"].as_array().unwrap().len() >= 40);
    let xs: Vec<f64> = pr["outline_polygon"].as_array().expect("its Edge.Cuts").iter().map(|p| p[0].as_f64().unwrap()).collect();
    let w = xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max) - xs.iter().cloned().fold(f64::INFINITY, f64::min);
    assert!((w - 21.05).abs() < 0.1, "board width {w}");

    let tmp = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("saved-problem.json");
    std::fs::write(&tmp, pr.to_string()).unwrap();
    let saved = ccomp(&["floorplan", "--input-problem", tmp.to_str().unwrap(), "--seed", "4", "--moves", "20000"]);
    let built = ccomp(&["floorplan", board, "--nets-from-board", "--outline-from-board", "--seed", "4", "--moves", "20000"]);
    assert_eq!(saved["parts"], built["parts"]);
}
