//! Every published package layout still passes its own block's constraints when measured on
//! its committed test-bench board.

use std::collections::BTreeMap;
use std::path::Path;

use verify::buck::{check, BuckRoles};
use verify::Limit;

/// "<= 0" or ">= 0.3", as verify.json writes its limits.
fn parse_limit(s: &str) -> Limit {
    let (op, v) = s.split_at(2);
    let op = match op {
        "<=" => "<=",
        ">=" => ">=",
        _ => panic!("limit {s}"),
    };
    Limit { op, value: v.trim().parse().unwrap() }
}

fn check_layout(name: &str) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/rp2350/layouts").join(name);
    let board = pcb::Board::load(&dir.join("fixture.kicad_pcb")).unwrap();
    let roles: BTreeMap<String, String> = serde_json::from_str(&std::fs::read_to_string(dir.join("roles.json")).unwrap()).unwrap();
    let roles = BuckRoles::from_map(&roles).unwrap();
    let published: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("verify.json")).unwrap()).unwrap();
    let limits: BTreeMap<String, Limit> = published["limits"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| (k.clone(), parse_limit(v.as_str().unwrap())))
        .collect();
    let r = check(&board, "core_reg", &roles, &limits).unwrap();
    assert!(r.geometry.routed, "{name}: the block's copper joins every loop");
    for m in &r.measurements {
        assert_ne!(m.passed, Some(false), "{name}: {} = {} fails {:?}", m.check, m.measured, m.limit);
    }
    // No worse than the measurements published with the layout.
    for m in &r.measurements {
        let was = published["measurements"][m.check].as_f64().unwrap();
        assert!((m.measured - was).abs() < 1e-3, "{name}: {} moved: {} now, {} published", m.check, m.measured, was);
    }
}

#[test]
fn rp2350_core_reg_qfn60_4layer() {
    check_layout("qfn60-4layer");
}

#[test]
fn rp2350_core_reg_qfn60_2layer() {
    check_layout("qfn60-2layer");
}

#[test]
fn rp2350_core_reg_qfn80_4layer() {
    check_layout("qfn80-4layer");
}

#[test]
fn rp2350_core_reg_qfn80_2layer() {
    check_layout("qfn80-2layer");
}
