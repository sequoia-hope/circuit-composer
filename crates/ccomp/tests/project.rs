//! `--project` / `--board`: a command not given its inputs takes them from the board's files.
//! This repository's boards resolve in place through their `board.json`; an easy_pcb-style
//! project (boards under `compose/`) resolves with no manifest and this repository's packages.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn ccomp(dir: &Path, args: &[&str]) -> Value {
    let out = Command::new(env!("CARGO_BIN_EXE_ccomp")).args(args).current_dir(dir).env_remove("CCOMP_PACKAGES").output().expect("ccomp runs");
    serde_json::from_slice(&out.stdout).expect("one JSON result")
}

#[test]
fn this_repositorys_boards_resolve_in_place() {
    let list = ccomp(&root(), &["boards"]);
    for b in ["bolt-earring", "easy2350", "flame-earring", "rp2350-battery"] {
        assert!(list["boards"].as_array().unwrap().iter().any(|n| n == b), "{b} not listed: {list:#}");
    }
    let b = ccomp(&root(), &["--board", "easy2350", "boards"]);
    assert_eq!(b["manifest"], true);
    for (k, there) in b["exists"].as_object().unwrap() {
        assert_eq!(there, true, "easy2350's {k} is missing: {}", b[k]);
    }
}

#[test]
fn project_mode_runs_what_explicit_paths_run() {
    let r = root();
    let explicit = ccomp(&r, &["compose", "boards/rp2350-battery/compose.json"]);
    assert_eq!(explicit["ok"], true);
    assert_eq!(ccomp(&r, &["-b", "rp2350-battery", "compose"]), explicit);

    let explicit = ccomp(&r, &["validate", "boards/easy2350/intent.json"]);
    assert_eq!(ccomp(&r, &["-C", ".", "-b", "easy2350", "validate"]), explicit);

    let base = ["floorplan", "--seed", "2", "--moves", "20000"];
    let explicit = ccomp(&r, &[&base[..], &["boards/rp2350-battery/composed.kicad_pcb", "--composed", "boards/rp2350-battery/composed.json", "--spec", "boards/rp2350-battery/floorplan.json"]].concat());
    assert_eq!(ccomp(&r, &[&["-b", "rp2350-battery"], &base[..]].concat()), explicit);
}

#[test]
fn an_easy_pcb_project_keeps_its_boards_under_compose() {
    let project = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("project-mode");
    let _ = std::fs::remove_dir_all(&project);
    std::fs::create_dir_all(project.join("compose/demo")).unwrap();
    std::fs::copy(root().join("boards/rp2350-battery/compose.json"), project.join("compose/demo/compose.json")).unwrap();

    // One board, no manifest: it is the board, with the spec's layout and this repository's packages.
    let b = ccomp(&root(), &["-C", project.to_str().unwrap(), "boards"]);
    assert_eq!(b["name"], "demo");
    assert_eq!(b["manifest"], false);
    assert!(b["intent"].as_str().unwrap().ends_with("compose/demo/intent.json"));
    assert!(b["pcb"].as_str().unwrap().ends_with("hw/demo/a/demo.kicad_pcb"));
    assert_eq!(b["exists"]["packages"], true);

    let theirs = ccomp(&project, &["compose"]);
    let ours = ccomp(&root(), &["compose", "boards/rp2350-battery/compose.json"]);
    assert_eq!(theirs["ok"], true, "{theirs:#}");
    assert_eq!(theirs["nets"], ours["nets"]);

    // A command with nothing to go on says what it needs.
    let none = ccomp(&root(), &["floorplan"]);
    assert_eq!(none["ok"], false);
    assert!(none["error"].as_str().unwrap().contains("--project"));
}
