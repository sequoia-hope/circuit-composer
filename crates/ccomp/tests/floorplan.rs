//! `ccomp floorplan` places two composed boards: easy2350 inside its own outline, where it must
//! be legal and need less wire than the person's placement, and the battery demo board, sized by
//! the floorplanner, where every edge and separation rule must hold, and with regions as well.

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

#[test]
fn a_footprint_locked_in_the_board_stays_where_the_board_has_it() {
    // The person's easy2350 has every footprint locked in KiCad: made a problem from the board by
    // itself, every part is fixed where it is (U1 at 110.5, 126.5; TP3 on the back).
    let pr = ccomp(&["floorplan", "boards/easy2350/input/easy2350.kicad_pcb", "--nets-from-board", "--outline-from-board", "--problem"]);
    let parts = pr["parts"].as_array().unwrap();
    assert!(parts.iter().all(|p| !p["fixed"].is_null()), "every part fixed");
    let part = |r: &str| parts.iter().find(|p| p["reference"] == r).unwrap();
    assert_eq!(part("U1")["fixed"]["x"], 110.5);
    assert_eq!(part("U1")["fixed"]["y"], 126.5);
    assert_eq!(part("TP3")["back"], true);
}

/// Regions in the spec: the battery board's power blocks kept left of a 4 mm barrier no part may
/// enter, its MCU and IMU right of it. A rigid layout comes with its block (the core regulator's
/// copper is in `mcu`).
#[test]
fn blocks_keep_their_regions() {
    let mut spec: Value = serde_json::from_str(&std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../boards/rp2350-battery/floorplan.json")).unwrap()).unwrap();
    spec["outline"] = serde_json::json!([0, 0, 48, 40]);
    spec["regions"] = serde_json::json!([
        {"name": "power", "rect": [0, 0, 20, 40], "blocks": ["chg", "reg", "power_path", "usb_in"]},
        {"name": "logic", "polygon": [[24, 0], [48, 0], [48, 40], [24, 40]], "blocks": ["mcu", "imu"]},
        {"name": "barrier", "kind": "never", "rect": [20, 0, 24, 40]}
    ]);
    let path = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("battery-regions.json");
    std::fs::write(&path, spec.to_string()).unwrap();
    let b = ccomp(&["floorplan", "boards/rp2350-battery/composed.kicad_pcb", "--composed", "boards/rp2350-battery/composed.json", "--spec", path.to_str().unwrap(), "--seed", "1", "--moves", "200000"]);
    assert_eq!(b["ok"], true, "{:#}", b["metrics"]);
    for (name, bx) in b["blocks"].as_object().unwrap() {
        let v: Vec<f64> = bx.as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect();
        match name.split('/').next().unwrap() {
            "chg" | "reg" | "power_path" | "usb_in" => assert!(v[2] <= 20.001, "{name} {v:?}"),
            "mcu" | "imu" => assert!(v[0] >= 23.999, "{name} {v:?}"),
            _ => {}
        }
    }
    // A never region spares what it excepts: here the barrier lets the charger's parts in.
    spec["regions"] = serde_json::json!([{"name": "barrier", "kind": "never", "rect": [-100, -100, 100, 100], "except": ["chg", "J1"]}]);
    spec.as_object_mut().unwrap().remove("outline");
    std::fs::write(&path, spec.to_string()).unwrap();
    let pr = ccomp(&["floorplan", "boards/rp2350-battery/composed.kicad_pcb", "--composed", "boards/rp2350-battery/composed.json", "--spec", path.to_str().unwrap(), "--problem"]);
    let parts = pr["parts"].as_array().unwrap();
    let members: Vec<&str> = pr["regions"][0]["members"].as_array().unwrap().iter().map(|i| parts[i.as_u64().unwrap() as usize]["reference"].as_str().unwrap()).collect();
    assert!(!members.contains(&"J1") && members.contains(&"J2"), "{members:?}");
    assert!(parts.iter().filter(|p| p["block"].as_str().is_some_and(|b| b.starts_with("chg/"))).all(|p| !members.contains(&p["reference"].as_str().unwrap())));
    spec["regions"] = serde_json::json!([{"name": "power", "rect": [0, 0, 20, 40], "blocks": ["chg"]}]);
    // A block that names nothing is an error, not an empty region.
    spec["regions"][0]["blocks"] = serde_json::json!(["no_such_block"]);
    std::fs::write(&path, spec.to_string()).unwrap();
    let e = ccomp(&["floorplan", "boards/rp2350-battery/composed.kicad_pcb", "--composed", "boards/rp2350-battery/composed.json", "--spec", path.to_str().unwrap(), "--moves", "1000"]);
    assert_eq!(e["ok"], false);
    assert!(e["error"].as_str().unwrap().contains("no_such_block"), "{e}");
}

/// A cluster lifted from a board (`lift_layout.py`): a layout written bare, its roles the
/// designators (the spec names none), and a member on the anchor's other side, which the
/// placement puts on the bottom and keeps at its offset.
#[test]
fn a_lifted_cluster_moves_as_one_with_its_bottom_side_member() {
    let tmp = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let layout = serde_json::json!({
        "anchor_footprint": "Crystal:Crystal_SMD_3225-4Pin_3.2x2.5mm", "block_type": "crystal",
        "parts": {"C20": {"at": [-3.1, 0.85], "rotation": -180.0}, "C21": {"at": [3.1, -0.85], "rotation": 0.0}, "TP3": {"at": [0.2, -2.9], "rotation": 0.0, "flipped": true}},
        "tracks": [], "vias": [], "keepouts": [], "pours": []
    });
    let lp = tmp.join("lifted-xtal.json");
    std::fs::write(&lp, layout.to_string()).unwrap();
    let mut spec: Value = serde_json::from_str(&std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../boards/easy2350/floorplan/spec.json")).unwrap()).unwrap();
    spec["rigid"].as_array_mut().unwrap().push(serde_json::json!({"name": "xtal", "anchor": "Y1", "layout": lp.to_str().unwrap()}));
    let sp = tmp.join("lifted-xtal-spec.json");
    std::fs::write(&sp, spec.to_string()).unwrap();
    let b = ccomp(&["floorplan", "boards/easy2350/floorplan/composed.kicad_pcb", "--composed", "boards/easy2350/floorplan/composed.json", "--spec", sp.to_str().unwrap(), "--seed", "3", "--moves", "60000"]);
    assert!(b["back"].as_array().unwrap().iter().any(|r| r == "TP3"), "{}", b["back"]);
    let (y, t) = (&b["parts"]["Y1"], &b["parts"]["TP3"]);
    let r = y["rotation"].as_f64().unwrap().to_radians();
    let (ox, oy) = (0.2, -2.9);
    let want = (y["x"].as_f64().unwrap() + ox * r.cos() + oy * r.sin(), y["y"].as_f64().unwrap() - ox * r.sin() + oy * r.cos());
    assert!((t["x"].as_f64().unwrap() - want.0).abs() < 0.01 && (t["y"].as_f64().unwrap() - want.1).abs() < 0.01, "{t} vs {want:?}");
}
