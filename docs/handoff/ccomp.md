# ccomp → boardroom crates/boardroom (`boardroom compose …`)

`ccomp` is Circuit Composer's command-line harness: one clap binary whose every command reads
files, writes files only when given `-o`, and prints one JSON object on stdout. Exit status: 0
ok, 1 the input failed a check (`ok: false` in the JSON), 2 the command could not run
(`{"ok": false, "error": …}`). Size: 3,031 lines of src (`main.rs` 1,838, `package_check.rs`
686, `floorplan_cmd.rs` 328, `project.rs` 179) and 1,408 lines of integration tests in
`crates/ccomp/tests/` (7 files, 22 tests) plus 3 unit tests. Nothing in the workspace depends on
it as a crate; it is run as a process by `tools/*.py` (`fp_bench`, `handcheck`,
`verify_routed`, `prepare_route` docs), `app/examples.sh`, and Boardroom's server
(`boardroom/app/server/src/snapshot.rs:136` and `problem.rs` shell out to `ccomp floorplan
--nets-from-board --outline-from-board --problem`). It depends on (Cargo.toml) `intent`, `pcb`,
`recognise`, `verify`, `blockgen`, `compile`, `library`, `interfaces`, `pinsolve`, `compose`,
`floorplan`, `shape`, `board-interface` (git tag v0.2.2), `serde`, `serde_json`, `clap` 4.

## Model in five minutes

- `crates/ccomp/src/main.rs:20` `Cli`: two global flags, `-C/--project` and `-b/--board`, and
  the `Cmd` enum (`main.rs:36`). `main` (`main.rs:515`) prints the result and maps it to the exit
  code; `run` (`main.rs:566`) is one `match` over every command.
- `Ctx` (`main.rs:537`): the project/board, resolved lazily. `Ctx::active()` (`main.rs:545`) is
  true when `--project` or `--board` is given **or the current directory has a `compose/`
  folder**. `or_board(given, what, pick)` (`main.rs:557`) returns the explicit path, else the
  board's file, else an error naming the flag. Only some commands use it (marked "board" in the
  table).
- `project.rs`: a project is a directory whose boards live in `compose/<board>/` (easy_pcb and
  Boardroom projects) if that exists, else `boards/<board>/` (this repo). A board dir may hold
  `board.json` (`project.rs:15`, `deny_unknown_fields`, every field optional, paths relative to
  the board dir) overriding the default layout: `compose.json`, `loose.json`,
  `out/composed.json`, `intent.json`, `floorplan.json`, `placement.json`, `shape.json`,
  `out/routing.json`, optional `dru`, and the KiCad board at
  `<project>/hw/<name>/a/<name>.kicad_pcb` (`project.rs:159`). Packages default to
  `$CCOMP_PACKAGES`, else `<project>/packages`, else this repo's `packages/` found through
  `CARGO_MANIFEST_DIR` at compile time (`project.rs:170-179`). `list` (`project.rs:103`) counts a
  subdirectory as a board when it holds `board.json` or `compose.json`.
- `package_check.rs`: `ccomp check package`, see below.
- `floorplan_cmd.rs`: turns a KiCad board + composition JSON + floorplan spec into a
  `floorplan::Problem`, see below.
- JSON in/out formats are those of the crates called (composition = `compose::Composition`,
  floorplan spec = `floorplan_cmd::Spec` at `floorplan_cmd.rs:14`, binding = `package_check::Binding`
  at `package_check.rs:21`, measure spec = `MeasureSpec` at `main.rs:1332`).

### Every subcommand

"board" = takes missing inputs from `--project/--board`. "logic" = the arm holds real logic
beyond parsing args and calling the function; that logic must move into a library crate so the
`boardroom` binary stays a thin wrapper.

| Command | What it does | Calls (file:line) | Notes |
|---|---|---|---|
| `boards` | list a project's boards, or one board's resolved files and which exist | `project::list` ccomp/src/project.rs:103, `project::resolve` :117, `Board::present` :67 | board |
| `schema [-o]` | print/write the intent JSON Schema | `intent::json_schema` intent/src/lib.rs:22 | |
| `classes` | list net class presets and block types | `intent::presets::NET_CLASSES` intent/src/presets.rs:74, `BLOCK_TYPES` | builds JSON inline |
| `netlist PATH` | read a `.net` (or export a `.kicad_sch`/`.kicad_pro` through `kicad-cli`) as JSON | `intent::netlist::Netlist::load` intent/src/netlist.rs:67 | |
| `validate [INTENT] [--netlist] [-o]` | validate intent against its netlist (default: `board.kicad_project` inside the intent, resolved beside it) | `intent::load` intent/src/lib.rs:16, `intent::validate::validate` intent/src/validate.rs:161 | board |
| `resolve [INTENT]` | nets with class soft defaults filled in | `intent::presets::resolve` intent/src/presets.rs:96 | board |
| `measure BOARD SPEC [-o]` | pad-to-pad copper paths, unreferenced track, decoupling per power pin | `pcb::Board::load` pcb/src/lib.rs:93, `verify::nets::{path :25, return_path :63, decoupling :134}` verify/src/nets.rs | logic (small): `MeasureSpec` loop, main.rs:637 |
| `check regions [PCB] [--routing] [-o]` | copper breaking a `routing.json` region rule | `compile::check_regions` compile/src/lib.rs:673 | board; `compile` goes away (spec) → `check`/`examine` |
| `check buck BOARD (--intent --block \| --roles) [--limit] [--svg] [-o]` | buck hot/output loop, SW area, FB clearance, SW underlap | `verify::buck::check` verify/src/buck.rs:136, `verify::svg::render` verify/src/svg.rs:47 | logic: `block_roles` main.rs:1403 reads limits from the intent block's `placement_goal` |
| `check half-bridge BOARD --roles [--block] [--limit] [--svg] [-o]` | commutation loop, phase path, gate/bootstrap loops | `verify::bridge::check` verify/src/bridge.rs:200, `render_bridge` svg.rs:69 | via `block_check` main.rs:1362 |
| `check sense …` | current-sense loop, Kelvin taps | `verify::bridge::sense` bridge.rs:379, `render_sense` svg.rs:87 | |
| `check protection …` | TVS path, vias before it, clamp loop | `verify::protect::protection` verify/src/protect.rs:98, `render_protection` svg.rs:101 | |
| `check ideal-diode …` | gate path, sense Kelvin, charge-pump loop | `verify::protect::ideal_diode` protect.rs:245, `render_ideal_diode` svg.rs:121 | |
| `check package BOARD --package DIR --binding FILE [-o]` | a board against a package's blocks by role | `package_check::check` ccomp/src/package_check.rs:211 | logic: the whole module (below) |
| `check supplies [COMPOSITION] [--packages] [-o]` | each rail's budget and regulator judged | `pinsolve::load` pinsolve/src/lib.rs:178, `pinsolve::supply::run` pinsolve/src/supply.rs:375 | board |
| `gen buck BOARD (--intent --block \| --roles) [--params] [--defaults] [-o]` | generate CIN/L/COUT layout in the IC's frame | `blockgen::buck::generate` blockgen/src/buck.rs:442 | |
| `compile [INTENT] [--pcb] -o ROUTING [--dru] [--dru-base]` | intent → `routing.json` + merged `.kicad_dru` | `compile::compile` compile/src/lib.rs:155, `compile::merge_dru` :480 | board; does not survive the move (spec: `compile` split up, `routing.json` gone) |
| `import easy-pcb NETS_CSV [--netlist] -o` | draft intent from easy_pcb `design/nets.csv` | `intent::import::from_easy_pcb_nets_csv` intent/src/import.rs:23 | needs global `--board` for the name only |
| `recognise board INPUT [-o] [--compare TRUTH]` | draft intent from a `.kicad_pcb` (pads) or netlist | `recognise::netlist_from_board` recognise/src/lib.rs:25, `recognise::recognise` :205, `recognise::compare` :766 | |
| `recognise corpus DIR --out OUT [--file] [--limit]` | every `DIR/<id>/<file>`; per-board intent+report and `summary.json` | `recognise_corpus` main.rs:1090 → `recognise::recognise` | logic: corpus totals and confidence histogram live in main.rs (→ `recognise` or `bench/`) |
| `compose [COMPOSITION] [--packages] [-o] [--bindings DIR] [--intent OUT]` | flat circuit + per-instance bindings; optionally the board's intent | `compose::circuits` compose/src/lib.rs:469, `compose::compose_with` :204, `compose::board_intent::board_intent` compose/src/board_intent.rs:126 | board |
| `floorplan [PCB] [--composed] [--spec] [--root] [--seed] [--moves] [--placer sa\|analytic] [--fill] [--evaluate PCB] [--problem] [--nets-from-board] [--outline-from-board] [--input-problem] [--intent] [-o]` | build a `floorplan::Problem` and solve, score or print it | `floorplan_cmd::problem` ccomp/src/floorplan_cmd.rs:109, `floorplan_cmd::limit_nets` :311, `floorplan::solve` floorplan/src/lib.rs:973, `floorplan::evaluate` :1374 | board; logic: `composed_from_board` main.rs:1280, `shape_into_spec` main.rs:1298, the Edge.Cuts / courtyard-box outline main.rs:867-890, `solve_and_report`'s `ok` rule main.rs:1264 |
| `shape [SHAPE] [--generators] [--import DRAWING] [-o]` | evaluate `shape.json` to polygons; list generators; import SVG/DXF | `shape::evaluate` shape/src/lib.rs:254, `shape::generators::GENERATORS` shape/src/generators.rs:30, `shape::import::{svg :14, dxf :402}` | board; `ok` = no warnings and one outline piece (main.rs:950) |
| `shape publish [--shape] [--interface] [--source] [--at]` | write `interface/<board>/board-shape.{dxf,json}` through board-interface | `shape::prepare_publish_with_followed` shape/src/sync.rs:646 | board; logic: interface path resolution main.rs:1704-1781, `git rev-parse` subprocess :1790, hand-rolled RFC 3339 clock :1804 |
| `shape status [--shape] [--interface]` | compare with the followed interface | `shape::status` sync.rs:877 | board |
| `shape pull [--shape] [--interface]` | take a followed interface as locked geometry | `shape::pull` sync.rs:372 | board; records the canonicalised absolute path in `sync.following` |
| `shape take-over` (alias `takeover`) | unlock pulled outline, Composer drives | `shape::take_over` sync.rs:450 | board |
| `pins [LOOSE] [--packages] [-o] [--nodes]` | solve loose connections into pins; supply rule; recompose | `interfaces::load_types` interfaces/src/lib.rs:210, `pinsolve::solve` pinsolve/src/lib.rs:244, `pinsolve::supply::{candidates :59, run :375}`, `compose::compose` compose/src/lib.rs:479 | board; logic: main.rs:989-1018 runs the supply rule twice and merges rails (→ `pinsolve`); finds `library/interfaces.json` as `packages/../library` |
| `interfaces PACKAGE [--variant] [--interface] [--root]` | a package's interfaces and the pins each signal can take | `interfaces::load_types`, `load_ports` interfaces/src/lib.rs:214, `Ports::options` | logic: `interfaces_cmd` main.rs:1206 shapes the report |
| `recipe [NAME] [--param K=V] [--root]` | list recipes, or expand one | `compose::load_library` compose/src/lib.rs:452, `compose::recipe::expand` compose/src/recipe.rs:308 | logic: `recipe_cmd` main.rs:1160 parses params |
| `library check [--root]` | claims db, interfaces data and recipes all consistent | `library::Library::load` library/src/lib.rs:118, `::check` :165, `interfaces::check_all` interfaces/src/lib.rs:324, `recipes_check` main.rs:1179 → `interfaces::power::check` interfaces/src/power.rs:178 | logic: `recipes_check` and the warning filter (main.rs:965-969) belong in `library`/`compose` |
| `library query [--set] [--id] [--scope k=v] [--attribute] [--status] [--text] [--root]` | claims matching filters | `library::Library::query` library/src/lib.rs:251 | |

### The modules with real logic

- **`package_check.rs` (686 lines).** Loads a package dir (`intent.json`, `schematic/nets.json`
  with `frame_variant` and per-net `anchor`/`pins`/`length`, `rules.json`, `ports.json`) and a
  binding (`{"package": "name@ver", "variant", "chip", "parts": {ROLE: REF}}`). Steps
  (`check`, :211): version and variant must match; nets resolved first through pins whose
  orientation is fixed, which decides which 2-pin R/C and 4-pin crystals (shield pads 2/4 on one
  net) are fitted rotated 180° (`swapped_pin` :141); then every net; chip pins are translated
  from the frame variant to the board's by pin *name* (`chip_pin` :190, lowest number wins for
  shared names). Findings: `wiring` (signal pins only; POWER/POWER_HI/GROUND compare only their
  anchor because real boards merge rails), `constraint` (each block `placement_goal` metric:
  via_count, unreferenced_length_mm through `verify::nets::return_path`, part_distance_mm as
  nearest pad centres, decoupling_path_mm, buck metrics through `verify::buck::check` cached per
  block), `length` (pin pair in `nets.json` `length`, via `verify::nets::path`), `value`
  (`rules.json` claims' `checks`, `parse_value` :600, `when_fitted` conditions). `ok` = no hard
  `fail`. **Where it belongs:** it is a judge of a board against cited package rules, built on
  `verify`, so it goes into `examine` (which `verify` becomes), with `Binding`/package loading
  perhaps shared with `compose` (which writes the same binding format: `compose::Composed.bindings`).
- **`floorplan_cmd.rs` (328 lines).** `problem` (:109) reads each composed part's footprint from
  the `.kicad_pcb` in the footprint's own frame (rotation undone, bottom-side mirrored in y),
  bbox = courtyard grown to pads past it by 0.07 mm, else pads + 0.25 mm, else ±0.5 mm;
  plated-through pads (F.Cu and B.Cu) become `through` boxes grown 0.2 mm; block = spec override
  else `instance/block` from the composition; `near` and `near_pin` from the composition; a part
  is fixed when the spec `keep`s it **or KiCad has it locked**; keep-outs become pad-less fixed
  parts `keepout:<name>`; nets with <2 or >`max_net_pins` (default 12, `max_pins` :93) pins or in
  `skip_nets` are dropped; rigid layouts (`ccomp gen` output wrapped in `layout`, or bare from
  `extract_layout.py`/`lift_layout.py`) become groups, with their F/B tracks and vias as
  pad-less member parts `copper:<name>:trackN`/`viaN`; regions resolved to part indices (block
  prefix match, rigid groups pulled in whole, `except` only on member-less `never`). `limit_nets`
  (:311) sets `max_mm = 0.75 × max_length` (`LIMIT_BOX`) and marks `planar` the nets a block's
  `unreferenced_length_mm` constraint names. **Where it belongs:** in `floorplan` (or a
  `floorplan::from_board` module) once `board` is there; it is the only producer of `Problem`
  besides the app, and Boardroom's server needs it as a function, not a subprocess.
- **`project.rs` (179 lines).** Board-file resolution described above. **Where it belongs:**
  Boardroom's `project` crate (the project model out of the server), as the Compose mode's view
  of `compose/<board>/`; the `boards/` fallback, the `CARGO_MANIFEST_DIR` packages fallback and
  `board.json` overrides are Composer-repo conveniences and can go (`packages/` will be
  Boardroom's own top-level dir).
- **In `main.rs`:** `composed_from_board` (:1280, a composition from a board's pad nets,
  skipping `unconnected-*`), the outline-from-board fallback (:867), `shape_into_spec` (:1298,
  holes/cut-outs/keep-outs as bbox keep-out rectangles), `block_roles` (:1403), the `pins`
  supply double pass (:989), `recipes_check` (:1179), `recognise_corpus` (:1090),
  `interfaces_cmd` (:1206), the shape-interface path helpers (:1704-1823).

## Solid

All 25 tests pass (`cargo test -p ccomp`, run 2026-11-12 through `reserve`).

- `tests/compose.rs`: `easy2350_recomposed_is_its_netlist` (65 nets, pin for pin, and the
  composer's binding equals the hand-written one), `the_minimal_board_composes` (55 nets).
- `tests/package_check.rs`: `easy2350_against_rp2350` (net mapping, R_XOUT reversed, the one
  hard fail is USB_DP/USB_DM over +3V3, core regulator passes, crystal value unverifiable),
  `via_in_pad_is_joined` (an ultraroute-routed board).
- `tests/project.rs`: `this_repositorys_boards_resolve_in_place`,
  `project_mode_runs_what_explicit_paths_run` (compose, validate, floorplan give identical JSON
  either way), `an_easy_pcb_project_keeps_its_boards_under_compose`.
- `tests/library.rs`: `library_is_whole`, `a_dispute_is_seen_from_both_sides`.
- `tests/shape.rs`: `the_bolt_earrings_shape_is_its_outline` (area 1118.07 mm²),
  `a_drawing_imports_and_a_shape_reaches_the_floorplanner` (SVG import; shape outline and a
  hole keep-out reach `floorplan --problem`).
- `tests/shape_interface.rs` (5 tests): pull/status/take-over/publish against
  board-interface's golden fixtures; covered in `shape.md`.
- `tests/floorplan.rs` (6 tests): covered in `floorplan.md`.
- Unit: `cli_is_consistent` (clap `debug_assert`, main.rs:1834),
  `rotated_four_pin_crystal_swaps_electrodes_not_oscillator` and `values` (package_check.rs:633, :667).

### Files the tests read

| Test file | Reads (relative to the repo root) |
|---|---|
| compose.rs | `boards/easy2350/compose.json`, `boards/easy2350/input/easy2350.net`, `boards/easy2350/rp2350.binding.json`, `packages/rp2350/**` (via the composition), `packages/rp2350/reference/rp2350a_minimal/compose.json` |
| package_check.rs | `boards/easy2350/input/easy2350.kicad_pcb`, `boards/easy2350/routed/regions-971e723/easy2350-ultraroute-971e723-regions-s1.kicad_pcb`, `boards/easy2350/rp2350.binding.json`, `packages/rp2350/{intent.json,schematic/nets.json,rules.json,ports.json}` |
| project.rs | the `boards/` listing must include bolt-earring, easy2350, flame-earring, rp2350-battery; every file in `boards/easy2350/board.json` must exist (`compose.json`, `floorplan/composed.json`, `intent.json`, `floorplan/spec.json`, `floorplan/placement.json`, `shape.json`, `floorplan/composed.kicad_pcb`, `compiled/routing.json`, `compiled/easy2350.kicad_dru`, `packages/`); `boards/easy2350/intent.json` (+ `input/easy2350.net` via `kicad_project`); `boards/rp2350-battery/{compose.json,composed.kicad_pcb,composed.json,floorplan.json,intent.json}` |
| shape.rs | `boards/bolt-earring/shape.json` (via its `board.json`), `boards/rp2350-battery/{compose.json,composed.json,floorplan.json,composed.kicad_pcb}`, `packages/` |
| library.rs | `library/**` (claims, `sources.json`, `interfaces.json`, `recipes/`, `stock.json`), every `packages/*/rules.json` and `ports.json` |
| floorplan.rs | `boards/easy2350/floorplan/{composed.kicad_pcb,composed.json,spec.json}` (+ the rigid layouts the spec names, under `packages/`), `boards/easy2350/input/easy2350.kicad_pcb`, `boards/rp2350-battery/{composed.kicad_pcb,composed.json,floorplan.json}` |
| shape_interface.rs | `../../../board-interface/golden/*.{dxf,json}` (a sibling checkout, or cargo's git checkout), `boards/rp2350-battery/*`, `boards/bolt-earring/shape.json`, `packages/` |
| (intent crate) `crates/intent/tests/easy2350.rs` | `boards/easy2350/intent.json`, `boards/easy2350/input/easy2350.net`, `schema/intent.schema.json` |

### Boardroom's `examples/easy2350` against what the tests read

Boardroom's example is the easy_pcb project itself (`design/`, `hw/easy2350/a/`, `route/`,
`scripts/`, `viewer/`), laid out as a project, not as a Composer board dir. Checked 2026-11-12:

| Composer path (`boards/easy2350/…`) | In `boardroom/examples/easy2350`? |
|---|---|
| `input/easy2350.kicad_pcb` | **different file** at `hw/easy2350/a/easy2350.kicad_pcb`: same 58 footprints at the same poses, values and footprint names, same net names, but re-routed later (Boardroom 8d9ae29, 56d2bd8: 581 segments vs 819, USB pair now on F.Cu) and **only 16 of 58 footprints `(locked yes)`** vs 58 of 58 |
| `input/easy2350.kicad_pro`, `input/easy2350.kicad_dru` | present at `hw/easy2350/a/` but different bytes (later revision) |
| `input/easy2350.net` | **missing**; `hw/easy2350/a/easy2350.kicad_sch` exists and `Netlist::load` can export it through `kicad-cli` (adds a kicad-cli dependency to tests) |
| `intent.json` | **missing** (only `design/intent.md` and `design/nets.csv`, from which `ccomp import easy-pcb` drafted the hand-annotated intent) |
| `compose.json` | **missing** |
| `rp2350.binding.json`, `rp2350.check.json`, `validate.json`, `shape.json`, `board.json` | **missing** |
| `floorplan/{composed.json,composed.kicad_pcb,spec.json,placement.json,placed.kicad_pcb}` | **missing** (no `floorplan/` or `compose/` at all) |
| `compiled/{routing.json,easy2350.kicad_dru,…}` | missing; not needed once `compile` goes |
| `routed/regions-971e723/easy2350-ultraroute-971e723-regions-s1.kicad_pcb` | **missing** (Boardroom's `route/easy2350/` holds a `report.json` only) |

So the tests' claim that easy2350 tests "point at examples/easy2350" holds only for the KiCad
inputs, and even those change behaviour: the floorplan test
`a_footprint_locked_in_the_board_stays_where_the_board_has_it` would fail (16 locked, not 58),
and `easy2350_against_rp2350`'s "the one hard failure is USB over +3V3 on B.Cu" will likely
change because the USB pair was re-routed onto F.Cu (unverified: not run against it). Proposal:
copy Composer's files into `examples/easy2350/compose/easy2350/` (`compose.json`, `intent.json`,
`rp2350.binding.json`, `shape.json`, `floorplan/*`) and keep Composer's pinned
`input/easy2350.{kicad_pcb,net,kicad_pro}` as a fixture under that folder (or re-pin the tests'
expectations to the new board deliberately). The routed `regions-971e723` board (one file) is
the only fixture for `via_in_pad_is_joined`.

## Known wrong or weak

- `docs/app-spec.md:390` promises `ccomp context` (the open board, selection, fields under it);
  it does not exist.
- `docs/app-spec.md:449` says every command accepts a project path; `netlist`, `measure`, every
  `check` except `regions`/`supplies`, `gen`, `recognise`, `interfaces`, `recipe`, `library`
  ignore `--project/--board` (the last three use `--root`, default `.`).
- `Ctx::active()` turns on project mode whenever the cwd has a `compose/` folder
  (main.rs:546): an unrelated `compose/` dir changes how a command resolves inputs.
- Project mode and explicit mode differ in `floorplan`: only project mode folds the board's
  `shape.json` into the spec (main.rs:861) and defaults `--intent` (main.rs:892). The test
  comparing the two passes because rp2350-battery has no `shape.json`.
- `pins` finds `library/interfaces.json` as `<packages>/../library/` (main.rs:985), a hidden
  layout assumption; `default_packages` falls back to the build tree's `packages/` through
  `CARGO_MANIFEST_DIR` (project.rs:178), so an installed binary silently points at a source path.
- `package_check` hardcodes the chip as package part `U1` (package_check.rs:130, :206, :294) and
  ground as package net `GND` (:297, :363); `trace_length_mm` and `clearance_mm` constraints are
  reported `unverifiable` ("not measured by this check yet", :403); swappable parts are
  recognised by reference prefix (`R`/`C`, `Y`/`X`), not by footprint or class (:143-151).
- `shape_into_spec` turns holes, cut-outs and polygon keep-outs into their bounding boxes
  (main.rs:1297), so a curved or diagonal keep-out over-reserves.
- `shape publish` shells out to `git rev-parse HEAD` and computes UTC by hand (main.rs:1790,
  :1804) — fine but untested and belongs in a library.
- f9f3cb9: a handover with no edits publishes a revision that is not its geometry hash,
  departing from the board-interface suite spec; noted as "to be settled in the crate" (later
  pins 8f962cc/42524ab/0103477 changed board-interface; whether this is now settled: see `shape.md`).
- `floorplan_cmd::problem` pad-past-courtyard growth is 0.07 mm, "half the copper clearance",
  a constant, not read from the board's rules (floorplan_cmd.rs:163).
- `compile` and `check regions` encode the `routing.json`/`--regions` contract, which the
  Boardroom spec retires.

## Not covered by tests

- Commands with no test through `ccomp`: `schema`, `classes`, `netlist`, `resolve`, `measure`,
  `check regions`/`buck`/`half-bridge`/`sense`/`protection`/`ideal-diode`/`supplies`, `gen buck`,
  `compile`, `import easy-pcb`, `recognise board`/`corpus`, `pins`, `interfaces`, `recipe`
  (the crates under them have their own tests; the arms' glue does not).
- `block_roles` limits taken from an intent block (main.rs:1403); `parse_limits`.
- `floorplan --evaluate` back-side handling (main.rs:920), `--placer analytic` and `--fill` via
  the CLI, `--intent` / `limit_nets` (no test asserts `max_mm`/`planar` on a problem).
- `composed_from_board`'s `unconnected-*` filter; the courtyard-box outline fallback for a board
  without Edge.Cuts (main.rs:871; used by fp_bench on Cypress boards).
- `package_check`: `chip_pin` variant translation (easy2350's variant is the frame variant),
  `when_fitted`, layer-scoped `not_applicable` (`layer_scope` :525), `part_distance_mm`.
- `shape publish` default `--source`/`--at` (tests pass both explicitly, unverified for all).
- Exit code 2 vs 1 semantics beyond `library check`.

## Lessons to keep

- **One JSON object on stdout, always** (main.rs:1): the agent and the tools never parse logs.
  Errors are JSON too. Keep this for `boardroom compose …`.
- **Outputs are never defaulted** (project.rs:5): a command writes only with `-o`, even in
  project mode. Inputs default from the board; outputs never do.
- **Locks live in the board** (5038c8b): a footprint KiCad has locked is fixed by the
  floorplanner exactly as a spec `keep`; Boardroom builds a project's problem this way, so a lock
  made in KiCad reaches Composer. easy2350's design parts (J1, J4, U1 + C14/C15/L1, SW1, SW2,
  D1, D3-D6, beside headers J2, J3) start locked; floorplanned with them: 763 mm wire vs the
  person's 878.
- **A placement promises a box, not a route** (72bc9d0): `max_length` becomes a pins'-box limit
  of 0.75 × the length, each mm over costing `w_limit` 10.
- **Rigid copper rides with its group** (975c0d1): a generated/lifted layout's tracks and vias are
  pad-less parts no courtyard may cover; on rp2350-battery R9 and C20 had landed on the core
  regulator's locked copper.
- **Power nets merge on real boards** (package_check.rs:254): VREG_VIN and IOVDD are one +3V3,
  so wiring checks compare only signal nets pin by pin.
- **Rotated passives are not errors** (bb5636d, 2bbb620): 2-pin R/C and four-pin crystals whose
  shield pads share a net may be fitted 180° turned; inductors are excluded
  (`rp2350.vreg.inductor_orientation`), and four-pin active oscillators are not symmetric.
- **A finding names its source** (claim id or provenance) and a strength; `unbound` and
  `unverifiable` are separate from `fail` — never guess a binding.
- **Copper joins like KiCad's connectivity** (975c0d1): tracks whose copper overlaps join, else
  `check package` called ultraroute's nets open; a via inside a pad joins the pad (bb5636d).
- Recogniser over the PCBench corpus (af5894b): 7,429 of 51,601 nets classified on 1,182 boards
  in 10 s; on easy2350 and rp2350-battery no wrong class against hand-written intent.
- Judge a floorplan change over several seeds, not one (the battery board's 10-16 package fails
  across 4 placements of one setup; 202869c's numbers are 4-placement averages).

## For the move

- **Commands**: rebuild as `boardroom compose <cmd>` for the Composer ones (`boards`, `compose`,
  `pins`, `floorplan`, `shape …`, `gen`, `recipe`, `interfaces`, `check supplies`,
  `check package`). The intent/library/judging ones are not Composer-specific: `schema`,
  `classes`, `validate`, `resolve`, `import`, `recognise` sit naturally under
  `boardroom intent …`/`recognise …`; `measure` and `check buck|half-bridge|sense|protection|ideal-diode|package`
  under `boardroom examine …` (verify becomes `examine`); `library check|query` under
  `boardroom library …`. Owner's call; the spec only fixes `compose` as a command.
- **Drop** `compile` and `check regions` (spec: `compile` split, `routing.json`/`--regions` go;
  `check_regions` → `check` or `examine`).
- **Move logic out of the binary first** (spec: "every action is a function first"):
  `package_check.rs` → `examine`; `floorplan_cmd.rs` + `composed_from_board` + outline fallback
  + `shape_into_spec` → `floorplan` (a `Problem::from_board(board, composed, spec, root)`), so
  Boardroom's server calls it instead of spawning `ccomp floorplan --problem`
  (`app/server/src/snapshot.rs:136`, `problem.rs`); `project.rs` → `project`; the `pins` supply
  double pass → `pinsolve`; `recipes_check` → `compose` or `library`; `recognise_corpus` →
  `recognise` or `bench/`; shape interface path resolution and `git`/clock → `shape` or
  `project`.
- **Renames**: `pcb::Board` becomes a module of `board` keeping its API (`Board::load`, `parts`,
  `pads`, `pad("REF.PIN")`, `outline`, `vias`, `layers`); `verify::*` becomes `examine::*`;
  `intent::netlist` moves to `kicad` (the netlist reader).
- **Paths**: tests use `boards/easy2350`, `boards/rp2350-battery`, `boards/bolt-earring`,
  `packages/rp2350`, `packages/*`, `library/`, `schema/intent.schema.json` and the sibling
  `../board-interface/golden`. In Boardroom: easy2350 → `examples/easy2350` once the missing
  files above are added; rp2350-battery → `examples/`; bolt-earring and flame-earring leave for
  their own repos, so `project.rs`'s board listing test and `shape.rs`'s bolt test need a small
  fixture cut from bolt-earring (spec says one is cut first). `default_packages` should use
  Boardroom's `packages/` by configuration, not `CARGO_MANIFEST_DIR`.
- **Project layout**: Boardroom projects keep Composer's files in `compose/<board>/` and KiCad in
  `hw/<board>/<variant>/`; `project.rs` already defaults `pcb` to `hw/<name>/a/<name>.kicad_pcb`,
  hard-coding variant `a` — take the variant from `boardroom.toml` instead.
- **Differently**: one board-resolution rule (drop the cwd-has-`compose/` magic and the
  `boards/` fallback); every command honours `--project/--board` or none does; project mode and
  explicit mode produce the same problem (fold `shape.json` in both, or neither); keep the
  JSON-out/exit-code contract and the clap `debug_assert` test.
