# blockgen → boardroom crates/blockgen

`blockgen` generates critical blocks instead of routing them: today one generator, the buck.
Given a board with the regulator IC and its CIN, inductor, COUT (and optionally a feedback
divider RFB_TOP/RFB_BOT/CFF) on it, it places those parts around the IC by beam search over
grid poses, draws the copper that joins them, adds ground vias and an optional keep-out under the
switch node, and emits a **layout in the IC's frame, by role** (not reference or net name), so it
can be transplanted onto any board with the same IC footprint. Size: 997 lines in `src/` (buck.rs
921, layout.rs 70, lib.rs 6), 48 in `tests/`. Depends on `pcb`, `verify`, `serde`, `rayon`
(dev: `serde_json`). Depended on by `ccomp` only (`ccomp gen buck`).

## Model in five minutes

- `buck::generate(board, &BuckRoles, &Params) -> Result<Generated, String>` (`src/buck.rs:442`).
  `BuckRoles` is `verify::buck::BuckRoles` — **the only thing blockgen takes from verify**.
- `Params` (`buck.rs:25`, defaults at `buck.rs:87`, serde `default` + `deny_unknown_fields`):
  grid 0.1 mm, search band (`reach` 5, `along` 4, `back` 0), rotations 0/90/180/270,
  `clearance` (None = the board's net classes via `Board::clearance`), `power_widths`
  [0.4, 0.3, 0.2], `fb_width`, `escape` 0.2, `neck` 0.6, `fb_clearance_min` 0.3 (hard floor),
  weights `w_hot` 1, `w_sw` 0.5, `w_out` 0.5, `beam` 60, `beam_inductor` 240, `diversity`,
  `gnd_vias`, via size/drill, `layer` F.Cu, `keepout_layer` In1.Cu, `keepout_reason` (defaults
  to the RP2350 datasheet citation), divider reach/beam/attempts, `gnd_return_vias`.
- Order of search: (1) CIN, scored on its hot-loop estimate, kept only if the SW pin can still
  escape; (2) inductor, scored on switch-node copper plus a lookahead on the output loop; (3)
  COUT, scored on the output loop; (4) feedback run (straight or one bend from a corner grid),
  or the divider (RFB_BOT, RFB_TOP with the VOUT sense from COUT, CFF), each a small beam;
  (5) ground vias in rings round CIN's (and divider's, COUT's) ground pad, clear of the keep-out
  and the inductor body. Copper: an escape straight out past each pin's tip, necked to pin width
  for `neck`, then one straight run (or corner/jog variants for SW).
- `Estimate` (`buck.rs:126`): hot loop, switch-node area, output loop, FB clearance, score =
  `w_hot·hot + w_sw·sw + w_out·out` (mm²). **Its own model**: loop polygons through pin centres,
  escape points and pad centres, `geom::loop_area(_, 60)`; switch area = sum of pad areas and
  segment length×width (not a union); FB clearance against its own capsules.
- `Generated` (`buck.rs:135`): `layout`, `estimate`, `tried`/`legal` per step, `runners_up`.
- `layout::Layout` (`src/layout.rs:10`): `block_type`, `anchor_footprint`, `parts` by role
  (`Pose { at, rotation }` in the anchor's frame), `tracks` (net = role: vin/sw/gnd/out/fb),
  `vias`, `keepouts` (with reason). `Frame::to_local/to_board` (`layout.rs:57`).
- Files: `ccomp gen buck BOARD --roles k=v,... | --intent F --block B [--params P.json] -o gen.json`
  (`crates/ccomp/src/main.rs:748`) writes `Generated` + `params` + `seconds`. Package dirs keep
  that whole object as `layout.json` for generated layouts; reference layouts
  (`extract_layout.py`) write a bare layout with extra `pours` and `reference` fields that the
  Rust `Layout` type does not have (only Python reads them).
- Invariants: deterministic (rayon results are collected in order and stably sorted; README says a
  rerun reproduces `layout.json` byte for byte, and 36e4b43 says qfn60-4layer regenerated "to
  1e-9 mm"); front-side parts only (`Footprint::of` refuses back-side parts); the inductor body
  admits no copper but its own pads'.

### How the published layouts are produced and checked

1. `tools/bench_buck.py BOARD ROLES.json OUT [--params P] [--layout GEN] [--copper-layers N]`
   (book cores: README runs it under `reserve -c 8`): runs `ccomp gen buck`, then
   `tools/apply_layout.py --fixture` (pcbnew: board reduced to the anchor and the block's parts,
   a solid GND plane on the first inner layer or B.Cu, the layout placed, locked, wrapped in a
   KiCad group `<block_type>_<anchor>`, zones refilled), then `ccomp check buck` (the verifier,
   independently, on the written board) with an SVG, then `kicad-cli pcb drc`. Writes `gen.json`,
   `fixture.kicad_pcb`, `check.json`, `block.svg`, `drc.json`, `bench.json`.
2. The package's `verify.json` (measurements, limits, DRC summary, generator estimate and
   `seconds_on_8_cores`) is committed from that run; no tool writes it.
3. `crates/verify/tests/package_layouts.rs` re-measures every committed `fixture.kicad_pcb` and
   fails if any number moves by 1e-3 or any limit fails.
4. `tools/compare_buck.py SPEC OUT` runs `ccomp check <buck|half-bridge|sense>` on several boards
   from a `compare.spec.json` (vendor board, transplanted board, generated fixture) with the
   same limits and weights, writing `compare.json` and SVGs.

- **rp2350 core regulator** (`packages/rp2350/layouts/qfn60-4layer`, `qfn60-2layer`,
  `qfn80-4layer`, `qfn80-2layer`): generated by `ccomp gen buck` with each dir's `params.json`
  on a bench from easy2350 / the minimal boards; `qfn60-reference`, `qfn80-reference` are
  Raspberry Pi's copper lifted by `tools/extract_layout.py` (95fc202). qfn60-4layer: measured
  hot 1.0253, SW 1.9042, out 2.3429 mm², FB 0.415 mm, underlap 0; estimate hot 1.0244, SW
  2.159, out 2.403; 47.4 s on 8 cores. The compare spec scores reference 3.47, transplanted
  3.44, generated 3.15 (README). All four generated layouts score better than the reference
  (bb5636d).
- **tlv62569 sot23-5-4layer** (36e4b43): roles U5/C22/C23/L2/R19/R18/C21 from rp2350-battery,
  `params.json` with `gnd_return_vias: true`, `back: 4`, no keep-out, widths 0.5/0.4/0.3;
  135.4 s on 8 cores. Measured hot 3.54 (1.72 through pad centres), output 11.05, SW 5.21 mm²,
  FB 2.67 mm, underlap 5.24 mm² (unlimited); DRC clean on its bench. No `compare.spec.json`.

## Solid

All pass (`cargo test -p blockgen`, 2 tests, 15.4 s in release on 8 booked cores, 2026-11-12):
- `tests/buck.rs::coarse_search_on_the_rp2350_bench` (grid 0.2, small beams: legal, hot loop
  < 1.3 mm², 3 parts, `gnd_vias` vias, one keep-out, only role nets).
- `tests/buck.rs::coarse_search_with_a_divider_on_the_tlv62569_bench` (grid 0.25: six parts,
  no keep-out, ≥ 1 + 2·gnd_vias vias, an `fb` track).
- The real guarantee is downstream: the committed layouts pass the verifier's golden tests and
  KiCad DRC on their benches.

## Known wrong or weak

- **Estimates disagree with the verifier** by up to ~30 %: TLV62569 hot 3.04 est vs 3.54
  measured, output 9.81 vs 11.05, SW 5.92 vs 5.21, FB 3.52 vs 2.67 mm; RP2350 qfn60-4layer SW
  2.16 vs 1.90. Causes: 60-cell vs 400-cell rasters, pad-centre polygons vs copper paths, SW area
  summed instead of unioned, ground through the plane not modelled as a path. The tests do not
  compare estimate with measurement (the test doc says "its estimates match what the verifier
  measures", the code does not assert it).
- **Best is not the minimum**: `best = done.first()` (`buck.rs:837`) is the first COUT candidate
  (in step-3 score order) whose feedback route or divider fits; the divider's added cost
  (`buck.rs:797`) never re-sorts `done`.
- Mixed units in the divider score: run lengths (mm) added to the mm² score, VOUT sense at 0.2×
  its length, and a flat +3.0 when the sense is left to the router (`buck.rs:775-778`).
- CFF is searched round `s.placed[n_top].courtyard[0]` (`buck.rs:786`), the first courtyard
  vertex of RFB_TOP, not its centre.
- One copper layer only (`params.layer`); no pours (the vendor references use zones; the generator
  draws tracks, LX 0.2 mm vs the reference's 0.3 mm strip — README caveat); no back-side parts.
- Proxies do not price trace resistance or inductance yet (README), nor switch-node underlap
  (it emits a keep-out instead, or none: TLV62569 measures 5.24 mm² of plane under SW, judged
  by nothing).
- `keepout_reason` defaults to the RP2350 datasheet citation for every buck.
- `role_of` names any net that is not vin/sw/gnd/out `fb` (`buck.rs:895`).
- `bench_buck.py` builds `block_refs` from vin's IC, cin, cout and inductor only (line 75): the
  divider parts' unconnected items are not reported in `open_between_block_parts`. Its
  `block_nets` is computed and unused.
- Spec disagreement: the System Spec's tool table says the generator outputs a "Locked
  sub-layout (KiCad design block)"; it outputs `layout.json`, which `apply_layout.py` turns into
  a locked KiCad **group**, not a design block.
- The README's "30–80 s on 8 shared cores" is for the RP2350; the TLV62569 took 135 s.

## Not covered by tests

- The default-parameter search (tests use coarse grids and small beams); determinism.
- Estimate-vs-measurement agreement.
- `Frame::to_local`/`to_board` round trip; `Layout` (de)serialisation.
- Asynchronous bucks (`diode` role): the generator ignores `roles.diode` entirely.
- The 2-layer and QFN-80 parameter sets (only qfn60-4layer and tlv62569 benches are run).
- `bench_buck.py`, `apply_layout.py`, `compare_buck.py`, `extract_layout.py` (no tests).

## Is "blockgen scores with verify" a contradiction?

The decisions log keeps one verifier "independent of the block generators so a wrong model
can't pass its own output". **Today the code keeps that rule**: blockgen imports only the
`BuckRoles` type from verify and never calls `verify::buck::check`; it optimises its own
`Estimate`, and the verifier measures the pcbnew-written board afterwards (`bench_buck.py`,
`package_layouts.rs`). The 15–30 % gaps above show the two models really are different. They do
share `pcb::geom::loop_area` and the metric definitions, so a bug in those would pass both.

**The one-system spec's sentence "Judging sits above Composer because `blockgen` scores its
candidate layouts with `examine`" does contradict the rule if implemented literally**: a
generator that searches for the minimum of `examine`'s numbers will find `examine`'s blind spots
(straight lines across fills, loops closed straight across parts, no plane return path) and then
be signed off by the same numbers. Either (a) say the dependency is for roles, metric names and
limits only, and keep scoring and sign-off separate (and add a test that estimate and measurement
agree within a tolerance), or (b) let blockgen score with `examine` but sign off with something
that does not share its model: KiCad DRC plus a slow tier (field solver) or an independent loop
measure. Moving the roles types into `intent` would let blockgen drop the dependency altogether.

## Lessons to keep

- A switching block is a small search: about 16k / 227k / 3.9M poses tried at the three steps for
  the RP2350 (39 / 82k / 120k legal), under a minute on 8 cores (qfn60-4layer `verify.json`).
- Without any prior the search put CIN and COUT where Raspberry Pi did (both on x = 2.0 mm in the
  IC frame); the inductor 0.4 mm further in, its LX pad over the pin-48 run (README).
- Keep CIN poses only if SW can still escape: on many regulators SW sits between VIN and PGND.
- Score the inductor with a lookahead on the output loop, or step 2 trades the output loop away
  for less switch copper; keep `beam_inductor` (240) wider than `beam` (60) for the same reason.
- Keep the beam diverse: one candidate per parent, `diversity` cell and rotation, or near-copies
  crowd out alternatives.
- Escape past the pin tip and neck to pin width before widening, so neighbouring QFN pins can fan
  out; the widest power width that fits wins.
- SOT-23 bucks need `gnd_return_vias`: GND sits beside SW across from VIN, and a same-layer ground
  run boxes SW in (36e4b43). Ground vias must clear the keep-out (a via there cannot reach the
  plane it is for).
- The VOUT sense to the divider is drawn when it fits, otherwise priced and left to the router
  (TI's DBV layout runs it on another layer).
- `BLOCKGEN_DEBUG=1` prints, per step, why poses were refused (part clash / no copper option /
  copper clash).
- pcbnew writes the result (`apply_layout.py`) because it rotates and writes footprints right;
  the one-system spec replaces that with the `kicad` writer.

## For the move

- Lands as `crates/blockgen` in the Composer group. Its dependencies become `board` (the `pcb`
  module), `examine` (only for `BuckRoles` — better, move role types to `intent`), `serde`,
  `rayon`. Resolve the independence question above before wiring it to `examine` scoring.
- `ccomp gen buck` → `boardroom compose gen buck`.
- Tests read `../../packages/{rp2350,tlv62569}/layouts/*/{fixture.kicad_pcb,roles.json,params.json}`
  → Boardroom `packages/`.
- `tools/apply_layout.py` is to be replaced by the Rust `kicad` writer: it must place parts in the
  anchor's frame (and `flipped` members on the anchor's other side), add tracks, vias, keep-outs
  and pours (`pours` from `extract_layout.py`), lock everything, wrap it in a KiCad group named
  `<block_type>_<anchor>` (replacing one of that name), refill zones, and support `--fixture`
  (reduce to a test bench with a solid GND plane, `--copper-layers`). The group is how locked
  power blocks reach the router (`board`'s `Block`). `bench_buck.py` and `compare_buck.py` go to
  `bench/`.
- Unify the two `layout.json` shapes (generated: the whole `Generated` object; reference: a bare
  layout with `pours` and `reference`) into one Rust type with `pours`, `flipped` and a source.
- What I would do differently: assert estimate ≈ measurement in the tests; pick the minimum of
  `done` by full score in one unit; draw pours rather than tracks for power nets; add the async
  (diode) case and a boost generator only after the router's boost scoring exists to check it.
