//! The compiler places part-anchored regions where their part is, expands classes to nets, and
//! the region check catches copper that breaks a rule. On the rp2350 package's test bench, whose
//! copper is known: an LX run on F.Cu, ground vias, and a GND plane on In1.Cu.

use std::path::Path;

use compile::{check_regions, compile};
use intent::IntentFile;

fn bench() -> pcb::Board {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/rp2350/layouts/qfn60-4layer");
    pcb::Board::load(&dir.join("fixture.kicad_pcb")).unwrap()
}

fn intent(regions: serde_json::Value) -> IntentFile {
    serde_json::from_value(serde_json::json!({
        "schema_version": "0.1",
        "board": {"name": "bench"},
        "sources": {"s": {"title": "test"}},
        "nets": {
            "/VREG_LX": {"class": "SWITCH"},
            "GND": {"class": "GROUND"}
        },
        "regions": regions
    }))
    .unwrap()
}

fn prov() -> serde_json::Value {
    serde_json::json!({"source": "s", "kind": "expert"})
}

#[test]
fn anchored_region_lands_on_its_part() {
    let b = bench();
    let i = intent(serde_json::json!([{
        "name": "around_l1", "kind": "polygon", "anchor": "L1",
        "polygon": [[-0.1, -0.1], [0.1, -0.1], [0.1, 0.1], [-0.1, 0.1]],
        "rules": [{"applies_to": {"class": "SWITCH"}, "mode": "avoid", "weight": 0.5}]
    }]));
    let (r, _) = compile(&i, &b).unwrap();
    let l1 = &b.parts["L1"];
    let c = pcb::geom::centroid(&r.regions[0].polygon);
    assert!(pcb::geom::dist(c, l1.at) < 1e-9, "{c:?} vs {:?}", l1.at);
    assert_eq!(r.regions[0].rules[0].nets.as_deref(), Some(&["/VREG_LX".to_string()][..]));
    assert_eq!(r.regions[0].rules[0].weight, Some(5.0));
}

#[test]
fn check_catches_never_must_and_vias() {
    let b = bench();
    let u1 = b.parts["U1"].at;
    let big = serde_json::json!([[u1[0] - 20.0, u1[1] - 20.0], [u1[0] + 20.0, u1[1] - 20.0], [u1[0] + 20.0, u1[1] + 20.0], [u1[0] - 20.0, u1[1] + 20.0]]);
    let tiny = serde_json::json!([[u1[0] - 0.1, u1[1] - 0.1], [u1[0] + 0.1, u1[1] - 0.1], [u1[0] + 0.1, u1[1] + 0.1], [u1[0] - 0.1, u1[1] + 0.1]]);
    let i = intent(serde_json::json!([
        {"name": "no_lx", "kind": "polygon", "polygon": big, "layers": ["F.Cu"], "rules": [{"applies_to": {"net": "/VREG_LX"}, "mode": "never"}], "provenance": prov()},
        {"name": "no_plane", "kind": "polygon", "polygon": big, "layers": ["In1.Cu"], "rules": [{"applies_to": {"net": "GND"}, "mode": "never"}], "provenance": prov()},
        {"name": "no_gnd_vias", "kind": "polygon", "polygon": big, "rules": [{"applies_to": {"class": "GROUND"}, "vias": true, "mode": "never"}], "provenance": prov()},
        {"name": "lx_stays_home", "kind": "polygon", "polygon": tiny, "layers": ["F.Cu"], "rules": [{"applies_to": {"net": "/VREG_LX"}, "mode": "must"}], "provenance": prov()}
    ]));
    let (r, notes) = compile(&i, &b).unwrap();
    assert!(notes.is_empty(), "{notes:?}");
    let f = check_regions(&b, &r);
    let hit = |region: &str, item: &str| f.iter().any(|x| x.region == region && x.item == item && x.violation);
    assert!(hit("no_lx", "track"), "LX tracks on F.Cu inside a never region");
    assert!(hit("no_plane", "pour"), "the GND plane on In1.Cu");
    assert!(hit("no_gnd_vias", "via"), "the bench's two ground vias");
    assert!(hit("lx_stays_home", "track"), "LX copper outside a must region");
    // And nothing where nothing breaks a rule: the same never rule on a layer with no LX copper.
    let quiet = intent(serde_json::json!([
        {"name": "no_lx_inner", "kind": "polygon", "polygon": big, "layers": ["In2.Cu"], "rules": [{"applies_to": {"net": "/VREG_LX"}, "mode": "never"}], "provenance": prov()}
    ]));
    let (r, _) = compile(&quiet, &b).unwrap();
    assert!(check_regions(&b, &r).is_empty());
}

#[test]
fn copper_touching_an_edge_is_outside() {
    let b = bench();
    // A region whose edge passes exactly one via radius from a bench ground via.
    let v = b.vias.iter().find(|v| v.net == "GND").unwrap();
    let x = v.at[0] + v.size / 2.0;
    let edge = |x0: f64| serde_json::json!([[x0, v.at[1] - 1.0], [x0 + 2.0, v.at[1] - 1.0], [x0 + 2.0, v.at[1] + 1.0], [x0, v.at[1] + 1.0]]);
    let touching = intent(serde_json::json!([{"name": "touch", "kind": "polygon", "polygon": edge(x), "rules": [{"applies_to": "all_nets", "vias": true, "mode": "never"}], "provenance": prov()}]));
    let (r, _) = compile(&touching, &b).unwrap();
    assert!(!check_regions(&b, &r).iter().any(|f| f.item == "via" && pcb::geom::dist(f.location, v.at) < 1e-9), "a via that only touches the edge");
    let entering = intent(serde_json::json!([{"name": "enter", "kind": "polygon", "polygon": edge(x - 0.01), "rules": [{"applies_to": "all_nets", "vias": true, "mode": "never"}], "provenance": prov()}]));
    let (r, _) = compile(&entering, &b).unwrap();
    assert!(check_regions(&b, &r).iter().any(|f| f.item == "via" && pcb::geom::dist(f.location, v.at) < 1e-9), "a via 10 um inside");
}
