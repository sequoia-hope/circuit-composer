# verify → boardroom crates/examine (its start: the "sound" physics measures)

`verify` is the fast tier of the verifier: physics proxies measured on KiCad geometry in
milliseconds (README says about 1 s on a whole board for the buck checks), so they can run in
placement loops and at sign-off. It measures switching-regulator loops (buck, synchronous or
asynchronous), MOSFET half-bridges with their gate driver, current-sense Kelvin taps, connector
TVS protection, ideal-diode controllers, and net-level paths, return-path reference and
decoupling distance; and draws each as SVG. Size: 1,730 lines in `src/` (bridge 521, protect
402, buck 317, svg 225, nets 175, lib 90), 83 in `tests/`. Depends on `pcb` and `serde` only
(dev: `serde_json`) — **so it depends only on Foundation (`pcb` → `board`)**: no intent, no
Composer crate. Confirmed in `crates/verify/Cargo.toml`. Depended on by `blockgen` (only for
`BuckRoles`) and `ccomp` (`check buck|half-bridge|sense|protection|ideal-diode`, `measure`,
`check package`).

## Model in five minutes

- `Measurement` (`src/lib.rs:55`): `check` name (e.g. `hot_loop_area_mm2`), `block`, `nets`,
  `location` (board mm), `measured`, `unit`, optional `Limit { op: "<="|">=", value }`,
  `passed: Option<bool>` (`None` = measured, not judged), `fix_stage: FixStage`
  (`Intent|Floorplan|Block|Route`), `method` (how it was obtained, e.g. "pad centres: a leg is
  unrouted"). `Measurement::judge` (`lib.rs:83`) applies a limit.
- Each check takes roles (parts by reference, pins as `REF.PIN`), a block name and a map of
  limits, and returns a report with `measurements` plus `geometry` (the polygons measured).
  - Buck: `buck::check` (`src/buck.rs:136`), roles `BuckRoles` (`buck.rs:32`: vin, sw, pgnd, fb,
    cin, cout, inductor; optional diode, rfb_top, rfb_bot, cff). Checks: `hot_loop_area_mm2`
    (VIN pin → CIN → PGND pin along copper, closed straight across the IC; through the diode for
    an async buck), `hot_loop_area_placed_mm2` (pad centres), `output_loop_area_mm2`,
    `switch_node_area_mm2` (union per layer, summed), `feedback_clearance_mm` (FB path to SW
    copper in plan view, IC pins excluded, crossings shielded by another net's fill on a layer
    between are discounted), `switch_node_underlap_mm2` (other nets' fills/tracks on the layer
    next to the SW pin's layer, under SW copper).
  - Half-bridge `bridge::check` (`bridge.rs:200`), sense `bridge::sense` (`bridge.rs:379`),
    shared `legs_loop` (`bridge.rs:161`) and `kelvin_tap` (`bridge.rs:348`).
  - `protect::protection` (`protect.rs:98`), `protect::ideal_diode` (`protect.rs:245`).
  - `nets::path`, `nets::return_path`, `nets::decoupling` (`nets.rs:25/63/134`).
  - `svg::render*` (`svg.rs`): one drawing function per report.
- Everything rests on `pcb::graph::copper_path` and `pcb::geom::loop_area(poly, 400)`.
- Files: reads boards via `pcb::Board::load`. Golden files: each package layout dir
  (`packages/<pkg>/layouts/<name>/`) holds `fixture.kicad_pcb` + `.kicad_pro`, `roles.json`
  (role → REF/REF.PIN), `verify.json` (`measurements`, `limits` as strings `"<= 0"`,
  `not_applicable`, `kicad_drc`, `generator`). No tool in the repo writes `verify.json`; it is
  assembled from `bench_buck.py`'s `bench.json` (unverified how; it is committed by hand).
- Invariant: published package measurements must reproduce to 1e-3 (`tests/package_layouts.rs`).

## Solid

All pass (`cargo test -p verify`, 13 tests, 2026-11-12):
- `tests/package_layouts.rs`: `rp2350_core_reg_qfn60_4layer`, `_qfn60_2layer`, `_qfn80_4layer`,
  `_qfn80_2layer`, `rp2350_core_reg_qfn60_reference`, `_qfn80_reference` (Raspberry Pi's own
  copper, pours and all), `tlv62569_reg_sot23_5_4layer`. Each: every loop routed, no limit
  failed, every measurement equal to the published one within 1e-3.
- Unit tests on synthetic boards (`lib.rs:12` `test_board`):
  `bridge::tests::half_bridge_finds_its_parts_and_measures_the_commutation_loop`,
  `bridge::tests::a_sense_line_into_the_pour_is_not_kelvin`,
  `protect::tests::a_tvs_on_a_branch_is_a_stub_reached_late`, `a_tvs_on_the_line_has_no_stub`,
  `dual_channel_tvs_picks_ground_not_sibling_signal`,
  `an_ideal_diode_sense_into_the_pour_is_not_kelvin`.
- Cross-checked against real boards (commit messages, not tests): extracted reference layouts
  measure as their source boards do (hot loop 1.06 / 1.06 mm², 95fc202); motor controller
  bridges A–C 25.4–25.5 mm², bridge D 91 mm² through phase A's cap (73eb8c6).

## Known wrong or weak

- **No boost check.** The owner's 2026-11-12 decision has the router score boost hot loops
  (output capacitor → output); `verify` has buck only (grep: no "boost" in the crate).
- **`buck::check` has no unit test**; it is covered only by the seven golden fixtures, all
  synchronous bucks. The `diode` (asynchronous) path, the divider FB target and plane
  shielding (33f4bf0) have no test of their own.
- Two different "unrouted" fallbacks: `buck::loop_polygon` (`buck.rs:121`) uses the first pad's
  centre (`Board::pad`), `bridge::legs_loop` (`bridge.rs:161`) the mean of the pin's pads.
  Same idea, two codes; the buck one is worse for exposed pads.
- Loops are closed **straight across each part** (CIN, the IC, the diode), and across fills by
  straight lines (`copper_path`); these are proxies, not current paths. Real loop inductance
  depends on the return plane, which is not modelled (no stackup heights).
- `switch_node_underlap_mm2` looks only at the copper layer next to the SW pin's layer, only at
  fills and tracks (not pads or vias), and knows nothing of 2-layer boards, where the bottom
  ground under LX is correct: the 2-layer layouts mark it `not_applicable` in `verify.json` and
  `compare.spec.json`, not in code. The TLV62569 layout measures 5.24 mm² underlap with no limit.
- `feedback_clearance_mm` is plan-view centreline-to-copper; the FB track's own width is not
  subtracted. With FB unrouted it uses the FB pad's first layer for both points.
- Heuristics by name: `decoupling` takes capacitors as parts whose reference starts with `C`
  (`nets.rs:139`); `protection` guesses ground by net name (`GND|PGND|AGND|VSS|*_GND|GND_*`,
  `protect.rs:112`); `ideal_diode` treats `C*` parts as quiet.
- `return_path` measures tracks only, against fills on the copper layers either side by index
  (no dielectric thickness), and needs refilled zones.
- `BuckRoles::from_map` ignores unknown keys (the bridge/protection ones refuse them); the serde
  derive has `deny_unknown_fields`, so JSON and `k=v` inputs behave differently.
- Spec disagreement: the decisions log says "One verifier, shared with ultraroute, grown from
  `crates/verify`". It is not shared: ultraroute has its own hot-loop measure in
  `tools/designer.py` (the one-system spec lists both as sources for `examine`).
- Spec disagreement: the System Spec's verifier reads "routed board, intent file" and reports
  "severity"; the crate reads roles and limits (ccomp turns intent constraints into limits,
  `ccomp/src/main.rs` `block_roles`), and has no severity, only pass/fail/unjudged.

## Not covered by tests

- `buck::check` on its own (async diode, divider, shielded crossings, underlap).
- `nets::path`, `nets::return_path`, `nets::decoupling` (all of `nets.rs`).
- `svg.rs` (all renderers).
- `Measurement::judge` for `>=`.
- `bridge::check`'s gate, bootstrap and driver-supply measures beyond the commutation loop
  (the one test asserts on parts found and the hot loop).
- `protect::protection` clamp loop and ground-via counts; `ideal_diode` VCAP path.

## Lessons to keep

- |winding| integral for loop areas (shoelace halved Raspberry Pi's output loop; README).
- Two numbers per loop: along the copper and through pad centres (`*_placed_mm2`). The placed
  one is what a floorplanner can score before routing; the routed one is what signs off. On the
  TLV62569 they differ 2× (1.72 vs 3.54 mm²).
- Plane shielding: an FB track crossing SW copper on another layer does not count if a fill of a
  third net lies between them (33f4bf0, motor controller's XL7005A).
- Kelvin tap = first point along the sense path that lies on load-carrying copper (a fill of the
  net, or a pad of a part other than the sensing IC and its capacitors); offset 0 means on the
  shunt/MOSFET pad. INA240s measured 0.24–0.51 mm early (73eb8c6); the LM74700 senses at the pads
  (1f8ec20).
- TVS stub `(d(conn,tvs) + d(tvs,first) − d(conn,first)) / 2`, and `tvs_lead_mm` (reached how much
  before the first other pad). Found real faults: USB ESD 15–16 mm behind two vias; a surge from
  J10 reaches bridge D 26.5 mm before D5 (1f8ec20).
- Dual-channel TVS: pick the shared net that looks like ground and has most pads, not a sibling
  signal (2bbb620, test `dual_channel_tvs_picks_ground_not_sibling_signal`).
- Reported numbers to compare against: RP2350 reference hot 1.064, SW 2.353, output 2.452, FB
  0.365 mm; generated qfn60-4layer 1.025 / 1.904 / 2.343 / 0.415 (score hot + ½SW + ½out:
  3.47 ref vs 3.15 generated). Motor controller's RP2350B within 2 % of the reference but over a
  solid In1 plane (a recorded datasheet-vs-practice dispute); XL7005A hot loop 25–26× the RP2350's.

## What the router's scoring should reuse

The owner's decision (System Spec decisions log, 2026-11-12): power sections arrive as locked,
routed KiCad groups; ultraroute only *scores* hot loops on its routing and on the designer's,
from `routing.json` `"blocks": [{"group", "type": "buck", "roles": {...}}]` (agreed, not built).
Reuse, in this order:
1. `pcb::graph::copper_path` + `geom::loop_area(_, 400)` — the measurement itself.
2. `bridge::legs_loop` (`bridge.rs:161`) as the generic "loop of copper legs" primitive (points,
   routed flag, length); retire `buck::loop_polygon` into it.
3. The buck hot-loop definition in `buck::check` (`buck.rs:178`): legs (VIN pin → CIN on vin)
   and (CIN → PGND pin on gnd), or with a diode (CIN gnd → diode anode, diode cathode → SW pin).
   `BuckRoles::from_map` takes exactly the role map `routing.json` will carry.
4. `Measurement`/`Limit`/`FixStage` as the report shape, so the router's scores and Composer's
   checks read the same.
Write new: a boost check (roles for the switch, rectifier and COUT; legs SW → rectifier → COUT →
ground → IC ground) on `legs_loop`. Keep the `*_placed` variant: it is the router's "before".
Remember the scored board's fills must be refilled by KiCad first (`board`'s `fill` is not
KiCad's).

## For the move

- Becomes the core of `examine`. Its dependency set already satisfies the rule ("`examine`
  depends only on Foundation and Understanding"): replace `pcb` with `board`'s `pcb` module.
- Split by kind when it lands: `examine::loops` (buck, bridge, boost, `legs_loop`),
  `examine::kelvin`, `examine::protection`, `examine::nets`. The SVG drawing should become
  `scene` output (stage crates emit `scene`; `render` draws), not hand-built SVG.
- Golden tests read `../../packages/<pkg>/layouts/<name>/{fixture.kicad_pcb,roles.json,verify.json}`
  via `CARGO_MANIFEST_DIR` — repoint at Boardroom's `packages/`.
- `ccomp check buck|half-bridge|sense|protection|ideal-diode`, `ccomp measure` become
  `boardroom check …` / `boardroom compose check …` (decide which; the router uses them too, so
  not under `compose`).
- Give limits a home in `intent` (they come from a block's `placement_goal.constraints` today,
  translated in `ccomp`), and make `not_applicable` data, not prose in JSON.
- What I would do differently: one roles type per block kind in `intent` (Understanding), so
  `blockgen` and the router need not depend on `examine` just for `BuckRoles`.
