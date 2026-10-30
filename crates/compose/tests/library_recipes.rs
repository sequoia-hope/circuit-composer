//! The recipes in `library/recipes/` expand with their defaults, cite sources the library has,
//! and choose stocked parts.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde_json::{json, Value};

fn library() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../library")
}

#[test]
fn every_recipe_expands_and_cites_known_sources() {
    let (recipes, stock) = compose::load_library(&library()).unwrap();
    assert!(recipes.len() >= 6, "{:?}", recipes.keys());
    let sources: BTreeMap<String, Value> = serde_json::from_str(&std::fs::read_to_string(library().join("sources.json")).unwrap()).unwrap();
    for (name, r) in &recipes {
        let x = compose::recipe::expand(r, &BTreeMap::new(), &stock).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(!x.circuit.parts.is_empty(), "{name}");
        let cited: BTreeSet<&str> = r.citations.iter().filter_map(|c| c["source"].as_str()).collect();
        assert!(!cited.is_empty(), "{name} cites nothing");
        for s in cited {
            assert!(sources.contains_key(s), "{name} cites {s}, not in library/sources.json");
        }
        for (role, p) in &x.circuit.parts {
            assert!(p.lcsc.is_some(), "{name}: {role} has no LCSC number");
        }
    }
}

#[test]
fn the_led_resistor_and_the_chain() {
    let (recipes, stock) = compose::load_library(&library()).unwrap();
    // Red at 2 mA from 3.3 V: (3.3 - 1.8) / 2 mA = 750 ohm at least, so 1k (680 would let 2.2 mA
    // through).
    let x = compose::recipe::expand(&recipes["led"], &BTreeMap::new(), &stock).unwrap();
    assert_eq!(x.circuit.parts["R1"].value, "1k");
    assert_eq!(x.circuit.parts["D1"].lcsc.as_deref(), Some("C2286"));
    // Green may not light from 3.3 V.
    let e = compose::recipe::expand(&recipes["led"], &[("led".to_string(), json!("green"))].into(), &stock).unwrap_err();
    assert!(e.contains("may not light"), "{e}");
    let x = compose::recipe::expand(&recipes["led"], &[("led".to_string(), json!("green")), ("drive_v".to_string(), json!(5.0))].into(), &stock).unwrap();
    assert_eq!(x.circuit.parts["D1"].value, "GREEN");
    // 134 LEDs, a capacitor every six: 23 capacitors, the last by D133.
    let x = compose::recipe::expand(&recipes["led_chain"], &[("n".to_string(), json!(134)), ("cap_every".to_string(), json!(6))].into(), &stock).unwrap();
    assert_eq!(x.circuit.parts.keys().filter(|k| k.starts_with('D')).count(), 134);
    assert_eq!(x.circuit.parts.keys().filter(|k| k.starts_with('C')).count(), 23);
    assert_eq!(x.circuit.parts["C23"].near.as_deref(), Some("D133.1"));
    assert_eq!(x.circuit.nets["D133_DO"], ["D133.2", "D134.4"]);
    assert_eq!(x.circuit.ports["DOUT"].pins, ["D134.2"]);
    // I2C pull-ups inside a window: 1.5k to 2.1k leaves only 2k.
    let x = compose::recipe::expand(&recipes["i2c_pullups"], &[("rmin_ohm".to_string(), json!(1500)), ("rmax_ohm".to_string(), json!(2100))].into(), &stock).unwrap();
    assert_eq!(x.circuit.parts["R_SDA"].value, "2k");
}
