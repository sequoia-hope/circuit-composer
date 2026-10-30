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

/// A composition's nets as sets of pins, names aside (a recipe's CC1 is `/usb/CC1`).
fn pin_sets(c: &compose::Composed) -> BTreeSet<BTreeSet<String>> {
    c.nets.values().map(|ps| ps.iter().cloned().collect()).collect()
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
    // The USB-C sink's D+ and D- reach the MCU's USB pins through the package's 27 ohm resistors.
    assert_eq!(n["USB_DP"], ["mcu.USB_DP", "usb.DP"].map(String::from).into());
    assert_eq!(n["USB_DM"], ["mcu.USB_DM", "usb.DM"].map(String::from).into());
    // GPIO0 shares its pad with the flash's unfitted CS1 link: avoided while others are free.
    assert!(!s.pinout.iter().any(|r| r.partner == "mcu.GPIO0"));
    // The IMU package brings its own pull-ups, which suit the bus: nothing added.
    assert!(s.added.is_empty(), "{:?}", s.added);
    let c = composes(&s.composition);
    assert!(c.ok, "{:?}", c.errors);
    // The LED recipe: red, and 1k holds it to 2 mA at 1.8 V.
    let r = &c.parts[&c.nets["LED"].iter().find(|p| p.starts_with('R')).unwrap().split('.').next().unwrap().to_string()];
    assert_eq!((r.value.as_str(), r.fields["lcsc"].as_str()), ("1k", Some("C11702")));
    // Composing the loose file itself is refused.
    let raw = composes(&read("boards/rp2350-battery/loose.json"));
    assert!(!raw.ok && raw.errors[0].contains("ccomp pins"), "{:?}", raw.errors);
}

#[test]
fn pinned_it_is_the_hand_written_board() {
    let mut loose = read("boards/rp2350-battery/loose.json");
    // Pinned where the hand-drawn board has them; SCL follows SDA to GPIO5 by itself.
    loose["connect"][1]["pins"] = json!({ "SDA": "GPIO4" });
    for (k, pin) in [(2, "GPIO6"), (3, "GPIO7"), (4, "GPIO25")] {
        loose["connect"][k]["pins"] = json!({ "IO": pin });
    }
    // The recipes' parts keep the hand-drawn board's designators.
    loose["instances"]["usb"]["designators"] = json!({ "J1": "J1", "R_CC1": "R1", "R_CC2": "R2" });
    loose["instances"]["path"]["designators"] = json!({ "Q1": "Q1", "R_GATE": "R3", "D1": "D3" });
    loose["instances"]["led"]["designators"] = json!({ "R1": "R4", "D1": "D1" });
    let s = solve(&loose);
    assert!(s.ok, "{:?}", s.errors);
    let solved = composes(&s.composition);
    let hand = composes(&read("boards/rp2350-battery/compose.json"));
    assert!(solved.ok && hand.ok, "{:?} {:?}", solved.errors, hand.errors);
    // Pin for pin the same board. Only the LED differs: compose.json's green C72043 is no
    // longer made, and the recipe fits a red one.
    assert_eq!(pin_sets(&solved), pin_sets(&hand));
    assert_eq!(solved.parts.keys().collect::<Vec<_>>(), hand.parts.keys().collect::<Vec<_>>());
    let differ: Vec<&String> = solved.parts.iter().filter(|(r, p)| p.value != hand.parts[*r].value).map(|(r, _)| r).collect();
    assert_eq!(differ, ["D1"]);
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
    // Two 4.7k pairs in parallel, 2.35k, are still above Rp(min) (967 ohm at 3.3 V): both stay.
    assert!(s.added.is_empty(), "{:?}", s.added);
}

#[test]
fn a_soft_block_when_asked_and_refusals_with_reasons() {
    let mut loose = read("boards/rp2350-battery/loose.json");
    loose["connect"][1]["to"] = json!("mcu.PIO1");
    let s = solve(&loose);
    assert!(s.ok, "{:?}", s.errors);
    assert!(s.warnings.iter().any(|w| w.contains("software")) && s.cost >= 100.0);

    // This circuit ties CS high and SDO/SA0 low for I2C: SPI can't be had from it.
    let mut loose = read("boards/rp2350-battery/loose.json");
    loose["connect"][1]["from"] = json!("imu.spi");
    let s = solve(&loose);
    assert!(!s.ok && s.errors[0].contains("which the package ties to"), "{:?}", s.errors);

    // An interface used twice.
    let mut loose = read("boards/rp2350-battery/loose.json");
    loose["connect"].as_array_mut().unwrap().push(json!({ "from": "imu.int1", "to": "mcu" }));
    let s = solve(&loose);
    assert!(!s.ok && s.errors.iter().any(|e| e.contains("connected twice")), "{:?}", s.errors);
}

#[test]
fn packages_given_as_json_solve_the_same() {
    let loose = read("boards/rp2350-battery/loose.json");
    let c: compose::Composition = serde_json::from_value(loose.clone()).unwrap();
    let mut pk = serde_json::Map::new();
    for (inst, i) in loose["instances"].as_object().unwrap().iter().filter(|(_, i)| i.get("recipe").is_none()) {
        let (name, _) = i["package"].as_str().unwrap().split_once('@').unwrap();
        let v = i["variant"].as_str().unwrap();
        pk.insert(inst.clone(), json!({ "circuit": read(&format!("packages/{name}/schematic/circuit.{v}.json")), "ports": read(&format!("packages/{name}/ports.json")) }));
    }
    let types = interfaces::load_types(&root().join("library/interfaces.json")).unwrap();
    let mut pkgs = pinsolve::from_json(&Value::Object(pk)).unwrap();
    let (recipes, stock) = compose::load_library(&root().join("library")).unwrap();
    pinsolve::add_recipes(&mut pkgs, &c, &recipes, &stock).unwrap();
    let s = pinsolve::solve(&loose, &types, &pkgs, 2_000_000).unwrap();
    assert_eq!(nets(&s.composition), nets(&solve(&loose).composition));
}
