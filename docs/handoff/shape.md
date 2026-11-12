# shape → boardroom crates/shape

`shape` is the board-outline document. `shape.json` lists features (rectangle, circle, slot,
polygon, exact curve path, parametric generator), each added to or cut from the outline in order.
Corner rounding and symmetry are applied to the result, and the document also holds mounting
holes, keep-outs by side and the board thickness. It evaluates in two ways. First, to **polygons**
(Clipper2 on integers), which the floorplanner keeps parts inside. Second, to **exact
lines, arcs and circles in nanometres**, published as the shared `board-shape.{dxf,json}` through
the external `board-interface` crate (git tag v0.2.2, commit cf831a8; also used by Waffle Iron). It
also imports SVG and DXF drawings and handles pull, take over, publish and status against an
interface another tool drives. Size: 5,228 lines of src (curves.rs 2,742, sync.rs 938, import.rs
852, lib.rs 575, generators.rs 121), 1,247 lines of in-file unit tests, and 1,050 lines of
integration tests in `crates/ccomp/tests/{shape.rs,shape_interface.rs}`. Depends on
`board-interface`, `clipper2-rust` 1.2, `serde`, `serde_json` and `sha2`. Depended on by `ccomp`
(29 call sites in main.rs) and `app/core` (12: evaluate, generators, import, take_over and pull for
the browser). No other Composer crate uses it.

## Model in five minutes

- `ShapeDoc` (`src/lib.rs:54`) is `shape.json`, with fields `features: Vec<Feature>`,
  `corner_radius`, `symmetry`, `thickness` (default 1.6), `holes`, `keepouts`, ID counters
  `next_hole_id`/`next_keepout_id`/`minted_ids`, and `sync: Option<SyncState>`.
  - `Feature {name, op: Add|Subtract, geom: Geom, hidden}` (`lib.rs:105`).
  - `Geom` (`lib.rs:126`, serde tag `type`): `rect{x,y,w,h,r,angle}`, `circle{x,y,d}`,
    `slot{x1,y1,x2,y2,w}`, `polygon{points,approximate}`, `path{elements: Vec<board_interface::Element>, approximate}`
    (exact nm curves, from DXF import or take-over), `generator{name, params}`.
  - Units are mm and degrees, in KiCad's frame with y down. A rect's `angle` turns counter-clockwise on
    screen (`transform`, `lib.rs:514`).
- **`evaluate(&ShapeDoc) -> Shape`** (`lib.rs:254`) produces the polygon result: `outline`
  (largest piece first), `cutouts`, `area_mm2`, `bbox`, `holes`, `keepouts: Vec<KeepoutShape>`,
  per-feature polygons for drawing handles, and `warnings`. A bad feature becomes a warning, not an
  error. When `sync.synced` is set, it goes to `evaluate_synced` (`lib.rs:322`), which turns the
  pulled nm loops into polygons and never runs Clipper on them.
  - Coordinates are Clipper2 integers at `SCALE` = 10,000 per mm (0.1 µm). Circles and round
    joins are chorded to `ARC_TOLERANCE` = 0.005 mm.
  - Order of operations: features in order (union or difference, NonZero), then `symmetric`
    (unions the mirror or half-turn image, `lib.rs:474`), then `round` (inflate +r, −2r, +r, which
    rounds concave and convex corners alike, `lib.rs:486`). An outer ring has positive signed area
    in this y-down frame.
- **`curves::evaluate_curves(&ShapeDoc) -> CurveEval`** (`curves.rs:277`) produces the exact result
  for `board-interface`. It works in three steps:
  1. Synced geometry is returned verbatim.
  2. Fast path (`try_direct_path_recipe`, `curves.rs:464`): untouched `Path` loops are returned
     as they are, keeping their nm coordinates and IDs.
  3. Otherwise it runs the same Clipper booleans, then matches each output edge back to the
     analytic source primitives (circles, slot ends, rect corners, symmetry images, fillets from
     `corner_radius`) and snaps junctions to whole nm.

  A rebuilt loop is accepted only if it passes `validate_loop_against_path` (`curves.rs:2274`):
  Hausdorff distance ≤ 0.040 mm (0.007–0.018 after rounding) and a symmetric-difference area
  bound. Otherwise the loop falls back to chords and `approximate: true`. Generator outlines are
  always chords with `approximate: true`. Element IDs look like `composer:<feature-slug>/...`.
  Holes are `composer:hole/<n>` and keep-outs `composer:keepout/<n>`.
- **`sync`** (`src/sync.rs`) holds the `board-interface` side:
  - `SyncState` (`sync.rs:17`): driver (Composer, KiCad or Waffle), revision, base, following,
    followed_revision, last_handover, pending_handover_from, base_record, synced geometry, and
    `recipe_aside`.
  - `pull` (`sync.rs:372`) locks the document to the incoming geometry and sets Composer's
    recipe aside.
  - `take_over` (`sync.rs:450`) unlocks it, turns the synced loops into `Path` features with IDs
    kept, and schedules `last_handover`.
  - `prepare_publish` and `prepare_publish_with_followed` (`sync.rs:603`, `:646`) refuse in these
    cases:
    - Composer is not the driver;
    - the target on disk or the followed file moved from the base;
    - the thickness is invalid;
    - nothing is left of the outline;
    - the outline is in several pieces;
    - it fails `within(...,1)`;
    - it fails a readback through `board_interface::read`.

    Otherwise they return the DXF and manifest text, and the caller writes the files.
  - `status` (`sync.rs:877`) runs `board_interface::compare`.
  - `mint_ids` (`sync.rs:140`) mints IDs.
- **`import::svg`** (`import.rs:14`) and **`import::dxf`** (`import.rs:402`) turn a drawing into
  `Vec<Feature>`.
  - SVG: user units are taken as mm unless the width and height say `mm`, `in` or `px` (96 per
    inch); transforms are ignored; Bézier curves are flattened.
  - DXF: reads LWPOLYLINE and POLYLINE with bulges, CIRCLE, ARC, and LINE/ARC chains joined within
    1 µm. y is flipped to y down, and units are assumed to be mm.
- **`generators`** (`generators.rs`): `GENERATORS` (`generators.rs:30`) holds a single entry,
  `bolt`, the bolt earring's outline. Each `Param` has a min, a default, a max and a step, which the
  app shows as sliders.
- **Files.** The crate reads and writes `shape.json` only through callers. The one exception is
  `prepare_publish`, which reads the followed `board-shape.{dxf,json}` from the filesystem
  (`read_followed_from_fs`, `sync.rs:572`; replaced by an "unreadable" warning on wasm32). `ccomp`
  writes `<project>/interface/<board>/board-shape.{dxf,json}`.
- **Invariants a change must keep:**
  1. The polygon output of `evaluate` stays byte-identical on existing documents (the SHA-256 test
     below).
  2. Pulled geometry never enters Clipper2, so pull → take over → publish keeps the geometry
     hash.
  3. A minted or retired hole or keep-out ID is never issued again (`minted_ids` persists).
  4. Publish verifies its own output by reading it back before returning it.

## Solid

All 20 unit tests and the 7 ccomp shape tests pass (run 2026-11-12 under `reserve`).

- Polygon geometry (`src/lib.rs`):
  - `a_rounded_rectangle_has_the_area_it_should`
  - `subtracting_inside_makes_a_cutout_and_at_the_edge_a_notch`
  - `symmetry_adds_the_image`
  - `rounding_rounds_both_kinds_of_corner`
  - `a_bad_feature_is_a_warning_not_a_failure`
- Regression pin: `repo_shape_json_polygon_results_are_byte_identical_to_pre_change_capture` pins
  the output's JSON length, SHA-256, area and bbox bit patterns, and vertex counts for 4 files.
- True arcs and IDs:
  - `composer_authored_primitives_and_booleans_publish_true_arcs_and_round_trip`: rect, rounded
    rect, circle, slot, union, subtract, an L-shape with `corner_radius`, symmetry, and a
    rotated rect.
  - `generator_board_publishes_approximate_true`
  - `element_ids_survive_parameter_resize_and_hole_ids_never_remint`
- Regressions from an audit:
  - `regression_defect_1_corner_radius_tangent_junctions_safety_net_and_readback`
  - `regression_defect_2_retired_hole_and_keepout_ids_never_reused_across_restarts`
  - `regression_defect_3_and_4_handover_back_to_own_folder_and_followed_checks`
- Goldens: `all_three_valid_goldens_pull_take_over_and_republish_with_identical_hash` covers
  board-interface goldens `easy2350`, `rounded_rect` and `round_board`.
- Import (`src/import.rs`): `svg_shapes_in_mm`, `svg_pixels_are_scaled`,
  `svg_curves_are_flattened`, `dxf_polylines_circles_and_lines`,
  `dxf_arcs_and_r12_polyline_preserve_true_arcs`.
- Generators (`src/generators.rs`):
  - `the_bolt_is_the_bolt_earrings_outline`: within 0.05 mm of `geometry.py`, area within 1 mm².
  - `every_parameter_is_in_range_by_default`
- `crates/ccomp/tests/shape.rs`:
  - `the_bolt_earrings_shape_is_its_outline`
  - `a_drawing_imports_and_a_shape_reaches_the_floorplanner`: SVG import, then
    `floorplan --problem` takes the outline and a hole as a keep-out bbox.
- `crates/ccomp/tests/shape_interface.rs`:
  - `acceptance_test_1_easy2350_pull_floorplan_take_over_publish`
  - `all_three_valid_goldens_round_trip_through_ccomp`
  - `composer_authored_shapes_publish_true_arcs_and_generator_is_approximate`
  - `refusals_within_rule_and_status_diff`
  - `audit_regressions_defects_1_to_4_end_to_end`

## Known wrong or weak

- **The golden fixtures are found by guessing, and probably the wrong revision is read.**
  `find_golden_dir` (`lib.rs:1504`, copied in `ccomp/tests/shape_interface.rs:19`) tries three
  places in order:
  1. `$BOARD_INTERFACE_GOLDEN`;
  2. the **first** `~/.cargo/git/checkouts/board-interface-*/<rev>/golden` that `read_dir`
     returns, of any revision;
  3. `../../../board-interface/golden`.

  On this machine, the cache holds four revisions, and directory order puts `0734d0b` first.
  Its `easy2350.json`, `round_board.json` and `rounded_rect.json` differ from the pinned
  v0.2.2 (`cf831a8`); only the `.dxf` files are identical. So the golden tests very likely run
  against older manifests, not the pinned ones. The read order was checked with `ls -f`; which
  directory the test actually picked is unverified. Tests pass either way.
- **Keep-outs reach the floorplanner as bounding boxes,** and `max_height` is dropped.
  - Every keep-out, cut-out and hole becomes an axis-aligned keep-out rectangle (`ccomp`
    `shape_into_spec`, `main.rs:1298`).
  - So a rotated or round keep-out blocks more than it should.
  - A keep-out's `max_height` (`lib.rs:227`) is published to `board-interface` but not carried in
    `KeepoutShape`. The floorplanner therefore treats a height-limited keep-out as a full keep-out.
- **The shape-to-floorplan handoff is written twice:** in Rust (`ccomp main.rs:1298`) and in JS
  (`app/web/src/App.svelte:355` `applyShape`, "as ccomp does"). The JS version takes `outline[0]`
  without the single-piece check that ccomp does.
- **SVG arcs (`A`/`a`) become their chord,** a straight line, marked `approximate` (`import.rs:235`).
  SVG transforms are ignored. In DXF, `$INSUNITS` is ignored (mm assumed), and SPLINE, ELLIPSE
  and INSERT/blocks are silently skipped, as are open chains (`chain_dxf_segments`).
- **The two outlines differ slightly.** The floorplanner keeps parts inside the polygon (chords
  within 5 µm). What is published is the rebuilt curve, which may stray up to 40 µm from that
  polygon and still be accepted (`curves.rs:2341–2350`).
- **Keep-outs in a synced document are named by index** from `doc.keepouts` (`lib.rs:370`). If
  the orders differ, names attach to the wrong keep-outs.
- **The ancestry check is one generation deep.** `record_descends_from` (`sync.rs:560`) only
  accepts the immediate parent or `last_handover.from_revision`. A target two revisions ahead of
  the base is refused as "base moved".
- **`following` paths resolve two ways.** `prepare_publish` resolves `following` against the
  process cwd. `ccomp` instead resolves it relative to `shape.json`'s directory and calls
  `prepare_publish_with_followed`. Only the second is correct for files on disk.
- **Docs disagree with the code:**
  - `docs/app-spec.md` §3 lists an "offset" operation; there is none (`Op` is `Add`/`Subtract`;
    `inflate` is used only internally).
  - It describes "holes, with keep-outs per side **and layer**"; it is per side only.
  - It calls symmetry "a constraint which pattern generators read"; it is an operation that adds
    the mirror image, and holes and keep-outs are not mirrored.
  - Project-script generators (`compose/<board>/generators/`) do not exist.
  - The only built-in generator is one board's outline (`bolt`).
- **Edge connectors on a shaped outline** were listed as "still to do" in 3e8dc95. app-spec
  milestone C now says they sit on the shaped edge. Unverified here; that lives in `floorplan`.
- Publish error text names `ccomp shape pull` and `ccomp shape status` (`sync.rs`), which will be
  wrong after the rename.
- Commit f9f3cb9 records a departure from the suite spec: a no-edit handover's revision was not
  its geometry hash. 8f962cc says this was fixed in board-interface v0.2.0 (`seal()`), and the
  workaround was removed.

## Not covered by tests

- `curves.rs` and `sync.rs` (3,680 lines) have no unit tests of their own; they are reached only
  through the lib.rs and ccomp tests above.
- SVG `A` (arc) path commands, `ellipse`, `polyline`, and `Q`/`S`/`T` commands.
- DXF files with non-mm units and DXF entities that are skipped.
- `evaluate_synced` keep-out naming when the orders differ, and synced documents with cut-outs
  (as polygons).
- `prepare_publish` resolving `following` against the cwd. The wasm32 branch, where the followed
  file is unreadable.
- Generator parameters at their min and max (only defaults are tested). `hidden` features in
  `evaluate_curves`.
- `Shape.features` per-feature polygons. Warnings for a multi-piece outline and for an empty
  outline.
- `app/core`'s wasm wrappers for `pull`, `take_over` and import. Covered only by Playwright
  `app/web/tests/shape-lock.spec.js` (not run here).

## Lessons to keep

- **Keep two results, and never let the exact one change the polygon one.** f9f3cb9 added
  `evaluate_curves` beside `evaluate` instead of changing it. The SHA-256 pin proves polygon
  output did not move on any `shape.json`. Keep that split: the floorplanner wants polygons,
  and the interface wants true arcs.
- **Rebuild true arcs by matching Clipper's output to the analytic sources,** then check the result
  (Hausdorff distance and symmetric difference). When it fails, fall back to chords with
  `approximate: true` rather than fitting arcs. Generator chords must never be fit to noisy arcs
  (`generator_board_publishes_approximate_true`).
- **Pulled geometry is kept in nm and never goes through Clipper.** This is what keeps the
  geometry hash identical across pull → take over → publish for all three goldens.
- **IDs are minted once and never reused,** even across deletions and restarts (`minted_ids`
  plus high-water counters). Incoming IDs are recorded on pull so later mints can't collide
  (audit defect 2).
- **Publish refuses rather than overwrites** (base moved, followed moved, wrong driver, several
  pieces). It reads its own DXF and manifest back before handing them to the caller to write.
- **board-interface pins:**
  - v0.2.0: revisions come from `seal()`, so a zero-edit handover differs from its parent (8f962cc).
  - v0.2.1: compare classifies by the fields the revision covers, not `author.at` (42524ab).
  - v0.2.2: a taker that has published since sees the old driver behind (0103477).
- **Bolt generator vs `geometry.py`:** bounds within 0.05 mm, area 1118.066 mm². The difference
  comes from where each library puts arc chords (shapely 16 per quarter, Clipper by tolerance),
  which moves the rounded tip's lowest vertex where the tab is centred.
- **Rounding is inflate +r, −2r, +r** so both concave and convex corners round. Its
  `corner_radius` fillets are rebuilt as true arcs, and the tangent junctions were audit defect 1.

## For the move

- **Destination:** `crates/shape`, in the Composer group. `board-interface` stays a tagged git
  dependency (v0.2.2). Nothing in the crate depends on another Composer crate, so it can move
  first.
- **Goldens.** From `crates/shape` or `crates/ccomp`, `../../../board-interface/golden` resolves to
  a `board-interface` checkout **beside the repository** (here `~/Software/board-interface`). In
  Boardroom (`~/Software/boardroom/crates/shape`) it would still resolve there, but only by
  accident: nothing guarantees a sibling checkout, and the cargo-cache scan picks an arbitrary
  revision. Do one of the following:
  - copy the six pinned files (`{easy2350,rounded_rect,round_board}.{dxf,json}` from v0.2.2
    cf831a8) into `crates/shape/tests/golden/`, noting the tag;
  - or have board-interface expose its goldens.

  Then delete the cache scan and the sibling path. `handover.{dxf,json}` exists from v0.2.1 but
  no test reads it.
- **Files the shape tests read from this repo, and what Boardroom has:**

  | Read by | Path | In Boardroom `examples/easy2350`? |
  |---|---|---|
  | `lib.rs` byte-identical test | `boards/easy2350/shape.json` | **No.** examples/easy2350 is a Boardroom project (`hw/`, `design/`, `route/`, `scripts/`) with no `shape.json`, `board.json`, `input/`, `floorplan/` or `intent.json`. |
  | same | `app/web/public/examples/easy2350/shape.json` | No. This is an export of the above made by `app/examples.sh`: same document, different whitespace. |
  | same, `generator_board_publishes_approximate_true` | `boards/bolt-earring/shape.json` | No. bolt-earring becomes its own repo; the spec says to cut a small fixture. |
  | same | `app/web/public/examples/bolt-earring/shape.json` | No. An export of the above (identical JSON). |
  | ccomp `shape.rs::the_bolt_earrings_shape_is_its_outline` | `boards/bolt-earring/board.json`, `shape.json` (via `-b bolt-earring`) | No. |
  | ccomp `shape.rs`, `shape_interface.rs` (2 tests) | `boards/rp2350-battery/{compose.json,composed.json,floorplan.json,composed.kicad_pcb}`, `packages/` | Not easy2350. rp2350-battery goes to `examples/` per the spec; `packages/` comes in. |
  | `shape_interface.rs::composer_authored_…` | `boards/bolt-earring/shape.json` | No. |

  The bolt fixture needs only `shape.json` (one `generator` feature named `bolt`) and a
  `board.json` for the ccomp test. easy2350's `shape.json` is one rect (110.5, 125.5, 21.05 ×
  51.05); put it at `examples/easy2350/compose/shape.json` ("each mode writes only its own
  folder"). Then re-point the byte-identical test, dropping the two `app/web/public/examples`
  copies, which are build output.
- **Commands.** `ccomp shape [--generators | --import FILE | publish | status | pull | take-over]`
  becomes `boardroom compose shape …`. Fix the `ccomp shape pull/status` wording in `sync.rs`
  error strings. The interface output stays `<project>/interface/<board>/board-shape.{dxf,json}`,
  which matches the spec's "shared outline in `interface/`".
- **Move `shape_into_spec` out of ccomp** (`main.rs:1298`) into a library function, in `shape` or
  `floorplan`, that the CLI, server and `wasm` all call. Delete the JS copy in
  `App.svelte:applyShape`. While doing it, pass true polygons and `max_height`, not bboxes.
- **Move the followed-path resolution** from `ccomp`'s `resolve_followed_from_str` into the crate,
  relative to the document's directory. Drop the cwd-relative `prepare_publish`, or make it take a
  base directory.
- **What I would do differently:**
  - Give `curves.rs` its own focused unit tests.
  - Make `ancestor` walk the full parent chain.
  - Key synced keep-out names by ID, not index.
  - Treat SVG arcs as true arcs; `board_interface::Element` already has `Arc`.
  - Read DXF `$INSUNITS`.
