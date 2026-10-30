//! `ccomp shape`: a board's shape.json evaluated, the generators listed, a drawing imported; and
//! a board whose floorplan spec sets no outline gets its shape's outline and keep-outs.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{json, Value};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn ccomp(dir: &Path, args: &[&str]) -> Value {
    let out = Command::new(env!("CARGO_BIN_EXE_ccomp")).args(args).current_dir(dir).output().expect("ccomp runs");
    serde_json::from_slice(&out.stdout).expect("one JSON result")
}

#[test]
fn the_bolt_earrings_shape_is_its_outline() {
    let s = ccomp(&root(), &["-b", "bolt-earring", "shape"]);
    assert_eq!(s["ok"], true, "{}", s["warnings"]);
    let area = s["area_mm2"].as_f64().unwrap();
    assert!((area - 1118.07).abs() < 1.0, "{area}");
    let g = ccomp(&root(), &["shape", "--generators"]);
    assert!(g.as_array().unwrap().iter().any(|g| g["name"] == "bolt"));
}

#[test]
fn a_drawing_imports_and_a_shape_reaches_the_floorplanner() {
    let tmp = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("shape-cmd");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    std::fs::write(tmp.join("outline.svg"), r#"<svg width="60mm" height="50mm" viewBox="0 0 60 50"><rect id="edge" x="-5" y="-5" width="60" height="50"/></svg>"#).unwrap();
    let doc = ccomp(&tmp, &["shape", "--import", "outline.svg"]);
    assert_eq!(doc["features"][0]["name"], "edge");

    // The battery demo board, in a board directory of its own whose spec sets no outline, with
    // that drawing as its shape plus a mounting hole.
    let b = root().join("boards/rp2350-battery");
    let board = tmp.join("demo");
    std::fs::create_dir_all(&board).unwrap();
    let manifest = json!({
        "composition": b.join("compose.json"), "composed": b.join("composed.json"),
        "floorplan": b.join("floorplan.json"), "pcb": b.join("composed.kicad_pcb"),
        "packages": root().join("packages"),
    });
    std::fs::write(board.join("board.json"), manifest.to_string()).unwrap();
    let mut shape = doc.clone();
    shape["holes"] = json!([{ "x": 0.0, "y": 0.0, "d": 3.2, "clearance": 0.5 }]);
    std::fs::write(board.join("shape.json"), shape.to_string()).unwrap();

    let pr = ccomp(&tmp, &["-b", "demo", "floorplan", "--problem"]);
    let poly = pr["outline_polygon"].as_array().expect("the shape's outline");
    let xs: Vec<f64> = poly.iter().map(|p| p[0].as_f64().unwrap()).collect();
    assert_eq!((xs.iter().cloned().fold(f64::INFINITY, f64::min), xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max)), (-5.0, 55.0));
    let hole = pr["parts"].as_array().unwrap().iter().find(|p| p["reference"] == "keepout:hole 1").expect("the hole keeps parts away");
    assert_eq!(hole["bbox"], json!([-2.1, -2.1, 2.1, 2.1]));
}
