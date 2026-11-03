//! The buck generator on the RP2350 package's own test bench, at a coarse grid so it runs in
//! seconds: it finds a legal arrangement, and its estimates match what the verifier measures.

use std::collections::BTreeMap;
use std::path::Path;

use blockgen::buck::{generate, Params};
use verify::buck::BuckRoles;

#[test]
fn coarse_search_on_the_rp2350_bench() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/rp2350/layouts/qfn60-4layer");
    let board = pcb::Board::load(&dir.join("fixture.kicad_pcb")).unwrap();
    let roles: BTreeMap<String, String> = serde_json::from_str(&std::fs::read_to_string(dir.join("roles.json")).unwrap()).unwrap();
    let roles = BuckRoles::from_map(&roles).unwrap();
    let params = Params { grid: 0.2, beam: 20, beam_inductor: 60, ..Params::default() };
    let g = generate(&board, &roles, &params).unwrap();
    let e = g.estimate;
    assert!(e.feedback_clearance_mm >= params.fb_clearance_min);
    // The vendor reference measures 1.06 mm² hot loop; a coarse search should come close.
    assert!(e.hot_loop_area_mm2 < 1.3, "hot loop {}", e.hot_loop_area_mm2);
    assert_eq!(g.layout.parts.len(), 3);
    assert_eq!(g.layout.vias.len(), params.gnd_vias);
    assert_eq!(g.layout.keepouts.len(), 1);
    assert!(g.layout.tracks.iter().all(|t| ["vin", "sw", "gnd", "out"].contains(&t.net.as_str())));
}

#[test]
fn coarse_search_with_a_divider_on_the_tlv62569_bench() {
    // A SOT-23 buck: VIN across the package from SW and GND, FB behind a divider. Ground returns
    // through the plane, and the divider sits by the FB pin, clear of the switch node.
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/tlv62569/layouts/sot23-5-4layer");
    let board = pcb::Board::load(&dir.join("fixture.kicad_pcb")).unwrap();
    let roles: BTreeMap<String, String> = serde_json::from_str(&std::fs::read_to_string(dir.join("roles.json")).unwrap()).unwrap();
    let roles = BuckRoles::from_map(&roles).unwrap();
    let saved: Params = serde_json::from_str(&std::fs::read_to_string(dir.join("params.json")).unwrap()).unwrap();
    let params = Params { grid: 0.25, beam: 8, beam_inductor: 20, power_widths: vec![0.4], divider_attempts: 10, ..saved };
    let g = generate(&board, &roles, &params).unwrap();
    let e = g.estimate;
    assert!(e.feedback_clearance_mm >= params.fb_clearance_min, "{e:?}");
    assert!(e.hot_loop_area_mm2 < 5.0, "hot loop {}", e.hot_loop_area_mm2);
    let parts: Vec<&str> = g.layout.parts.keys().map(String::as_str).collect();
    assert_eq!(parts, ["cff", "cin", "cout", "inductor", "rfb_bot", "rfb_top"]);
    assert!(g.layout.keepouts.is_empty());
    // Ground through the plane: the ground pin's via, CIN's and COUT's, and the divider's foot.
    assert!(g.layout.vias.len() >= 1 + 2 * params.gnd_vias, "{} vias", g.layout.vias.len());
    assert!(g.layout.tracks.iter().any(|t| t.net == "fb"));
}
