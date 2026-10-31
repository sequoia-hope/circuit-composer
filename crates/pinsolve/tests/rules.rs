//! The interface rules and the level checks, on the battery board.

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn board() -> Value {
    serde_json::from_str(&std::fs::read_to_string(root().join("boards/rp2350-battery/loose.json")).unwrap()).unwrap()
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

#[test]
fn a_bus_without_pull_ups_gets_a_pair() {
    let mut loose = board();
    loose["instances"]["imu"]["omit"] = json!(["R_SDA_PU", "R_SCL_PU"]);
    let s = solve(&loose);
    assert!(s.ok, "{:?}", s.errors);
    // Named for the bus it pulls up (i2c0_pu or i2c1_pu, whichever the solver chose).
    let (pu, _) = s.composition["instances"].as_object().unwrap().iter().find(|(_, i)| i["recipe"] == "i2c_pullups").unwrap();
    assert!(s.added.iter().any(|a| a.contains(pu.as_str())), "{:?}", s.added);
    let c = composes(&s.composition);
    assert!(c.ok, "{:?}", c.errors);
    // 400 kHz (the IMU's limit), ~40 pF: 967 ohm to 8.85k, so 4.7k on each line, to +3V3.
    for sig in ["SDA", "SCL"] {
        let imu = c.bindings["imu"].chip.clone();
        let (_, net) = c.nets.iter().find(|(n, ps)| !n.starts_with('/') && ps.iter().any(|p| p.starts_with(&format!("{imu}.")))  && n.ends_with(sig)).unwrap();
        let r = net.iter().find(|p| p.starts_with('R')).unwrap().split('.').next().unwrap().to_string();
        assert_eq!(c.parts[&r].value, "4.7k");
        assert!(c.nets["+3V3"].contains(&format!("{r}.2")), "{r}");
    }
}

#[test]
fn too_many_pull_ups_in_parallel_are_omitted() {
    let mut loose = board();
    // Two IMUs, each with 1.5k pull-ups: 750 ohm in parallel is below Rp(min).
    let strong = json!({ "R_SDA_PU": { "value": "1.5k", "lcsc": "C25867" }, "R_SCL_PU": { "value": "1.5k", "lcsc": "C25867" } });
    loose["instances"]["imu"]["parts"] = strong.clone();
    loose["instances"]["imu2"] = loose["instances"]["imu"].clone();
    loose["nets"]["GND"].as_array_mut().unwrap().push(json!("imu2.GND"));
    loose["nets"]["+3V3"].as_array_mut().unwrap().push(json!("imu2.VDD"));
    loose["connect"].as_array_mut().unwrap().push(json!({ "from": "imu2.i2c", "to": "mcu" }));
    let s = solve(&loose);
    assert!(s.ok, "{:?}", s.errors);
    assert_eq!(s.composition["instances"]["imu2"]["omit"], json!(["R_SCL_PU", "R_SDA_PU"]));
    assert!(s.added.iter().any(|a| a.contains("one pull-up pair is enough")), "{:?}", s.added);
    assert!(composes(&s.composition).ok);
}

#[test]
fn a_5v_status_line_goes_to_a_5v_tolerant_pin() {
    let mut loose = board();
    loose["no_connect"] = json!(["reg.EN"]);
    loose["connect"].as_array_mut().unwrap().push(json!({ "from": "chg.stat", "to": "mcu", "name": "CHG_STAT" }));
    let s = solve(&loose);
    assert!(s.ok, "{:?}", s.errors);
    let row = s.pinout.iter().find(|r| r.net == "CHG_STAT").unwrap();
    assert!(!row.partner.contains("ADC"), "{row:?}");
    // STAT's high is VBUS (up to 5.5 V): within the FT pin's limit only while IOVDD is up.
    assert!(s.warnings.iter().any(|w| w.contains("chg.stat") && w.contains("IOVDD")), "{:?}", s.warnings);

    // An ADC pin isn't fault tolerant: refused, and why.
    loose["connect"].as_array_mut().unwrap().last_mut().unwrap()["pins"] = json!({ "IO": "GPIO26" });
    let s = solve(&loose);
    assert!(!s.ok, "{:?}", s.pinout);
    assert!(s.errors.iter().any(|e| e.contains("above what mcu.GPIO26_ADC0")), "{:?}", s.errors);
}

#[test]
fn an_led_chain_on_the_cell_needs_a_level_shifter() {
    let mut loose = board();
    loose["instances"]["leds"] = json!({ "recipe": "led_chain", "params": { "n": 12, "cap_every": 3 } });
    loose["nets"]["VBAT"].as_array_mut().unwrap().push(json!("leds.VDD"));
    loose["nets"]["GND"].as_array_mut().unwrap().push(json!("leds.GND"));
    loose["no_connect"].as_array_mut().unwrap().push(json!("leds.DOUT"));
    let mut direct = loose.clone();
    direct["connect"].as_array_mut().unwrap().push(json!({ "from": "leds.din", "to": "mcu", "name": "LED_DATA" }));
    let s = solve(&direct);
    // DI reads high above 0.7 VDD, 2.96 V at the cell's 4.23 V; a GPIO's high is 2.62 V.
    assert!(!s.ok);
    assert!(s.errors.iter().any(|e| e.contains("below what leds.DIN")), "{:?}", s.errors);

    // Through an N-MOSFET to the cell's rail it works.
    loose["instances"]["shift"] = json!({ "recipe": "nfet_level_shift" });
    loose["nets"]["VBAT"].as_array_mut().unwrap().push(json!("shift.VOUT"));
    loose["nets"]["GND"].as_array_mut().unwrap().push(json!("shift.GND"));
    loose["connect"].as_array_mut().unwrap().push(json!({ "from": "shift.in", "to": "mcu", "name": "LED_DATA_N" }));
    loose["connect"].as_array_mut().unwrap().push(json!({ "from": "leds.din", "to": "shift", "name": "LED_DATA" }));
    let s = solve(&loose);
    assert!(s.ok, "{:?}", s.errors);
    let c = composes(&s.composition);
    assert!(c.ok, "{:?}", c.errors);
    assert_eq!(c.parts.values().filter(|p| p.value == "WS2816C-1313").count(), 12);
    assert_eq!(c.parts.values().filter(|p| p.instance.as_deref() == Some("leds") && p.value == "100n").count(), 4);
}
