# floorplan → boardroom crates/floorplan

The floorplanner: given every part's courtyard box and pad positions, the nets between pads and a
set of rules (edge connectors, rigid groups, separations, regions, an outline or none), it finds
each part's position and quarter turn by simulated annealing (optionally started from a
gradient-descent "analytic" placement), legalises the result, and returns a placement plus the
board outline it needs. Size: 3,106 lines in `src/` (lib.rs 1,689, inc.rs 421, legal.rs 394,
analytic.rs 392, region.rs 210), of which about 320 are `#[cfg(test)]` unit tests; its integration
tests are 138 lines in `crates/ccomp/tests/floorplan.rs` (there is no `crates/floorplan/tests/`).
Depends only on `serde` and `serde_json`. Depended on by `ccomp` (`src/floorplan_cmd.rs` builds the
`Problem` from a board + composition + spec; `src/main.rs` `Cmd::Floorplan`) and by `app/core`
(`composer-core`, the WASM crate: the live anneal in the browser). Pure computation, no file I/O
of its own.

## Model in five minutes

- **Input: `Problem`** (lib.rs:274): `parts: Vec<PartGeom>`, `nets: Vec<Net>`, `groups:
  Vec<Rigid>`, `separations`, `options: Options`, `outline: Option<[x0,y0,x1,y1]>`,
  `outline_polygon: Option<Vec<P>>`, `regions: Vec<Region>`. It is serde JSON; `ccomp floorplan
  --problem` prints it and `--input-problem` reads it back. The app's offline examples are these
  files (`app/web/public/examples/<board>/problem.json`, written by `app/examples.sh`).
- **`PartGeom`** (lib.rs:36): courtyard **bounding box** at rotation 0 in footprint frame, pad
  centres by pin, optional `block`, `near: (part index, pin)` + `near_pin` (a decoupler facing its
  pin), `edge: Edge {side, facing}`, `fixed: Pose`, `back` (bottom side; mirrored in y then turned,
  as KiCad flips, lib.rs:367), `through` (through-hole pad boxes that block the other side too).
- **Pseudo-parts**: keep-outs (`keepout:<name>`) and a rigid layout's tracks/vias
  (`copper:<group>:trackN` / `viaN`) are added by `ccomp`'s `floorplan_cmd.rs:195,240,246` as
  pad-less parts. Any reference containing `:` is not a board part: metrics skip it for density
  (lib.rs:869), `Placement.back` omits it (lib.rs:1199), `app/core` filters it. `Placement.parts`
  still contains them. Keep this convention or replace it with a typed field.
- **`Net`** (lib.rs:85): pins as (part index, pin name), `weight`, `max_mm` (limit on the pins'
  half-perimeter box), `planar` (count crossings). Power/ground and nets over `max_net_pins` (12)
  are dropped by the caller, not here.
- **`Rigid`** (lib.rs:204): an anchor part index and members at (offset, rotation) in its frame;
  moves and turns as one movable. **`Region`** (region.rs:34): polygon, `Must`/`Never`, member
  indices, `Sides`; a `Never` region with no members binds every non-fixed part on its side
  (region.rs:49).
- **`Options`** (lib.rs:221, defaults lib.rs:267): seed 1, 400k moves, clearance 0, margin 0.5,
  weights w_wire 1, w_area 0.05, w_near 3, w_block 0.3, w_edge 5, w_sep 5, w_overlap 20, fill 0.5,
  w_fill 3, w_limit 10, w_corridor 5, w_cross 0, placer `sa` | `analytic`.
- **Entry points**: `solve(&Problem) -> Placement` (lib.rs:973); `solve_with(pr, start, every,
  on)` (lib.rs:991): progress snapshots every `every` moves, `on` returning false stops, `start`
  = warm start (heat 0.03, reach 0.2, kept if nothing better); `evaluate(pr, poses) -> Metrics`
  (lib.rs:1374) scores a given placement (a person's) — rigid groups are not enforced. `rot`
  (lib.rs:361) is public: KiCad's convention, positive CCW on screen, y down.
- **Output: `Placement`** (lib.rs:702): poses by reference (rounded to 0.001), outline, per-block
  boxes, `back` list, `Metrics` (lib.rs:667: wire_mm, overlaps + pairs, near_mm, corridor_mm,
  crossings, density, gap_mm, halo_mm, fill_mm2, edge/separation/region misses), cost.
- **Cost** (`State::cost`, lib.rs:764): weighted sum of net half-perimeters (+ `w_limit` per mm over
  `max_mm`), courtyard overlap (+ outline overhang + region misses + flush-edge margin misses, all
  at the overlap weight), area of the parts' box, near distance, corridor (near line through other
  courtyards), planar crossings, block spread, edge misses, separation shortfall (weight scaled
  with the overlap weight), fill (overlap of halo-grown courtyards). The overlap weight rises
  `w_overlap·(1+200·prog²)` (lib.rs:1368).
- **Anneal loop** (lib.rs:1090): moves are edge-snap (shaped outline, 25 %), slide-along-side (edge
  part with a named side on a rect outline), quarter turn (12 %), swap (8 %), Gaussian shift.
  Temperature from 200 random probes, geometric cooling to 1e-4. Incremental cost in `inc.rs`
  (O(n) per move); `State::cost` re-syncs it every 2000 moves.
- **After the anneal**: `seat_edges` (edge parts onto the shaped edge), `legalise` (lib.rs:1405,
  push pairs apart along the shallow axis, ≤500 pushes), `legal::negotiate` (legal.rs:132,
  PathFinder-style eviction with history cost; never onto a fixed part, off board, astray of a
  region, or breaking a separation). Placements rank by **number of rules missed, then cost**
  (`State::score`, lib.rs:758).
- **Analytic placer** (analytic.rs:31, `global`): Adam on weighted-average wirelength + smooth
  overlap/fill/outline/region/near/block/edge/separation, 1200 iterations, quarter turns tried
  against `Inc` from 30 % on; then fed to `solve_with` as a warm start.
- **Regions** (region.rs, added in 22e9298): miss = area of the courtyard on the wrong side (exact,
  Sutherland–Hodgman clip, region.rs:96) + half-perimeter × signed distances of 9 sample points.
  Parts bound to a `must` region start inside it (`region_homes`, lib.rs:1230).

Invariants a change must keep:
- `inc.rs` must equal `State::cost` (test `the_cached_cost_is_the_full_cost`). Any new term goes
  in both, or the anneal silently optimises something else.
- RNG draws are part of the result: the start draws a spread pose even for fixed parts "so a seed
  places a board without regions as it always has" (lib.rs:1013). The ccomp tests pass on fixed
  seeds (3, 2, 1), so changing the draw order can flip them.
- `ok` (in ccomp, main.rs:1267) = 0 overlaps and no edge, separation or region misses. `max_mm`,
  corridor and crossings are soft and never make a placement not ok.

## Solid

All pass (run 2026-11-12, `--release`, booked with reserve): 18 unit tests, 6 ccomp tests.

- lib.rs `tests`: `a_chain_packs_without_overlap`, `rigid_groups_keep_their_shape_and_edges_face_out`,
  `a_near_pins_line_is_kept_clear`, `planar_nets_count_their_crossings`,
  `a_limited_net_pays_for_its_excess`, `parts_keep_the_margin_from_a_flush_edge`,
  `a_polygon_outline_holds_the_parts`, `fill_spreads_the_parts_over_the_board`,
  `the_analytical_placer_alone_is_legal`, `the_analytical_placer_keeps_groups_and_edges`,
  `a_crowded_back_is_legalised` (flame-earring problem.json), `edge_connectors_sit_on_a_shaped_outline`,
  `parts_keep_their_regions`, `separated_blocks_stay_apart`.
- lib.rs `inc_tests::the_cached_cost_is_the_full_cost`: 3,000 random moves (kept and undone) on
  rp2350-battery, easy2350 and bolt-earring problem.json with limits, planar nets, a concave
  `must` and a `never` region and near pins added; cached = full within 1e-6 relative.
- region.rs: `a_must_miss_is_zero_inside_and_grows_with_distance`,
  `a_never_region_catches_a_part_straddling_a_thin_strip`, `concave_regions_are_measured_exactly`.
- `crates/ccomp/tests/floorplan.rs` (runs the `ccomp` binary, cwd = repo root):
  - `easy2350_beats_its_own_placement`: legal at seed 3 / 200k, less wire than `--evaluate` of the
    person's board, J1 rotation 180, U1–L1 rigid offset kept. Reads
    `boards/easy2350/floorplan/{composed.kicad_pcb,composed.json,spec.json}`,
    `boards/easy2350/input/easy2350.kicad_pcb`, and via the spec's rigid layout
    `packages/rp2350/layouts/qfn60-4layer/layout.json`.
  - `the_battery_board_keeps_its_rules`: seed 2 / 200k, ok, 0 overlaps, no separation misses, area
    < 2000 mm². Reads `boards/rp2350-battery/{composed.kicad_pcb,composed.json,floorplan.json}` and
    the spec's layouts `packages/rp2350/layouts/qfn60-4layer/layout.json`,
    `packages/tlv62569/layouts/sot23-5-4layer/layout.json`.
  - `a_board_by_itself_is_a_problem_and_a_saved_problem_solves_the_same`: reads
    `boards/easy2350/input/easy2350.kicad_pcb` (≥56 parts, ≥40 nets, Edge.Cuts width 21.05 mm);
    saved problem and in-place problem give identical placements.
  - `a_footprint_locked_in_the_board_stays_where_the_board_has_it`: same board; asserts **every**
    part is fixed (all 58 footprints are KiCad-locked in that file), U1 at (110.5, 126.5), TP3 on
    the back.
  - `blocks_keep_their_regions`: rp2350-battery files above with a rewritten spec (rect, polygon
    and `never` barrier regions; `except`; an unknown block is an error).
  - `a_lifted_cluster_moves_as_one_with_its_bottom_side_member`: easy2350 floorplan files above
    plus a lifted crystal layout written to a temp dir; the flipped member goes to the back at its
    offset.
- `app/core` tests also exercise it (warm anneal keeps a locked part and stays legal, analytic
  start legal on the battery board, stopping an anneal).
- Benchmark (`docs/floorplan-benchmark.md`, 5 seeds × budgets): with SA every board is legal 5/5
  at 50k and 200k; the only miss in either table at 50k+ is rp2350-battery at 1M (4/5, both
  placers). Cypress boards always legal.

## Known wrong or weak

- **Courtyards are axis-aligned bounding boxes.** Real courtyard polygons are never used; an L- or
  round courtyard wastes room, and a non-quarter-turn fixed part gets its rotated box's box
  (`bbox_at`, lib.rs:375). Rotations are quarter turns only.
- **Shaped outline containment is sampled**, not exact: 8 points of each box against a 0.1 mm
  signed-distance grid (lib.rs:834). A sharp concave notch between samples is missed. Regions,
  by contrast, are exact (area clip). Two different "inside a polygon" tests in one cost.
- **Edge and fixed parts are exempt from the outline check** (lib.rs:814, 831); an edge part's
  placement relies entirely on the edge term, so a rejected edge part shows as an `edge_miss`, not
  an overhang.
- **Length limits are a proxy**: a net's `max_length` becomes a box of `LIMIT_BOX` = 0.75 of it
  (`floorplan_cmd.rs:304`), soft only. Corridor, crossings and limits are not in `score()`, so the
  final ranking ignores them.
- **The analytic placer omits terms**: no `max_mm` limit, corridor, planar crossings or flush-edge
  margin in its gradient (analytic.rs, none referenced). They only act in the cool anneal after.
- **w_cross is off by default** because it hurt (202869c): crossings 5–8 → 1 and QSPI fails 8.25 →
  7.0, but crystal fails 1.25 → 3.5, package fails 12.75 → 13.25.
- **`best` resets every 2000 moves** (`best.0 = INFINITY` at each sync, lib.rs:1103) and is only
  recorded after 90 % progress: the result is the best accepted state since the last re-sync, not
  the best seen in the last 10 %. Probably deliberate (costs at different overlap weights are not
  comparable) but undocumented.
- **`legalise` restarts its O(n²) scan after every single push** and gives up after 500 pushes
  without saying so; `negotiate` is the real legaliser. It ignores regions and edges.
- **`negotiate` leaves a separation that is already short** when the part in trouble isn't the
  cause (floorplan-benchmark.md, "Separations are kept, not repaired").
- **About 1 in 5 runs on a cramped shaped outline miss a separation** (battery board in an ellipse
  or hexagon, 10 seeds × 2 placers, 200k; floorplan-benchmark.md 2026-10-31).
- **rp2350-battery is 4/5 legal at 1M moves** with either placer (benchmark table); easy2350's
  analytic row "moves between runs" at 5 seeds.
- **Separations measure block bounding boxes**, so two interleaved L-shaped blocks can be "far"
  or "near" wrongly. Unverified in practice.
- **Docs disagree with the code**: the System Spec (line 19, line 134) says the floorplanner
  "places blocks as soft regions", "honours the domain tree" and outputs "domain regions"; the code
  places parts, has no notion of domains, and outputs no regions (regions are input only, `must`
  and `never` — no `prefer`/`avoid` weights). The Spec also lists the intent file as an input;
  only `max_length` and over-ground nets are read from it, by ccomp (`limit_nets`).
  `docs/floorplanning-approaches.md`'s recommendation still names `repair` (replaced by
  `negotiate`, cbb3be4), the `routing.json` handoff (going away in Boardroom) and "copperroute".
- `docs/floorplan-benchmark.md`'s note that the earrings' analytic run (4–5 s) "hasn't been
  profiled" is stale: 2.4 s after cbb3be4, most time was the old `repair`.
- No TODO/FIXME/XXX markers in the crate.

## Not covered by tests

- `solve_with`'s warm start "kept when nothing better is found" path (lib.rs:1067, 1161) is only
  exercised through app/core, never asserted to keep the start.
- `Snapshot`/stop: only app/core's `an_anneal_can_be_stopped`.
- `evaluate` on rigid groups and pseudo-parts (it places members from the anchor's pose,
  lib.rs:1380): only reached through `--evaluate` in one ccomp test, metrics not checked beyond
  wire.
- `through` pads blocking the other side in `legalise`/`negotiate` (lib.rs:1418): no direct test.
- Swap move, edge-slide on a rect outline with `keep_along`, and `seat_edges` have no test of
  their own.
- `fill`'s halo sizing across front and back separately (`halos`, lib.rs:417): one front-only test.
- `Sides::Back` regions, and a `never` region with members.
- Determinism across platforms (WASM vs native give the same placement for the same seed): not
  tested. Unverified.
- The benchmark is not a test: `tools/fp_bench.py` must be run by hand.

## Lessons to keep

- **Judge a placement change over several seeds, never one.** Source:
  `boards/rp2350-battery/routed/README.md` (202869c): the same setup gave 10, 16, 14, 11 package
  fails across four placements (mean 12.75), and w_cross's "improvement" was inside that spread.
  easy2350's analytic row needed 20 seeds, not 5 (cbb3be4). Run `--seed 1..N` and report median and
  spread; the benchmark is 5 seeds minimum.
- **Incremental cost gave ~10x** (f52cb41): bolt earring 200k moves 30.5 s → 2.8 s, same placement
  per seed. Keep `State::cost` as the reference and the equality test.
- **Legal first, then cost** (1d0d5fa): a 0.0005 mm² overlap cost less than the wire to clear it,
  so an illegal placement won. Rank by rules missed, then cost.
- **A repair that looks for a clear spot fails on full boards** (cbb3be4): the earrings' backs have
  ~180 reserved keep-outs and no clear spot for some parts anywhere (0.1 mm scan). Negotiation
  with eviction and history cost made both earrings 5/5 legal at every budget (flame was 0/5 below
  1M) and easy2350's analytic start 20/20 legal with no anneal (was 6/20).
- **Separation must rise with the overlap weight** (6f2fbbd, 975c0d1): a constant pull left
  aggressor/victim blocks interleaved; easy2350 seed 3 ended 0.89 mm short of one.
- **Analytic beats SA on Cypress's boards** at every budget on 9 of 10 (small-9, 476 parts: 1624 mm
  in 1.5 s vs SA 1762 mm after 1M moves / 17 s), not on small-7 (bad basin). On our small boards
  SA at 1M is as good. The app defaults to analytic start.
- **With an outline, nothing used free room**: parts packed into one corner until the fill/halo
  term (5f7c04f), default fill 0.5.
- **Edge parts on shaped outlines** need the signed distance, not the bounding box (1d0d5fa): on an
  ellipse every run was "legal" with a USB face 2.2–7.2 mm off the board. SDF padded 6 mm, not 3,
  so far-off parts read true; that padding alone changed the earrings' wire noticeably.
- **A near hint measured from the courtyard centre fenced pins in** (87f7a33): with only decouplers
  facing their pins, caps on RP2350 pins 20 and 23 boxed in XIN/XOUT. Measure from the part's own
  pad and keep the line clear (corridor).
- **Rigid copper must be obstacles** (975c0d1): R9 and C20 were placed on the core regulator's
  locked copper. Margin at a flush edge: C3's pads ended 0.287 mm from the edge against 0.3.
- **Generated hot loops are fixed by the layout, not the placement**: the TLV62569 hot loop ranged
  2.98–11.4 mm² when placed freely, 3.53 every seed as a rigid layout (rp2350-battery routed
  README). Placing power blocks as locked rigid groups is the design.
- easy2350 in its own outline: legal, ~763–790 mm wire vs the person's 878 (fc9f13b, README).
- The routing boundary (floorplanning-approaches.md): a placer needs a model of routing (airlines
  and crossings, congestion vs capacity, escape room), not a router; check any routability proxy
  against real routed results before trusting it.

## For the move

- **Tests to bring into `crates/floorplan/tests/`** (the unit tests in `src/` come with the code):
  - The unit tests read `app/web/public/examples/{rp2350-battery,easy2350,bolt-earring,flame-earring}/problem.json`
    via `CARGO_MANIFEST_DIR/../../app/web/...` (lib.rs:1317, 1620). In Boardroom the web app is
    `web/` and the earrings leave for their own repos; the earrings' KiCad inputs were never
    committed, so these `problem.json` files (20–132 KB) are the only fixtures that reproduce
    them. Copy all four into `crates/floorplan/tests/data/` and point the tests there.
  - The ccomp integration tests become `boardroom compose floorplan` tests (or, better, tests that
    call `floorplan_cmd::problem` directly). They need, relative to the repo root:
    `boards/easy2350/floorplan/{composed.kicad_pcb,composed.json,spec.json}`,
    `boards/easy2350/input/easy2350.kicad_pcb`,
    `boards/rp2350-battery/{composed.kicad_pcb,composed.json,floorplan.json}`,
    `packages/rp2350/layouts/qfn60-4layer/layout.json`,
    `packages/tlv62569/layouts/sot23-5-4layer/layout.json`.
- **Boardroom's `examples/easy2350` does not hold what these tests read** (checked 2026-11-12):
  - It has no `floorplan/` at all: `composed.kicad_pcb`, `composed.json`, `spec.json` are missing
    (also `placement.json`, `placed.kicad_pcb`, which the app's examples use). Copy them in, e.g.
    to `examples/easy2350/compose/easy2350/` per the "each mode writes its own folder" rule.
  - Its board is `hw/easy2350/a/easy2350.kicad_pcb`, not `input/easy2350.kicad_pcb`, and it is a
    **different file**: same 58 footprints at the same positions, but re-routed (581 segments vs
    819) and only **16 of 58 footprints locked** against 58 of 58. `a_footprint_locked_in_the_board_stays_where_the_board_has_it`
    asserts every part is fixed and will fail on it; either lock them, keep Composer's
    `input/easy2350.kicad_pcb` as a test fixture, or change the test to check only the locked ones.
    The `--evaluate` wire comparison should be unchanged (same positions), unverified.
  - It has no `easy2350.net` (other ccomp tests may need it; not the floorplan ones).
  - rp2350-battery is not in Boardroom's `examples/` yet; per the spec it goes there. Bring
    `composed.kicad_pcb`, `composed.json`, `floorplan.json` (and `intent.json`, read when the
    board is resolved as a project).
  - `packages/` does not exist in Boardroom yet; the spec rigid layouts resolve against `--root`
    (default cwd), so paths like `packages/rp2350/layouts/...` need the Boardroom `packages/`
    tree in place, or the spec paths rewritten.
- **Context trap in ccomp**: `Ctx::active()` (main.rs:545) turns true when the cwd has a
  `compose/` directory, and then the floorplan command pulls `shape.json`, `intent.json` and the
  board's spec from the project instead of the arguments' defaults. Tests run with cwd = repo
  root; make sure Boardroom's root has no `compose/` directory, or pass explicit paths/flags.
- **Dependents**: `wasm` (from `app/core`) and `boardroom compose floorplan` (from ccomp). The crate
  itself needs no rename and depends on nothing in Composer, so it fits the dependency rule
  (Composer group). `floorplan_cmd.rs` (Problem from `pcb::Board` + composition + spec + intent)
  belongs in `floorplan` behind a feature or in `compose`, not in the binary: it is real logic
  (spec parsing, regions by block, rigid layouts to pseudo-parts, keep-outs, `limit_nets`), and
  it uses `pcb::Board`, which becomes a module of `board` with the same API.
- **`tools/fp_bench.py` → `bench/`**: it shells out to `target/release/ccomp floorplan`, reads the
  four `app/web/public/examples/*/problem.json`, fetches Cypress's ten small boards (Apache-2.0,
  pinned NVlabs/Cypress 743b07c) into `bench/cypress/`, and writes `bench/results.json` and
  `docs/floorplan-benchmark.md`. Composer's `bench/` is gitignored. Repoint `CCOMP` to the
  `boardroom` binary (`compose floorplan`), the problems to the fixtures above, and keep the
  Cypress download out of git. Run under `reserve` (`-c 12`).
- **Docs**: move `docs/floorplanning-approaches.md` and `docs/floorplan-benchmark.md` to Boardroom
  `docs/`; correct the stale `repair` and `routing.json` mentions.
- **What I would do differently**: replace the `:`-in-reference pseudo-part convention with an
  explicit `kind` field on `PartGeom`; use real courtyard polygons (at least for the outline and
  region tests) and one exact containment test for both outline and regions; make the analytic
  gradient include the limit/corridor/crossing terms or document that it doesn't; split
  `lib.rs` (cost, anneal, edges, metrics) — the cost function is one 220-line method; turn the
  benchmark into a reproducible bench target that the "several seeds" rule can be run against by
  any change.
