//! The battery demo board, written as loose connections, against its hand-written composition.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(p: &str) -> Value {
    serde_json::from_str(&std::fs::read_to_string(root().join(p)).unwrap()).unwrap()
}

fn solve(loose: &Value) -> pinsolve::Solved {
    let c: compose::Composition = serde_json::from_value(loose.clone()).unwrap();
    let pkgs = pinsolve::load(&c, &root().join("packages")).unwrap();
    let types = interfaces::load_types(&root().join("library/interfaces.json")).unwrap();
    pinsolve::solve(loose, &types, &pkgs, 2_000_000).unwrap()
}

fn nets(v: &Value) -> BTreeMap<String, BTreeSet<String>> {
    v["nets"].as_object().unwrap().iter().map(|(k, e)| (k.clone(), e.as_array().unwrap().iter().map(|x| x.as_str().unwrap().to_string()).collect())).collect()
}

fn composes(v: &Value) -> compose::Composed {
    let c: compose::Composition = serde_json::from_value(v.clone()).unwrap();
    compose::compose(&c, &root().join("packages")).unwrap()
}

#[test]
fn the_loose_board_solves_and_composes() {
    let s = solve(&read("boards/rp2350-battery/loose.json"));
    assert!(s.ok, "{:?}", s.errors);
    assert!(s.exhaustive);
    let n = nets(&s.composition);
    // The IMU on I2C0's GPIO4/5, side by side, as the board was drawn by hand.
    assert_eq!(n["I2C0_SDA"], ["imu.SDA", "mcu.GPIO4"].map(String::from).into());
    assert_eq!(n["I2C0_SCL"], ["imu.SCL", "mcu.GPIO5"].map(String::from).into());
    assert!(s.pinout.iter().filter(|r| r.via.starts_with("I2C0")).all(|r| r.function.as_deref() == Some("F3")));
    // GPIO0 shares its pad with the flash's unfitted CS1 link: avoided while others are free.
    assert!(!s.pinout.iter().any(|r| r.partner == "mcu.GPIO0"));
    assert!(composes(&s.composition).ok);
    // Composing the loose file itself is refused.
    let raw = composes(&read("boards/rp2350-battery/loose.json"));
    assert!(!raw.ok && raw.errors[0].contains("ccomp pins"), "{:?}", raw.errors);
}

#[test]
fn pinned_it_is_the_hand_written_board() {
    let mut loose = read("boards/rp2350-battery/loose.json");
    // Pinned where the hand-drawn board has them; SCL follows SDA to GPIO5 by itself.
    loose["connect"][0]["pins"] = json!({ "SDA": "GPIO4" });
    for (k, pin) in [(1, "GPIO6"), (2, "GPIO7"), (3, "GPIO25")] {
        loose["connect"][k]["pins"] = json!({ "IO": pin });
    }
    let s = solve(&loose);
    assert!(s.ok, "{:?}", s.errors);
    assert_eq!(nets(&s.composition), nets(&read("boards/rp2350-battery/compose.json")));
}

#[test]
fn two_imus_share_one_bus() {
    let mut loose = read("boards/rp2350-battery/loose.json");
    loose["instances"]["imu2"] = loose["instances"]["imu"].clone();
    loose["nets"]["GND"].as_array_mut().unwrap().push(json!("imu2.GND"));
    loose["nets"]["+3V3"].as_array_mut().unwrap().push(json!("imu2.VDD"));
    loose["connect"].as_array_mut().unwrap().push(json!({ "from": "imu2.i2c", "to": "mcu" }));
    let s = solve(&loose);
    assert!(s.ok, "{:?}", s.errors);
    // One SDA net and one SCL net, each with both IMUs and one MCU pin.
    for sig in ["SDA", "SCL"] {
        let n = nets(&s.composition);
        let (_, net) = n.iter().find(|(_, e)| e.contains(&format!("imu.{sig}"))).unwrap();
        assert!(net.contains(&format!("imu2.{sig}")) && net.len() == 3, "{net:?}");
    }
    // Both packages tie SA0 low: the solver can't see straps, so it says to check them.
    assert!(s.warnings.iter().any(|w| w.contains("address")), "{:?}", s.warnings);
}

#[test]
fn a_soft_block_when_asked_and_refusals_with_reasons() {
    let mut loose = read("boards/rp2350-battery/loose.json");
    loose["connect"][0]["to"] = json!("mcu.PIO1");
    let s = solve(&loose);
    assert!(s.ok, "{:?}", s.errors);
    assert!(s.warnings.iter().any(|w| w.contains("software")) && s.cost >= 100.0);

    // This circuit ties CS high and SDO/SA0 low for I2C: SPI can't be had from it.
    let mut loose = read("boards/rp2350-battery/loose.json");
    loose["connect"][0]["from"] = json!("imu.spi");
    let s = solve(&loose);
    assert!(!s.ok && s.errors[0].contains("which the package ties to"), "{:?}", s.errors);

    // An output into an output, and an interface used twice.
    let mut loose = read("boards/rp2350-battery/loose.json");
    loose["connect"].as_array_mut().unwrap().push(json!({ "from": "imu.int1", "to": "mcu" }));
    let s = solve(&loose);
    assert!(!s.ok && s.errors.iter().any(|e| e.contains("connected twice")), "{:?}", s.errors);
}

#[test]
fn packages_given_as_json_solve_the_same() {
    let loose = read("boards/rp2350-battery/loose.json");
    let mut pk = serde_json::Map::new();
    for (inst, i) in loose["instances"].as_object().unwrap() {
        let (name, _) = i["package"].as_str().unwrap().split_once('@').unwrap();
        let v = i["variant"].as_str().unwrap();
        pk.insert(inst.clone(), json!({ "circuit": read(&format!("packages/{name}/schematic/circuit.{v}.json")), "ports": read(&format!("packages/{name}/ports.json")) }));
    }
    let types = interfaces::load_types(&root().join("library/interfaces.json")).unwrap();
    let s = pinsolve::solve(&loose, &types, &pinsolve::from_json(&Value::Object(pk)).unwrap(), 2_000_000).unwrap();
    assert_eq!(nets(&s.composition), nets(&solve(&loose).composition));
}
