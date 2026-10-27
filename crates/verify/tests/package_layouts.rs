//! Every published package layout still passes its own block's constraints when measured on
//! its committed test-bench board.

use std::collections::BTreeMap;
use std::path::Path;

use verify::buck::{check, BuckRoles};
use verify::Limit;

#[test]
fn rp2350_core_reg_qfn60_4layer() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/rp2350/layouts/qfn60-4layer");
    let board = pcb::Board::load(&dir.join("fixture.kicad_pcb")).unwrap();
    let roles: BTreeMap<String, String> = serde_json::from_str(&std::fs::read_to_string(dir.join("roles.json")).unwrap()).unwrap();
    let roles = BuckRoles::from_map(&roles).unwrap();
    let mut limits = BTreeMap::new();
    limits.insert("feedback_clearance_mm".to_string(), Limit { op: ">=", value: 0.3 });
    limits.insert("switch_node_underlap_mm2".to_string(), Limit { op: "<=", value: 0.0 });
    let r = check(&board, "core_reg", &roles, &limits).unwrap();
    assert!(r.geometry.routed, "the block's copper joins every loop");
    for m in &r.measurements {
        assert_ne!(m.passed, Some(false), "{} = {} fails {:?}", m.check, m.measured, m.limit);
    }
    // No worse than the measurements published with the layout.
    let published: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("verify.json")).unwrap()).unwrap();
    for m in &r.measurements {
        let was = published["measurements"][m.check].as_f64().unwrap();
        assert!((m.measured - was).abs() < 1e-3, "{} moved: {} now, {} published", m.check, m.measured, was);
    }
}
