//! End-to-end integration tests for `ccomp shape {publish, status, pull, take-over}`.

use std::path::{Path, PathBuf};
use std::process::Command;

use std::sync::atomic::{AtomicU64, Ordering};

use board_interface::{
    Author, ChangeKind, Classification, Driver, ElementKind, Handover, Point, compare, hash, read,
    write,
};
use serde_json::{Value, json};
use shape::loop_area_mm2;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn find_golden_dir() -> PathBuf {
    if let Ok(p) = std::env::var("BOARD_INTERFACE_GOLDEN") {
        let pb = PathBuf::from(p);
        if pb.join("easy2350.json").is_file() {
            return pb;
        }
    }
    let cargo_home = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cargo")));
    if let Some(ch) = cargo_home {
        if let Ok(checkouts) = std::fs::read_dir(ch.join("git/checkouts")) {
            for entry in checkouts.flatten() {
                if entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("board-interface-")
                {
                    if let Ok(revs) = std::fs::read_dir(entry.path()) {
                        for rev in revs.flatten() {
                            let g = rev.path().join("golden");
                            if g.join("easy2350.json").is_file() {
                                return g;
                            }
                        }
                    }
                }
            }
        }
    }
    for rel in [
        "../../../board-interface/golden",
        "/home/sequoia/Software/board-interface/golden",
    ] {
        let p = Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
        if p.join("easy2350.json").is_file() {
            return p;
        }
    }
    panic!("could not locate board-interface golden fixtures");
}

fn ccomp_run(dir: &Path, args: &[&str]) -> (Value, bool) {
    let out = Command::new(env!("CARGO_BIN_EXE_ccomp"))
        .args(args)
        .current_dir(dir)
        .output()
        .expect("ccomp runs");
    let val: Value = serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("expected JSON on stdout ({e}): {}", String::from_utf8_lossy(&out.stdout)));
    (val, out.status.success())
}

fn fresh_tmp(name: &str) -> PathBuf {
    static SEQ: AtomicU64 = AtomicU64::new(1);
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let p = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("{name}-{}-{nanos}-{n}", std::process::id()));
    std::fs::create_dir_all(&p).unwrap();
    p
}

#[test]
fn acceptance_test_1_easy2350_pull_floorplan_take_over_publish() {
    let proj = fresh_tmp("shape-accept-easy2350");
    let golden_dir = find_golden_dir();
    let golden = read(
        std::fs::read(golden_dir.join("easy2350.dxf")).unwrap(),
        std::fs::read(golden_dir.join("easy2350.json")).unwrap(),
    )
    .unwrap();

    // Set up a board directory with a pre-existing Composer recipe (so `pull` tests setting it
    // aside with a warning) and no floorplan outline (so `floorplan --problem` uses `shape.json`).
    let board_dir = proj.join("boards/easy2350");
    std::fs::create_dir_all(&board_dir).unwrap();
    let bat = root().join("boards/rp2350-battery");
    std::fs::write(
        board_dir.join("board.json"),
        json!({
            "composition": bat.join("compose.json"),
            "composed": bat.join("composed.json"),
            "floorplan": bat.join("floorplan.json"),
            "pcb": bat.join("composed.kicad_pcb"),
            "packages": root().join("packages"),
        })
        .to_string(),
    )
    .unwrap();
    std::fs::write(
        board_dir.join("shape.json"),
        json!({
            "features": [{
                "name": "draft_box",
                "op": "add",
                "geom": { "type": "rect", "x": 113.5, "y": 97.5, "w": 27.0, "h": 45.0, "r": 1.0 }
            }],
            "corner_radius": 0.5
        })
        .to_string(),
    )
    .unwrap();

    // 1. Pull golden/easy2350.
    let golden_stem = golden_dir.join("easy2350");
    let (pulled, ok) = ccomp_run(
        &proj,
        &[
            "-b",
            "easy2350",
            "shape",
            "pull",
            "--interface",
            golden_stem.to_str().unwrap(),
        ],
    );
    assert!(ok && pulled["ok"] == true, "{pulled}");
    assert_eq!(pulled["driver"], "kicad");
    assert_eq!(pulled["locked"], true);
    assert_eq!(pulled["geometry_hash"], golden.manifest.geometry_hash);
    assert!(
        pulled["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w.as_str().unwrap().contains("set aside")),
        "expected set-aside warning: {pulled}"
    );

    // 2. Status against the followed golden is `in_sync`.
    let (st, ok) = ccomp_run(&proj, &["-b", "easy2350", "shape", "status"]);
    assert!(ok && st["ok"] == true, "{st}");
    assert_eq!(st["in_sync"], true);
    assert_eq!(st["classification"], "in_sync");
    assert_eq!(st["changes"], json!([]));

    // 3. Floorplanner sees the pulled synced geometry (polygonized within ARC_TOLERANCE).
    let (pr, ok) = ccomp_run(&proj, &["-b", "easy2350", "floorplan", "--problem"]);
    assert!(ok, "{pr}");
    let poly = pr["outline_polygon"].as_array().expect("pulled outline reaches floorplanner");
    assert!(poly.len() > 8, "rounded corners polygonized: {} pts", poly.len());
    let xs: Vec<f64> = poly.iter().map(|p| p[0].as_f64().unwrap()).collect();
    let ys: Vec<f64> = poly.iter().map(|p| p[1].as_f64().unwrap()).collect();
    let xmin = xs.iter().copied().fold(f64::INFINITY, f64::min);
    let xmax = xs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let ymin = ys.iter().copied().fold(f64::INFINITY, f64::min);
    let ymax = ys.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    assert!((xmin - 100.0).abs() < 1e-6 && (xmax - 121.0).abs() < 1e-6);
    assert!((ymin - 100.0).abs() < 1e-6 && (ymax - 151.0).abs() < 1e-6);

    // 3b. Refuses `publish` while KiCad (`golden/easy2350`'s driver) controls the outline.
    let (refuse_locked, ok) = ccomp_run(&proj, &["-b", "easy2350", "shape", "publish"]);
    assert!(!ok && refuse_locked["ok"] == false, "{refuse_locked}");
    assert!(
        refuse_locked["error"].as_str().unwrap().contains("KiCad"),
        "{refuse_locked}"
    );

    // 4. Take over the outline explicitly.
    let (to, ok) = ccomp_run(&proj, &["-b", "easy2350", "shape", "take-over"]);
    assert!(ok && to["ok"] == true, "{to}");
    assert_eq!(to["driver"], "composer");
    assert_eq!(to["previous_driver"], "kicad");
    assert_eq!(to["from_revision"], golden.manifest.revision);
    assert_eq!(to["kept_ids"], 8);
    assert!(to["warnings"].as_array().unwrap().len() >= 2, "{to}");

    // 5. Publish after take-over without edits: writes interface/easy2350/board-shape.{dxf,json}
    //    with identical geometry_hash, CleanHandover metadata, and true Arcs preserved.
    let (pub_res, ok) = ccomp_run(
        &proj,
        &[
            "-b",
            "easy2350",
            "shape",
            "publish",
            "--source",
            "0123456789abcdef0123456789abcdef01234567",
            "--at",
            "2026-11-03T20:00:00Z",
        ],
    );
    assert!(ok && pub_res["ok"] == true, "{pub_res}");
    assert_eq!(pub_res["wrote"], true);
    assert_eq!(pub_res["driver"], "composer");
    assert_eq!(pub_res["parent"], golden.manifest.revision);
    assert_eq!(pub_res["last_handover"]["by"], "composer");
    assert_eq!(pub_res["last_handover"]["from_revision"], golden.manifest.revision);
    assert_eq!(pub_res["geometry_hash"], golden.manifest.geometry_hash);
    assert_eq!(pub_res["approximate"], false);

    let out_dir = proj.join("interface/easy2350");
    let republished = read(
        std::fs::read(out_dir.join("board-shape.dxf")).unwrap(),
        std::fs::read(out_dir.join("board-shape.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(hash(&republished), golden.manifest.geometry_hash);
    assert_eq!(
        republished
            .outline
            .elements
            .iter()
            .filter(|e| matches!(e.kind, ElementKind::Arc { .. }))
            .count(),
        4,
        "all 4 corner arcs must remain true Arcs"
    );
    assert_eq!(
        republished
            .outline
            .elements
            .iter()
            .filter(|e| matches!(e.kind, ElementKind::Line { .. }))
            .count(),
        4
    );
    let cmp = compare(&golden, &republished);
    assert_eq!(cmp.classification, Classification::CleanHandover);
    assert!(cmp.changes.is_empty());
}

#[test]
fn all_three_valid_goldens_round_trip_through_ccomp() {
    let proj = fresh_tmp("shape-all-goldens");
    let golden_dir = find_golden_dir();

    for (stem, board_name) in [
        ("easy2350", "easy2350"),
        ("rounded_rect", "rounded_carrier"),
        ("round_board", "round_sensor"),
    ] {
        let golden = read(
            std::fs::read(golden_dir.join(format!("{stem}.dxf"))).unwrap(),
            std::fs::read(golden_dir.join(format!("{stem}.json"))).unwrap(),
        )
        .unwrap();
        let bdir = proj.join("boards").join(board_name);
        std::fs::create_dir_all(&bdir).unwrap();

        let src_stem = golden_dir.join(stem);
        let (pulled, ok) = ccomp_run(
            &proj,
            &[
                "-b",
                board_name,
                "shape",
                "pull",
                "--interface",
                src_stem.to_str().unwrap(),
            ],
        );
        assert!(ok && pulled["ok"] == true, "{stem}: {pulled}");
        assert_eq!(pulled["locked"], true);

        if stem == "round_board" {
            let bat = root().join("boards/rp2350-battery");
            std::fs::write(
                bdir.join("board.json"),
                json!({
                    "composition": bat.join("compose.json"),
                    "composed": bat.join("composed.json"),
                    "floorplan": bat.join("floorplan.json"),
                    "pcb": bat.join("composed.kicad_pcb"),
                    "packages": root().join("packages"),
                })
                .to_string(),
            )
            .unwrap();
            let (pr, ok) = ccomp_run(&proj, &["-b", board_name, "floorplan", "--problem"]);
            assert!(ok, "{pr}");
            assert!(pr["outline_polygon"].as_array().unwrap().len() >= 64);
            let refs: Vec<&str> = pr["parts"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|p| p["reference"].as_str())
                .collect();
            assert!(refs.contains(&"keepout:cut-out 1"), "{refs:?}");
            assert!(refs.contains(&"keepout:hole 4"), "{refs:?}");
        }

        let (to, ok) = ccomp_run(&proj, &["-b", board_name, "shape", "take-over"]);
        assert!(ok && to["ok"] == true, "{stem}: {to}");

        let (pub_res, ok) = ccomp_run(
            &proj,
            &[
                "-b",
                board_name,
                "shape",
                "publish",
                "--source",
                "0123456789abcdef0123456789abcdef01234567",
                "--at",
                "2026-11-03T20:05:00Z",
            ],
        );
        assert!(ok && pub_res["ok"] == true, "{stem}: {pub_res}");
        assert_eq!(pub_res["wrote"], true, "{stem}");
        assert_eq!(pub_res["geometry_hash"], golden.manifest.geometry_hash, "{stem}");
        assert_eq!(pub_res["approximate"], false, "{stem}");

        let out_dir = proj.join("interface").join(board_name);
        let round_tripped = read(
            std::fs::read(out_dir.join("board-shape.dxf")).unwrap(),
            std::fs::read(out_dir.join("board-shape.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(hash(&round_tripped), golden.manifest.geometry_hash, "{stem}");
        assert_eq!(
            round_tripped.outline.elements.len(),
            golden.outline.elements.len(),
            "{stem}"
        );
        assert_eq!(round_tripped.cutouts.len(), golden.cutouts.len(), "{stem}");
        assert_eq!(round_tripped.holes.len(), golden.holes.len(), "{stem}");
        assert_eq!(round_tripped.keepouts.len(), golden.keepouts.len(), "{stem}");
    }
}

#[test]
fn composer_authored_shapes_publish_true_arcs_and_generator_is_approximate() {
    let proj = fresh_tmp("shape-authored");

    let cases = [
        (
            "rect_r",
            json!({
                "features": [{
                    "name": "body",
                    "op": "add",
                    "geom": { "type": "rect", "x": 20.0, "y": 15.0, "w": 40.0, "h": 30.0, "r": 4.0 }
                }],
                "corner_radius": 2.0
            }),
            4usize,
            0usize,
        ),
        (
            "circle_board",
            json!({
                "features": [{
                    "name": "disc",
                    "op": "add",
                    "geom": { "type": "circle", "x": 0.0, "y": 0.0, "d": 30.0 }
                }]
            }),
            0usize,
            1usize,
        ),
        (
            "slot_board",
            json!({
                "features": [{
                    "name": "pill",
                    "op": "add",
                    "geom": { "type": "slot", "x1": -10.0, "y1": 0.0, "x2": 10.0, "y2": 0.0, "w": 12.0 }
                }]
            }),
            2usize,
            0usize,
        ),
        (
            "rect_plus_circle",
            json!({
                "thickness": 2.0,
                "features": [
                    { "name": "plate", "op": "add", "geom": { "type": "rect", "x": 0.0, "y": 0.0, "w": 30.0, "h": 20.0, "r": 2.0 } },
                    { "name": "tab", "op": "add", "geom": { "type": "circle", "x": 15.0, "y": 0.0, "d": 14.0 } },
                    { "name": "window", "op": "subtract", "geom": { "type": "slot", "x1": -6.0, "y1": 0.0, "x2": 2.0, "y2": 0.0, "w": 4.0 } }
                ],
                "holes": [{ "x": -10.0, "y": 0.0, "d": 3.2, "plated": true, "clearance": 0.4 }],
                "keepouts": [{
                    "name": "antenna",
                    "side": "front",
                    "max_height": 2.5,
                    "geom": { "type": "rect", "x": -8.0, "y": -6.0, "w": 6.0, "h": 4.0, "r": 0.5 }
                }]
            }),
            5usize,
            0usize,
        ),
    ];

    for (name, shape_doc, min_arcs, want_circles) in cases {
        let bdir = proj.join("boards").join(name);
        std::fs::create_dir_all(&bdir).unwrap();
        std::fs::write(bdir.join("shape.json"), shape_doc.to_string()).unwrap();

        let (poly_eval, ok) = ccomp_run(&proj, &["-b", name, "shape"]);
        assert!(ok && poly_eval["ok"] == true, "{name}: {poly_eval}");
        let poly_area = poly_eval["area_mm2"].as_f64().unwrap();

        let (pub_res, ok) = ccomp_run(&proj, &["-b", name, "shape", "publish"]);
        assert!(ok && pub_res["ok"] == true, "{name}: {pub_res}");
        assert_eq!(pub_res["wrote"], true, "{name}");
        assert_eq!(pub_res["approximate"], false, "{name}");

        let out_dir = proj.join("interface").join(name);
        let rec = read(
            std::fs::read(out_dir.join("board-shape.dxf")).unwrap(),
            std::fs::read(out_dir.join("board-shape.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(hash(&rec), pub_res["geometry_hash"].as_str().unwrap(), "{name}");
        assert!(!rec.manifest.approximate, "{name}");

        let n_arcs = rec
            .outline
            .elements
            .iter()
            .filter(|e| matches!(e.kind, ElementKind::Arc { .. }))
            .count();
        let n_circles = rec
            .outline
            .elements
            .iter()
            .filter(|e| matches!(e.kind, ElementKind::Circle { .. }))
            .count();
        assert!(n_arcs >= min_arcs, "{name}: got {n_arcs} arcs, expected >= {min_arcs}");
        assert_eq!(n_circles, want_circles, "{name}");

        let curve_area: f64 = loop_area_mm2(&rec.outline)
            - rec.cutouts.iter().map(loop_area_mm2).sum::<f64>();
        assert!(
            (curve_area - poly_area).abs() < 0.5,
            "{name}: curve_area={curve_area} vs poly_area={poly_area}"
        );
    }

    // Verify minted IDs persisted in `rect_plus_circle`'s `shape.json`.
    let saved_doc: Value = serde_json::from_str(
        &std::fs::read_to_string(proj.join("boards/rect_plus_circle/shape.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(saved_doc["holes"][0]["id"], "composer:hole/1");
    assert_eq!(saved_doc["keepouts"][0]["id"], "composer:keepout/antenna");
    assert_eq!(saved_doc["thickness"], 2.0);
    assert_eq!(saved_doc["keepouts"][0]["max_height"], 2.5);

    // Generator board (`bolt-earring`) publishes `approximate: true`.
    let bolt_dir = proj.join("boards/bolt-earring");
    std::fs::create_dir_all(&bolt_dir).unwrap();
    std::fs::copy(
        root().join("boards/bolt-earring/shape.json"),
        bolt_dir.join("shape.json"),
    )
    .unwrap();
    let (bolt_pub, ok) = ccomp_run(&proj, &["-b", "bolt-earring", "shape", "publish"]);
    assert!(ok && bolt_pub["ok"] == true, "{bolt_pub}");
    assert_eq!(bolt_pub["approximate"], true);
    let bolt_rec = read(
        std::fs::read(proj.join("interface/bolt-earring/board-shape.dxf")).unwrap(),
        std::fs::read(proj.join("interface/bolt-earring/board-shape.json")).unwrap(),
    )
    .unwrap();
    assert!(bolt_rec.manifest.approximate);
}

#[test]
fn refusals_within_rule_and_status_diff() {
    let proj = fresh_tmp("shape-refusals");
    let golden_dir = find_golden_dir();

    // 1. Refusal: not the driver (`round_board` is driven by Waffle Iron).
    let waffle_dir = proj.join("boards/round_sensor");
    std::fs::create_dir_all(&waffle_dir).unwrap();
    let round_stem = golden_dir.join("round_board");
    let (_, ok) = ccomp_run(
        &proj,
        &[
            "-b",
            "round_sensor",
            "shape",
            "pull",
            "--interface",
            round_stem.to_str().unwrap(),
        ],
    );
    assert!(ok);
    let (refuse_driver, ok) = ccomp_run(&proj, &["-b", "round_sensor", "shape", "publish"]);
    assert!(!ok && refuse_driver["ok"] == false, "{refuse_driver}");
    assert!(
        refuse_driver["error"]
            .as_str()
            .unwrap()
            .contains("Waffle Iron"),
        "{refuse_driver}"
    );

    // Also test `status` when driver (`waffle`) updates the interface (moves hole h1 & changes thickness).
    let live_iface = proj.join("interface/round_sensor");
    std::fs::create_dir_all(&live_iface).unwrap();
    let mut updated = read(
        std::fs::read(golden_dir.join("round_board.dxf")).unwrap(),
        std::fs::read(golden_dir.join("round_board.json")).unwrap(),
    )
    .unwrap();
    let parent_rev = updated.manifest.revision.clone();
    let moved_hole_id = updated.holes[0].id.clone().unwrap();
    updated.holes[0].center = Point::new(updated.holes[0].center.x + 1_500_000, updated.holes[0].center.y);
    updated.manifest.thickness_mm = 2.0;
    updated.manifest.parent = Some(parent_rev);
    updated.manifest.author = Author {
        app: Driver::Waffle,
        source: "fedcba9876543210".into(),
        at: "2026-11-03T21:00:00Z".into(),
    };
    let new_hash = hash(&updated);
    updated.manifest.geometry_hash = new_hash.clone();
    updated.manifest.revision = new_hash;
    let (u_dxf, u_json) = write(&updated);
    std::fs::write(live_iface.join("board-shape.dxf"), u_dxf).unwrap();
    std::fs::write(live_iface.join("board-shape.json"), u_json).unwrap();

    let (st, ok) = ccomp_run(
        &proj,
        &[
            "-b",
            "round_sensor",
            "shape",
            "status",
            "--interface",
            live_iface.to_str().unwrap(),
        ],
    );
    assert!(ok && st["ok"] == true, "{st}");
    assert_eq!(st["in_sync"], false);
    assert_eq!(st["classification"], "updated_by_driver");
    assert_eq!(st["thickness_changed"], json!([1_600_000, 2_000_000]));
    let moved_h1 = st["changes"].as_array().unwrap().iter().any(|c| {
        c["id"] == moved_hole_id
            && c["kind"]
                == serde_json::to_value(ChangeKind::Moved {
                    dx_nm: 1_500_000,
                    dy_nm: 0,
                })
                .unwrap()
    });
    assert!(moved_h1, "expected Moved on {moved_hole_id}: {st}");

    // 2. Publish-nothing (`within` <= 1 nm) + Refusal: base moved.
    let own_dir = proj.join("boards/own");
    std::fs::create_dir_all(&own_dir).unwrap();
    let shape_path = own_dir.join("shape.json");
    std::fs::write(
        &shape_path,
        json!({
            "features": [{ "name": "box", "op": "add", "geom": { "type": "rect", "x": 0.0, "y": 0.0, "w": 20.0, "h": 15.0, "r": 2.0 } }],
            "holes": [{ "x": 5.0, "y": 0.0, "d": 3.0 }]
        })
        .to_string(),
    )
    .unwrap();
    let (p1, ok) = ccomp_run(&proj, &["-b", "own", "shape", "publish"]);
    assert!(ok && p1["wrote"] == true, "{p1}");
    let dxf_path = proj.join("interface/own/board-shape.dxf");
    let json_path = proj.join("interface/own/board-shape.json");
    let bytes_before = std::fs::read(&json_path).unwrap();

    // Nudge hole by 1 nm (0.000001 mm) -> within 1 nm tolerance, writes nothing!
    let mut own_doc: Value = serde_json::from_str(&std::fs::read_to_string(&shape_path).unwrap()).unwrap();
    own_doc["holes"][0]["x"] = json!(5.000_001);
    std::fs::write(&shape_path, own_doc.to_string()).unwrap();
    let (p_noop, ok) = ccomp_run(&proj, &["-b", "own", "shape", "publish"]);
    assert!(ok && p_noop["ok"] == true, "{p_noop}");
    assert_eq!(p_noop["wrote"], false);
    assert_eq!(p_noop["unchanged"], true);
    assert_eq!(std::fs::read(&json_path).unwrap(), bytes_before);

    // Advance on-disk revision behind Composer's back -> `publish` refuses because base moved.
    let mut moved_disk = read(std::fs::read(&dxf_path).unwrap(), &bytes_before).unwrap();
    moved_disk.holes[0].center.x += 500_000;
    moved_disk.manifest.parent = Some(moved_disk.manifest.revision.clone());
    let mh = hash(&moved_disk);
    moved_disk.manifest.geometry_hash = mh.clone();
    moved_disk.manifest.revision = mh;
    let (m_dxf, m_json) = write(&moved_disk);
    std::fs::write(&dxf_path, m_dxf).unwrap();
    std::fs::write(&json_path, m_json).unwrap();

    let (refuse_base, ok) = ccomp_run(&proj, &["-b", "own", "shape", "publish"]);
    assert!(!ok && refuse_base["ok"] == false, "{refuse_base}");
    assert!(
        refuse_base["error"].as_str().unwrap().contains("base moved"),
        "{refuse_base}"
    );

    // 3. Refusal: multi-piece outline.
    let multi_dir = proj.join("boards/multi");
    std::fs::create_dir_all(&multi_dir).unwrap();
    std::fs::write(
        multi_dir.join("shape.json"),
        json!({
            "features": [
                { "name": "left", "op": "add", "geom": { "type": "rect", "x": -20.0, "y": 0.0, "w": 10.0, "h": 10.0 } },
                { "name": "right", "op": "add", "geom": { "type": "rect", "x": 20.0, "y": 0.0, "w": 10.0, "h": 10.0 } }
            ]
        })
        .to_string(),
    )
    .unwrap();
    let (refuse_multi, ok) = ccomp_run(&proj, &["-b", "multi", "shape", "publish"]);
    assert!(!ok && refuse_multi["ok"] == false, "{refuse_multi}");
    assert!(
        refuse_multi["error"]
            .as_str()
            .unwrap()
            .contains("2 separate pieces"),
        "{refuse_multi}"
    );
}

#[test]
fn audit_regressions_defects_1_to_4_end_to_end() {
    let proj = fresh_tmp("shape-audit-regressions");

    // --- Defect 1: slot + corner_radius = 1, and polygon + 45-deg rect + corner_radius = 0.5 ---
    let slot_dir = proj.join("boards/slot_cr1");
    std::fs::create_dir_all(&slot_dir).unwrap();
    let slot_shape = slot_dir.join("shape.json");
    let slot_raw = json!({
        "features": [{
            "name": "f0",
            "op": "add",
            "geom": { "type": "slot", "x1": 28.0, "y1": 17.0, "x2": 19.0, "y2": 48.0, "w": 5.0 }
        }],
        "corner_radius": 1.0
    });
    std::fs::write(&slot_shape, slot_raw.to_string()).unwrap();

    // Also verify `ccomp shape` does NOT touch `shape.json` on disk.
    let before_shape_bytes = std::fs::read(&slot_shape).unwrap();
    let (poly_eval, ok) = ccomp_run(&proj, &["-b", "slot_cr1", "shape"]);
    assert!(ok && poly_eval["ok"] == true, "{poly_eval}");
    assert_eq!(
        std::fs::read(&slot_shape).unwrap(),
        before_shape_bytes,
        "`ccomp shape` must leave shape.json untouched"
    );
    let poly_area = poly_eval["area_mm2"].as_f64().unwrap();

    let (slot_pub, ok) = ccomp_run(&proj, &["-b", "slot_cr1", "shape", "publish"]);
    assert!(ok && slot_pub["ok"] == true, "{slot_pub}");
    assert_eq!(slot_pub["approximate"], false);
    let slot_rec = read(
        std::fs::read(proj.join("interface/slot_cr1/board-shape.dxf")).unwrap(),
        std::fs::read(proj.join("interface/slot_cr1/board-shape.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(slot_rec.outline.elements.len(), 4);
    assert!((loop_area_mm2(&slot_rec.outline) - poly_area).abs() < 0.15);

    // --- Defect 2: retired hole & keep-out IDs are never reused across edits & restarts ---
    let id_dir = proj.join("boards/id_no_reuse");
    std::fs::create_dir_all(&id_dir).unwrap();
    let id_shape = id_dir.join("shape.json");
    std::fs::write(
        &id_shape,
        json!({
            "features": [{
                "name": "plate",
                "op": "add",
                "geom": { "type": "rect", "x": 0.0, "y": 0.0, "w": 40.0, "h": 30.0, "r": 2.0 }
            }],
            "holes": [
                { "x": -12.0, "y": -8.0, "d": 3.0 },
                { "x": 12.0, "y": -8.0, "d": 3.0 }
            ],
            "keepouts": [{
                "name": "rf_zone",
                "side": "front",
                "geom": { "type": "rect", "x": -10.0, "y": 8.0, "w": 5.0, "h": 4.0 }
            }]
        })
        .to_string(),
    )
    .unwrap();
    let if_v1 = proj.join("if_id_v1");
    let (p_v1, ok) = ccomp_run(
        &proj,
        &[
            "-b",
            "id_no_reuse",
            "shape",
            "publish",
            "--interface",
            if_v1.to_str().unwrap(),
        ],
    );
    assert!(ok && p_v1["wrote"] == true, "{p_v1}");

    // Delete `composer:hole/1` and `composer:keepout/rf_zone`, add a new hole and new `rf_zone`
    // keep-out elsewhere, and publish in a new `ccomp` process.
    let mut edited_doc: Value =
        serde_json::from_str(&std::fs::read_to_string(&id_shape).unwrap()).unwrap();
    let holes_arr = edited_doc["holes"].as_array_mut().unwrap();
    assert_eq!(holes_arr[0]["id"], "composer:hole/1");
    assert_eq!(holes_arr[1]["id"], "composer:hole/2");
    holes_arr.remove(0);
    holes_arr.push(json!({ "x": 0.0, "y": 9.0, "d": 3.0 }));
    let ko_arr = edited_doc["keepouts"].as_array_mut().unwrap();
    assert_eq!(ko_arr[0]["id"], "composer:keepout/rf_zone");
    ko_arr.clear();
    ko_arr.push(json!({
        "name": "rf_zone",
        "side": "back",
        "geom": { "type": "rect", "x": 10.0, "y": 8.0, "w": 5.0, "h": 4.0 }
    }));
    std::fs::write(&id_shape, edited_doc.to_string()).unwrap();

    let (p_v2, ok) = ccomp_run(
        &proj,
        &[
            "-b",
            "id_no_reuse",
            "shape",
            "publish",
            "--interface",
            if_v1.to_str().unwrap(),
        ],
    );
    assert!(ok && p_v2["wrote"] == true, "{p_v2}");
    let after_doc: Value =
        serde_json::from_str(&std::fs::read_to_string(&id_shape).unwrap()).unwrap();
    assert_eq!(after_doc["holes"][0]["id"], "composer:hole/2");
    assert_eq!(
        after_doc["holes"][1]["id"],
        "composer:hole/3",
        "retired composer:hole/1 must never be minted again"
    );
    assert_eq!(
        after_doc["keepouts"][0]["id"],
        "composer:keepout/1",
        "retired composer:keepout/rf_zone must never be minted again"
    );

    // --- Defects 3 & 4: two-folder handover publish back to Composer's own folder + followed checks ---
    let hb_dir = proj.join("boards/handback");
    std::fs::create_dir_all(&hb_dir).unwrap();
    std::fs::write(
        hb_dir.join("shape.json"),
        json!({
            "features": [{
                "name": "body",
                "op": "add",
                "geom": { "type": "rect", "x": 0.0, "y": 0.0, "w": 30.0, "h": 20.0, "r": 2.0 }
            }]
        })
        .to_string(),
    )
    .unwrap();

    let comp_if = proj.join("composer_if");
    let waffle_if = proj.join("waffle_if");
    std::fs::create_dir_all(&waffle_if).unwrap();

    // 1. Composer publishes revision A to `composer_if`.
    let (pub_a, ok) = ccomp_run(
        &proj,
        &[
            "-b",
            "handback",
            "shape",
            "publish",
            "--interface",
            comp_if.to_str().unwrap(),
            "--at",
            "2026-11-04T02:00:00Z",
        ],
    );
    assert!(ok && pub_a["wrote"] == true, "{pub_a}");
    let rec_a = read(
        std::fs::read(comp_if.join("board-shape.dxf")).unwrap(),
        std::fs::read(comp_if.join("board-shape.json")).unwrap(),
    )
    .unwrap();

    // 2. Waffle takes over from A and publishes W1 in `waffle_if` (with parent = A).
    let mut rec_w1 = rec_a.clone();
    rec_w1.manifest.parent = Some(rec_a.manifest.revision.clone());
    rec_w1.manifest.driver = Driver::Waffle;
    rec_w1.manifest.author = Author {
        app: Driver::Waffle,
        source: "waffle-1".into(),
        at: "2026-11-04T02:05:00Z".into(),
    };
    rec_w1.manifest.last_handover = Some(Handover {
        by: Driver::Waffle,
        at: "2026-11-04T02:05:00Z".into(),
        from_revision: rec_a.manifest.revision.clone(),
    });
    rec_w1.holes.push(board_interface::Hole {
        id: Some("waffle:hole/1".into()),
        center: Point::new(0, 0),
        diameter_nm: 3_200_000,
        plated: false,
        clearance_nm: 0,
    });
    let gh_w1 = hash(&rec_w1);
    rec_w1.manifest.geometry_hash = gh_w1.clone();
    rec_w1.manifest.revision = gh_w1;
    let (w1_dxf, w1_json) = write(&rec_w1);
    std::fs::write(waffle_if.join("board-shape.dxf"), w1_dxf).unwrap();
    std::fs::write(waffle_if.join("board-shape.json"), w1_json).unwrap();

    // 3. Composer pulls W1 from `waffle_if` and takes over.
    let (_, ok) = ccomp_run(
        &proj,
        &[
            "-b",
            "handback",
            "shape",
            "pull",
            "--interface",
            waffle_if.to_str().unwrap(),
        ],
    );
    assert!(ok);
    let (_, ok) = ccomp_run(&proj, &["-b", "handback", "shape", "take-over"]);
    assert!(ok);

    // 4. Defect 4 check: advance `waffle_if` to W2 behind Composer's back -> publishing to
    //    `composer_if` MUST fail because the followed file moved!
    let mut rec_w2 = rec_w1.clone();
    rec_w2.holes[0].center.x = 3_000_000;
    rec_w2.manifest.parent = Some(rec_w1.manifest.revision.clone());
    rec_w2.manifest.author.at = "2026-11-04T02:06:00Z".into();
    let gh_w2 = hash(&rec_w2);
    rec_w2.manifest.geometry_hash = gh_w2.clone();
    rec_w2.manifest.revision = gh_w2;
    let (w2_dxf, w2_json) = write(&rec_w2);
    std::fs::write(waffle_if.join("board-shape.dxf"), &w2_dxf).unwrap();
    std::fs::write(waffle_if.join("board-shape.json"), &w2_json).unwrap();

    let (refuse_fol, ok) = ccomp_run(
        &proj,
        &[
            "-b",
            "handback",
            "shape",
            "publish",
            "--interface",
            comp_if.to_str().unwrap(),
        ],
    );
    assert!(!ok && refuse_fol["ok"] == false, "{refuse_fol}");
    assert!(
        refuse_fol["error"].as_str().unwrap().contains("base moved")
            && refuse_fol["error"]
                .as_str()
                .unwrap()
                .contains("followed interface"),
        "{refuse_fol}"
    );

    // 5. Pull W2, take over, and publish back to `composer_if` (which STILL holds A!) -> Defect 3:
    //    succeeds, sets parent = W2, and classifies as CleanHandover against W2!
    let (_, ok) = ccomp_run(
        &proj,
        &[
            "-b",
            "handback",
            "shape",
            "pull",
            "--interface",
            waffle_if.to_str().unwrap(),
        ],
    );
    assert!(ok);
    let (_, ok) = ccomp_run(&proj, &["-b", "handback", "shape", "take-over"]);
    assert!(ok);

    // Also verify refusing an unrelated folder `unrel_if` with actionable guidance:
    let unrel_if = proj.join("unrel_if");
    std::fs::create_dir_all(&unrel_if).unwrap();
    let mut unrel_rec = rec_a.clone();
    unrel_rec.holes.push(board_interface::Hole {
        id: Some("kicad:hole/99".into()),
        center: Point::new(5_000_000, 5_000_000),
        diameter_nm: 2_000_000,
        plated: false,
        clearance_nm: 0,
    });
    let uh = hash(&unrel_rec);
    unrel_rec.manifest.geometry_hash = uh.clone();
    unrel_rec.manifest.revision = uh;
    let (u_dxf, u_json) = write(&unrel_rec);
    std::fs::write(unrel_if.join("board-shape.dxf"), u_dxf).unwrap();
    std::fs::write(unrel_if.join("board-shape.json"), u_json).unwrap();

    let (refuse_unrel, ok) = ccomp_run(
        &proj,
        &[
            "-b",
            "handback",
            "shape",
            "publish",
            "--interface",
            unrel_if.to_str().unwrap(),
        ],
    );
    assert!(!ok && refuse_unrel["ok"] == false, "{refuse_unrel}");
    assert!(
        refuse_unrel["error"].as_str().unwrap().contains("base moved")
            && refuse_unrel["error"]
                .as_str()
                .unwrap()
                .contains("ccomp shape pull"),
        "{refuse_unrel}"
    );

    // Publish C1 to `composer_if` (still holding A):
    let (pub_c1, ok) = ccomp_run(
        &proj,
        &[
            "-b",
            "handback",
            "shape",
            "publish",
            "--interface",
            comp_if.to_str().unwrap(),
            "--at",
            "2026-11-04T02:10:00Z",
        ],
    );
    assert!(ok && pub_c1["wrote"] == true, "{pub_c1}");
    assert_eq!(pub_c1["parent"], rec_w2.manifest.revision);
    let rec_c1 = read(
        std::fs::read(comp_if.join("board-shape.dxf")).unwrap(),
        std::fs::read(comp_if.join("board-shape.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(compare(&rec_w2, &rec_c1).classification, Classification::CleanHandover);

    // 6. Defect 4 unreadable-followed warning: move `waffle_if/board-shape.json` aside so
    //    `sync.following` cannot be read, nudge hole in `shape.json`, and publish C2 to
    //    `composer_if` -> succeeds AND warns explicitly that `following` could not be read!
    std::fs::rename(
        waffle_if.join("board-shape.json"),
        waffle_if.join("board-shape.json.moved_aside"),
    )
    .unwrap();
    let mut hb_doc: Value =
        serde_json::from_str(&std::fs::read_to_string(hb_dir.join("shape.json")).unwrap()).unwrap();
    hb_doc["holes"][0]["x"] = json!(4.5);
    std::fs::write(hb_dir.join("shape.json"), hb_doc.to_string()).unwrap();

    let (pub_c2, ok) = ccomp_run(
        &proj,
        &[
            "-b",
            "handback",
            "shape",
            "publish",
            "--interface",
            comp_if.to_str().unwrap(),
            "--at",
            "2026-11-04T02:12:00Z",
        ],
    );
    assert!(ok && pub_c2["wrote"] == true, "{pub_c2}");
    assert_eq!(pub_c2["parent"], rec_c1.manifest.revision);
    assert!(
        pub_c2["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w.as_str().unwrap().contains("could not be read")),
        "expected warning for unreadable followed interface: {pub_c2}"
    );
}
