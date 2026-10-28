//! The research library in this repository stays whole: `ccomp library check` passes on it, and
//! a query finds a dispute from both sides.

use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

fn ccomp(args: &[&str]) -> (Value, bool) {
    let out = Command::new(env!("CARGO_BIN_EXE_ccomp")).args(args).current_dir(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")).output().expect("ccomp runs");
    (serde_json::from_slice(&out.stdout).expect("one JSON result"), out.status.success())
}

#[test]
fn library_is_whole() {
    let (r, ok) = ccomp(&["library", "check"]);
    assert!(ok, "{:#}", r["errors"]);
    assert!(r["by_set"]["packages/rp2350"].as_u64().unwrap() > 0);
    assert!(r["by_set"]["library/buck"].as_u64().unwrap() > 0);
}

#[test]
fn a_dispute_is_seen_from_both_sides() {
    // The datasheet's cut-out under the RP2350's switch node, against TI's EMI measurement.
    let (r, _) = ccomp(&["library", "query", "--id", "rp2350.vreg.lx_cutout"]);
    let c = &r["claims"][0];
    assert!(c["links"].as_array().unwrap().iter().any(|l| l["contradicts"] == "buck.sw.keep_ground_under"));
    let (r, _) = ccomp(&["library", "query", "--scope", "circuit=buck", "--status", "contested"]);
    let ids: Vec<&str> = r["claims"].as_array().unwrap().iter().map(|c| c["id"].as_str().unwrap()).collect();
    assert!(ids.contains(&"buck.sw.keep_ground_under"), "{ids:?}");
    assert!(r["claims"].as_array().unwrap().iter().all(|c| c["sources"].as_object().is_some_and(|s| !s.is_empty())));
}
