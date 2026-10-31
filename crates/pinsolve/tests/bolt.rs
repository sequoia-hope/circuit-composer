//! The bolt earring composed from blocks: against the board's own netlist, and as loose
//! connections the solver turns back into it.

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

fn composes(v: &Value) -> compose::Composed {
    let c: compose::Composition = serde_json::from_value(v.clone()).unwrap();
    compose::compose(&c, &root().join("packages")).unwrap()
}

fn pin_sets(nets: impl Iterator<Item = BTreeSet<String>>) -> BTreeSet<BTreeSet<String>> {
    nets.filter(|s| !s.is_empty()).collect()
}

#[test]
fn the_blocks_are_the_bolt_net_for_net() {
    let c = composes(&read("boards/bolt-earring/compose.json"));
    assert!(c.ok, "{:?}", c.errors);
    assert!(c.unconnected_ports.is_empty(), "{:?}", c.unconnected_ports);
    let bolt = read("boards/bolt-earring/circuit.json");
    let parts = bolt["components"].as_object().unwrap();
    // Every one of the bolt's 256 parts, by its own designator.
    assert_eq!(c.parts.keys().collect::<BTreeSet<_>>(), parts.keys().collect::<BTreeSet<_>>());
    // The bolt's footprints name a few pads differently: its USB-C receptacle merges each pair of
    // power pads (A1B12) and numbers its shell legs 1-4 (KiCad's has one S1), and its NCP730's
    // exposed pad is EP, the package's 7.
    let alias: BTreeMap<&str, &str> = [
        ("J1.A1", "J1.A1B12"), ("J1.B12", "J1.A1B12"), ("J1.B1", "J1.B1A12"), ("J1.A12", "J1.B1A12"),
        ("J1.A4", "J1.A4B9"), ("J1.B9", "J1.A4B9"), ("J1.B4", "J1.B4A9"), ("J1.A9", "J1.B4A9"),
        ("J1.S1", "J1.1"), ("U3.7", "U3.EP"),
    ]
    .into();
    let composed = pin_sets(c.nets.values().map(|ps| ps.iter().map(|p| alias.get(p.as_str()).map_or(p.clone(), |a| a.to_string())).collect()));
    let mut by_net: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
    for (r, part) in parts {
        for (pin, net) in part["pins"].as_object().unwrap() {
            let net = net.as_str().unwrap();
            let shell_leg = r == "J1" && ["2", "3", "4"].contains(&pin.as_str());
            if !net.is_empty() && !shell_leg {
                by_net.entry(net).or_default().insert(format!("{r}.{pin}"));
            }
        }
    }
    let theirs = pin_sets(by_net.into_values());
    assert_eq!(composed.len(), 192);
    let missing: Vec<_> = theirs.difference(&composed).collect();
    let extra: Vec<_> = composed.difference(&theirs).collect();
    assert!(missing.is_empty() && extra.is_empty(), "the bolt's nets not composed: {missing:?}\ncomposed nets not the bolt's: {extra:?}");
    // The departures from the RP2350 package, each reported: the flash's exposed pad grounded,
    // CS straight on QSPI_SS (the 0 ohm link omitted), and the BOOTSEL switch's node driven by the
    // long-press block.
    assert_eq!(c.added_pins["mcu.FLASH.9"], "GND");
    assert_eq!(c.joined["mcu.FLASH_SS"], "/mcu/QSPI_SS");
    assert_eq!(c.taps["mcu.~USB_BOOT"], "BOOTSEL_SW");
}

#[test]
fn the_loose_bolt_solves_and_composes() {
    let s = solve(&read("boards/bolt-earring/loose.json"));
    assert!(s.ok, "{:?}", s.errors);
    assert!(s.exhaustive);
    // The three sensors on ADC pins; the IMU brings its own pull-ups.
    for net in ["VBSENSE", "VBUS_SENSE", "LIGHT"] {
        let row = s.pinout.iter().find(|r| r.net == net).unwrap();
        assert!(row.via.starts_with("ADC"), "{row:?}");
    }
    assert!(s.added.is_empty(), "{:?}", s.added);
    // The button's line reaches the MCU and the long-press timer both.
    let btn: BTreeSet<&str> = s.composition["nets"]["BTN_SENSE"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
    assert!(btn.contains("boot.IN") && btn.contains("latch.PRESSED") && btn.len() == 3, "{btn:?}");
    let c = composes(&s.composition);
    assert!(c.ok, "{:?}", c.errors);
}

#[test]
fn pinned_the_loose_bolt_is_the_blocks() {
    let mut loose = read("boards/bolt-earring/loose.json");
    let hand = read("boards/bolt-earring/compose.json");
    // The bolt's own GPIOs, and its designators.
    let pins: BTreeMap<&str, Value> = [
        ("imu.i2c", json!({ "SDA": "GPIO6", "SCL": "GPIO11" })),
        ("imu.int1", json!({ "IO": "GPIO8" })),
        ("chg.chrg", json!({ "IO": "GPIO19" })),
        ("chg.done", json!({ "IO": "GPIO28" })),
        ("latch.hold", json!({ "IO": "GPIO20" })),
        ("latch.pressed", json!({ "IO": "GPIO21" })),
        ("vbat_sense.out", json!({ "IN": "GPIO29" })),
        ("vbus_sense.out", json!({ "IN": "GPIO27" })),
        ("light.out", json!({ "IN": "GPIO26" })),
        ("sw_l.on", json!({ "IO": "GPIO1" })),
        ("sw_r.on", json!({ "IO": "GPIO0" })),
        ("sw_c.on", json!({ "IO": "GPIO22" })),
        ("shift_l.in", json!({ "IO": "GPIO18" })),
        ("shift_r.in", json!({ "IO": "GPIO17" })),
        ("shift_c.in", json!({ "IO": "GPIO16" })),
    ]
    .into();
    for c in loose["connect"].as_array_mut().unwrap() {
        c["pins"] = pins[c["from"].as_str().unwrap()].clone();
    }
    for (name, inst) in hand["instances"].as_object().unwrap() {
        loose["instances"][name]["designators"] = inst["designators"].clone();
    }
    let s = solve(&loose);
    assert!(s.ok, "{:?}", s.errors);
    let solved = composes(&s.composition);
    let hand = composes(&hand);
    assert!(solved.ok && hand.ok, "{:?} {:?}", solved.errors, hand.errors);
    let sets = |c: &compose::Composed| pin_sets(c.nets.values().map(|ps| ps.iter().cloned().collect()));
    assert_eq!(sets(&solved), sets(&hand));
    assert_eq!(solved.parts.keys().collect::<Vec<_>>(), hand.parts.keys().collect::<Vec<_>>());
}
