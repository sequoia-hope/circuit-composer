# composer-core (app/core) → boardroom crates/wasm

`app/core` (crate `composer-core`) is the Circuit Composer web app's Rust core, compiled to WASM
with wasm-bindgen. It holds one board's floorplan as the page edits it (`Session`: the floorplan
`Problem` plus every part's pose), runs the annealer with progress callbacks, builds the
`board-draw` scene and overlay meshes the page draws, reads a `.kicad_pcb`'s parts/nets/locks and
writes a placement back into the `.kicad_pcb` text byte-faithfully (no pcbnew), and exposes thin
JSON bindings to `shape`, `pinsolve`, `compose` and `compose::recipe`. Size: 1,327 lines of Rust
(`src/lib.rs` 1,140 of which ~280 are tests, `src/draw.rs` 187); no `tests/` directory. Nothing
in Rust depends on it (it is a workspace member, `Cargo.toml:3`); the Svelte app imports the
generated `app/web/src/core/composer_core.js`. It depends on `floorplan`, `shape`, `pinsolve`,
`compose`, `interfaces` (path deps), `board-draw` (git, rev b45cc3f), ultraroute's `kicad-pcb`
(git, rev 0e581c4, the same commit `crates/pcb` reads with), `serde`, `serde_json`, and under
feature `wasm` only: `wasm-bindgen = "=0.2.129"`, `js-sys`. Crate type `cdylib` + `rlib`, so the
same code runs natively under `cargo test`.

## Model in five minutes

Types (`app/core/src/lib.rs`):
- `KicadBoard` (:21) / `KicadBoard::read` (:37): from `.kicad_pcb` text via
  `kicad_pcb::parse_board`: `poses` (REF → `floorplan::Pose`, mm and degrees; KiCad's metres ×1e3),
  `locked` (footprints with KiCad's lock), `parts` (value, footprint lib id), `nets` (net name →
  `REF.pin`, skipping empty and `unconnected-…` nets).
- `Session` (:63): `pr: floorplan::Problem`, `poses: Vec<Pose>` (problem order), `index`
  (REF → i), `group_of` (part → rigid group, member offset and rotation in the anchor frame).
  Key methods: `new` (:87), `set_placement`/`placement` (:111/:119), `set_outline` (:129, sets
  `pr.outline_polygon`, clears `pr.outline`), `set_keepouts` (:137), `set_poses` (:165),
  `write_kicad_pcb` (:176), `take_locks` (:198), `placement_of` (:211), `locked`/`set_locked`
  (:215/:221), `move_to` (:227), `rotate` (:238), `parts`/`part` → `PartView` (:272/:276),
  `ratsnest` (:299, Prim MST per net), `evaluate` (:344, `floorplan::evaluate`), `set_placer`
  (:350, `sa`|`analytic`), `set_fill` (:357, clamped 0..1), `anneal` (:364,
  `floorplan::solve_with` on a clone of the problem with seed/moves overridden).
- `set_footprint_locks` (:713) and `unlock_moved_copper` (:750): text-level `.kicad_pcb` edits
  used by `write_kicad_pcb`.
- `draw.rs`: `part_id` (:13, problem index i ↔ board-draw part id i+1; 0 is the board),
  `bd_pose` (:23, pose convention conversion), `build` (:29, scene: pads as 0.22 mm discs at
  pin centres plus courtyard rectangle, in the part's frame; optional outline), `sync_poses` (:56),
  `OverlayOpts` (:65) / `overlay` (:87, block tint, dashed box, ratsnest, locks, selection),
  `grid` (:133), `floorplan_point` (:181, test helper).

Exported WASM API (`mod wasm`, `lib.rs:379`, feature `wasm`), and who calls it in `app/web/src`:

| Export | lib.rs | JS caller |
|---|---|---|
| `kicad_circuit(text)` | 411 | `lib/source.js` (board opened from a project store, no `circuit.json`) |
| `shape_evaluate(doc)` | 418 | `lib/ShapeView.svelte` |
| `pins_solve(loose, types, packages, library)` | 428 | `lib/compose.js` |
| `compose_board(composition, packages, library)` | 442 | `lib/compose.js` |
| `recipe_expand(recipe, params, stock)` | 464 | `lib/Connections.svelte` |
| `shape_generators()` | 476 | `lib/ShapeView.svelte` |
| `shape_import(text, "svg"\|"dxf")` | 482 | `lib/ShapeView.svelte` |
| `shape_take_over(doc)` | 494 | `lib/ShapeView.svelte` |
| `shape_pull(doc, dxf, manifest)` | 502 | **none** (exported, unused by the page) |
| class `Board` (391), `new(problem_json)` | 513 | `App.svelte:277`, `lib/anneal.worker.js:11` |
| `set_placement`, `placement`, `problem`, `parts` | 522/548/553/557 | `App.svelte` |
| `write_kicad_pcb`, `set_placement_from_kicad_pcb` | 534/539 | `App.svelte` (Save to project / open from store) |
| `index` | 561 | unverified (no `board.index(` call found; `store.js` hits are IndexedDB) |
| `poses`, `set_poses` | 566/594 | `App.svelte`, `anneal.worker.js` (flat `[x,y,rot]*n`, per frame) |
| `set_outline`, `set_keepouts` | 571/580 | `App.svelte` (Shape → "Use as board outline") |
| `ratsnest`, `metrics`, `set_locked`, `rotate` | 600/608/622/617 | `App.svelte` |
| `ratsnest_nets` | 604 | **none** |
| `move_to` | 612 | `App.svelte`, `lib/BoardView.svelte` |
| `set_placer`, `set_fill`, `anneal` | 627/632/638 | `lib/anneal.worker.js` |
| `mesh`, `overlay`, `grid`, `pick`, `set_side_hidden`, `labels` | 659/672/677/683/688/697 | `lib/BoardView.svelte` |

Data in/out: `Board::new` takes `ccomp floorplan --problem` output (`floorplan::Problem` JSON;
examples in `app/web/public/examples/<board>/problem.json`, written by `app/examples.sh`).
Placements are `{parts: {REF: {x, y, rotation}}}` (mm, degrees). Everything else is JSON strings
of the underlying crate's serde types, or flat `Vec<f32>`/`Vec<f64>` for per-frame data.
`mesh(px, mask)` returns board-draw's vertex buffer; the JS side is `Renderer` from the npm
package `board-draw` (`BoardView.svelte:7`).

Build: `app/build.sh` runs `wasm-pack build core --release --target web --out-dir
../web/src/core --out-name composer_core --no-pack -- --features wasm`, then `vite build` into
`app/dist/`. `wasm-opt = false` (`Cargo.toml` `[package.metadata.wasm-pack.profile.release]`)
because the machine's wasm-opt (116) cannot parse what current Rust emits; the `.wasm` is 3.5 MB
unoptimised. `wasm-bindgen` is pinned `=0.2.129`; the commit that added it (a27de99) gives no
reason; the installed CLI here is 0.2.129, and wasm-bindgen's library and CLI must match exactly,
so presumably that (unverified). CI (`.github/workflows/pages.yml`) installs wasm-pack 0.14.0;
the machine has 0.13.1. The generated files in `app/web/src/core/` are **not committed**: the
directory has a `.gitignore` of `*`, and `app/.gitignore` ignores `web/src/core/` and `dist/`;
Pages builds them in CI.

Invariants a change must keep:
- Units: floorplan mm and degrees; `kicad_pcb` metres (×1e3 in `read`, ×1e-3 in `write_kicad_pcb`).
- Pose convention (`draw.rs:19-26`): the floorplan turns counter-clockwise on screen (KiCad's
  sense, y down) and mirrors back-side parts in y; board-draw turns +x→+y and mirrors in x. So
  board-draw rotation = `-(r + 180)` for back parts, `-r` for front.
- A board read and written back with nothing moved or relocked is byte-identical.
- A rigid group moves and turns as one about its anchor; a group's lock is its anchor's `fixed`.
- References containing `:` (`shape:…`, `keepout:…`) are keep-outs: never written to the board,
  never drawn as locked. `set_keepouts` appends `shape:NAME` fixed padless parts after all others
  and replaces earlier ones by truncating at the first `shape:` part.
- Board-draw part id = problem index + 1; `pick` returns index or -1.

## Solid

`cargo test -p composer-core` (11 tests, all pass, run 2026-11-12 through `reserve`; the
`--features wasm` build also `cargo check`s for `wasm32-unknown-unknown`):
- `the_placement_is_written_into_the_board_as_pcbnew_writes_it`: all 58 footprints byte-identical
  to `tools/apply_placement.py`'s pcbnew output (`boards/easy2350/floorplan/placed.kicad_pcb`).
- `a_board_read_and_written_back_is_unchanged`: read → write is byte-identical; poses match.
- `a_parts_lock_is_the_boards`: locks round-trip; unlocking U1 unlocks its group (C14, C15, L1)
  and removes exactly 4 lines.
- `a_moved_parts_locked_copper_is_unlocked_and_no_other`: stub and via at a moved U1 unlocked,
  C1's copper kept locked.
- `the_saved_placement_is_legal_here_too`, `a_group_moves_and_turns_as_one`,
  `a_locked_part_stays_and_a_warm_anneal_keeps_the_board_legal` (warm anneal never worse in
  wire_mm), `the_analytic_start_places_the_battery_board_legally`,
  `shape_keepouts_are_fixed_parts_that_replace_each_other`, `an_anneal_can_be_stopped`.
- `board_draw_poses_are_the_floorplans`: every pad of rp2350-battery, bolt-earring and easy2350,
  all quarter turns, both sides (bolt-earring has back parts), lands where the floorplan says.
- Through the UI only: Playwright `app/web/tests/placement.spec.js` (run and Stop a placement),
  `shape-lock.spec.js` (take over a locked outline).

## Known wrong or weak

- **KiCad 7 locks are read but not written.** `kicad_pcb`'s reader accepts both `(locked yes)` and
  KiCad 7's bare `locked` token (`kicad-pcb/src/copper.rs:848`), but `set_footprint_locks`
  (`lib.rs:713`) only recognises `\t\t(locked yes)` as the footprint's second line, and
  `unlock_moved_copper` only removes `(locked yes)` (`continue`s otherwise). On a KiCad 7 file a
  locked footprint would get a second lock line, an unlock would not take, and a moved part's
  locked copper stays locked silently. It is also line/tab-format dependent (`\t(footprint `,
  `\t)`), i.e. KiCad 8+ formatting only.
- **Side is not read from the board.** `KicadBoard` records x, y, rotation but not side;
  `placement_of` ignores it and `write_kicad_pcb` writes the problem's `back`. A part flipped in
  KiCad is flipped back on save.
- Pads are drawn as 0.22 mm discs at pin centres (`draw.rs:17`): the floorplan has no pad shapes.
- `ratsnest` treats a pin missing from the part's geometry as (0, 0) in part frame
  (`unwrap_or((0.0, 0.0))`, `lib.rs:309`) instead of failing.
- The anneal is one synchronous WASM call; the `on` callback's stop (`return false`) is never used
  by the page: the worker always returns true and Stop terminates the worker
  (`anneal.worker.js:1-3`), keeping the last snapshot.
- `mesh` clones the whole vertex buffer every call (`lib.rs:667`).
- Dead exports: `shape_pull`, `ratsnest_nets` (and probably `Board.index`) have no JS caller.
- Doc drift: the module comment (`lib.rs:7-8`) still says drawing goes through "a placeholder until
  `board-draw` lands" (it landed in 37642cc). `docs/app-spec.md:292` says app/core depends on
  `compose`, `intent`, `floorplan`, `verify` and `compile`; it actually depends on `floorplan`,
  `shape`, `pinsolve`, `compose`, `interfaces`, `board-draw`, `kicad-pcb`. The spec's "polygon
  operations" in core live in `shape` (Clipper2).
- Clippy (`--features wasm`): one warning in this crate, `type_complexity` at `lib.rs:68`
  (`group_of`).
- No TODO/FIXME/XXX in the crate.

## Not covered by tests

- Every `#[wasm_bindgen]` function's glue: JSON (de)serialisation, error mapping, `Board::new`'s
  outline derivation from `outline_polygon` or the `outline` rect (`lib.rs:515`), `library()`
  defaults when `recipes`/`stock` are absent, `recipe_expand`'s hand-built JSON shape,
  `shape_import`'s kind dispatch, `shape_take_over`, `shape_pull`.
- `kicad_circuit` / `KicadBoard::read`'s `parts` and `nets` beyond one assertion each
  (`/+3V3` contains U1.1, C1 is 100n); the `unconnected-` filter and pin dedup.
- `draw::overlay` (block tint, dashed box, ratsnest, locks, selection), `draw::grid`, `hue`;
  `Board::mesh`'s rebuild vs `redraw` caching, `pick`, `labels`, `set_side_hidden`.
- `Session::rotate` on a single (ungrouped) part; `set_fill` clamping; `set_outline` effect on
  anneal; `refix` (a locked part moved by hand stays locked at the new place).
- `write_kicad_pcb` with a problem part the board lacks (documented as an error).
- `unlock_moved_copper`: arcs, the "token and preceding space" removal branch (non-own-line lock),
  rotation-only or side-only moves.

## Lessons to keep

- **Write the board in Rust, not pcbnew** (2a7abb1): `kicad_pcb::place` reproduces pcbnew's save
  byte for byte on easy2350's 58 footprints, so it runs in the browser. Keep byte-identity as the
  test oracle, and "nothing changed ⇒ same bytes" so saves don't churn diffs.
- **Locks live in the board** (5038c8b): the app reads each footprint's KiCad lock (a group
  follows its anchor) and writes back only changed ones, so a lock made in KiCad reaches Composer
  and vice versa.
- **Moving a part must unlock its locked copper** (e0ab110): copper stays put when a part moves, so
  its fan-out shorted the pads landing on it, and Route's bare-board start (`--strip`) keeps locked
  copper. On easy2350 with U1 moved 2 mm, 84 locked items were unlocked and a `--strip` route left
  no power net open (before: +1V1, +3V3 and GND shorted by U1's old fan-out). Unlock = flood over
  same-net locked items joined end to end (or within a via's radius), starting from items touching
  a moved pad; tolerance 1 nm (file precision) and 1e-6 deg.
- **Scene built once, moves are poses** (37642cc): pads and courtyard go in once per part in its own
  frame; moving only changes the pose, so redraws rewrite only moved parts' vertices. Overlay and
  grid are separate meshes rebuilt per frame. Picking is board-draw's: front before back, smallest
  first. The pose-convention conversion (above) was verified by a test over all quarter turns and
  both sides; keep that test when the renderer changes.
- Per-frame data crosses as flat `Float64Array` (`poses`, `set_poses`, snapshots), not JSON.
- Anneal runs in a Web Worker on its own `Board` built from `problem()` + `poses()`; it cannot hear a
  stop message mid-call, so stop is worker termination (a27de99).
- Solve and compose run in the browser: the bolt earring's `pins_solve` + compose takes about
  40 ms (22c209f). `pins_solve` uses a search budget of 2,000,000 (`lib.rs:435`).
- `fill` (5f7c04f): with an outline, nothing in the cost used the free room and parts packed into a
  corner; `fill` (default 0.5) spreads them. Exposed here as `set_fill`.
- Rendering decided 2026-10-29 (docs/app-spec.md decision 5): WebGL2 through `board-draw`, shared
  with UltraRoute Studio; no WebGPU front end and no wgpu (Composer draws only 2D and runs nothing
  on the GPU; WebGL2 works in headless Chrome/Playwright, Firefox on Linux and phones).
- `wasm-opt` off: the installed wasm-opt 116 cannot parse current rustc output.

## For the move

- Destination: Boardroom's one `crates/wasm` ("render, the live floorplanner"). board-draw stops
  being a separate repo: its Rust half becomes `scene` + `render`, its JS `renderer.js` becomes
  `render`'s JS package. `draw.rs` must be rewritten onto `scene`/`render`; it uses
  `board_draw::{Scene, Item, Geom::{Pad, Area, Outline}, Prim::Circle, AreaKind::Courtyard,
  Pose{bottom}, Mesh{fill, ring, dashed_ring, segment, into_vec}, Rgba}` and
  `Scene::{set_layers, insert_part, insert, set_pose, draw, redraw, part_at, labels,
  style.hide_front/hide_back}`, `Drawing.vertices`. Composer pins board-draw at
  b45cc3f (Cargo and `app/web/package.json`); Boardroom's web app pins d37b1f8
  (`boardroom/app/web/package.json:25`) together with ultraroute's studio core via `draw.sh`. Use
  whatever `render` becomes; keep `board_draw_poses_are_the_floorplans` as the check that the pose
  convention survived. Per the spec, Composer's floorplan drawing could become a stage crate
  emitting `scene` data (floorplan → scene) so it doesn't depend on `render`.
- One WASM crate must now also carry Studio's/`render`'s board view; watch module size (3.5 MB
  today without wasm-opt; try a current wasm-opt / binaryen in the toolchain pin 1.97.1).
- `kicad-pcb` (rev 0e581c4) becomes Boardroom's `kicad` crate. Move `set_footprint_locks`,
  `unlock_moved_copper` and the body of `Session::write_kicad_pcb` into `kicad` (spec: "applying a
  block, a placement or a route is one call" in Rust), and fix KiCad 7 bare `locked` there.
  `KicadBoard::read` belongs on `board` (the merged `pcb` module already has parts, pads with nets);
  add side.
- `Session` (editable floorplan document: groups, locks, keep-outs, ratsnest) is not WASM-specific.
  Move it into `floorplan` (e.g. `floorplan::session`) so `server`, `boardroom compose floorplan`
  and the wasm crate share it, and leave `crates/wasm` as bindings only ("every action is a function
  first").
- `pinsolve`/`compose`/`shape` bindings stay thin wrappers; drop `shape_pull` and `ratsnest_nets`
  unless Compose mode needs them.
- Test fixtures to bring or re-point: `app/web/public/examples/{rp2350-battery,easy2350,
  bolt-earring}/{problem,placement}.json` (written by `app/examples.sh`, which runs
  `ccomp floorplan --problem` → becomes `boardroom compose floorplan --problem`), and
  `boards/easy2350/floorplan/{composed,placed}.kicad_pcb`. Boardroom's `examples/easy2350` has
  `hw/easy2350/a/easy2350.kicad_pcb` and `route/` but **no** `floorplan/composed.kicad_pcb` or
  `placed.kicad_pcb`, and no app example JSON; the four `.kicad_pcb` tests need those two files
  copied in (or regenerated) and the 58-footprint count kept. bolt-earring leaves for its own repo;
  `board_draw_poses_are_the_floorplans` needs some fixture with back-side parts (easy2350's problem has J2,
  J3 on the back per the test comment, so it can stand in; adjust the `bolt-earring` assertion).
  Tests locate files with `CARGO_MANIFEST_DIR/../web/public/examples` and `../..`; these paths
  change with the new layout.
- Build: `app/build.sh`'s wasm-pack line moves to the one web build (`web/`, output into the web
  app's source tree, gitignored). Align wasm-pack versions (CI 0.14.0 vs local 0.13.1) and keep
  `wasm-bindgen` pinned to the installed CLI. GitHub Pages deploy of the standalone app goes away
  with the repo (spec step 7).
- Differently: make the anneal step-wise (`step(n)` returning a snapshot) instead of one sync call
  with a JS callback, so the worker can be messaged to stop and the page could even run it on the
  main thread in slices; and serve the board view from `render`'s `view(board)` rather than a
  floorplan-only scene, so Compose's Board tab shows real pad shapes.
