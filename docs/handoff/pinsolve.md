# pinsolve → boardroom crates/pinsolve

The pin solver. A **loose composition** says "the IMU's I2C to the MCU" (`connect` entries)
instead of naming pins; this crate finds, through each package's typed interfaces and pin-mux
table (`interfaces`), every way the partner can serve each connection, judges logic levels pin
against pin, and searches all assignments (branch and bound) for the cheapest. Then two rules add
circuitry: the **I2C rule** (pull-ups sized to SLVA689A's window, added as a recipe or extra
pairs omitted) and the **supply rule** (`src/supply.rs`, commit 0def710: a regulator for each
named rail that has loads and no feed, chosen from cited power contracts, with every candidate's
pass/fail and reasons). Out comes a composition `compose` accepts. Size: 1,917 lines of `src`
(lib 799, supply 665, levels 276, rules 177; unit tests included), 477 lines in `tests/`.
Depends on `compose`, `interfaces`, `serde`, `serde_json`. Used by `ccomp` (`pins`,
`check supplies`) and `app/core` (`pins_solve`, `compose`, which take packages as JSON via
`from_json`).

## Model in five minutes

Inputs: the loose composition as JSON, `interfaces::Types` (`library/interfaces.json`), and
`BTreeMap<instance, Package>` where `Package` (`src/lib.rs:81`) = the instance's `Circuit`, its
`Ports` (`ports.json`, or a recipe's expanded one) and recipe port voltages. `load` (:178) reads
them from `packages/` (+ recipes from `packages/../library`); `from_json` (:201) takes them from
JSON (the app); `add_recipes` (:191) expands recipe instances.

`solve(loose, types, pkgs, node_limit)` (:244) → `Solved { ok, composition, pinout: Vec<Row>,
cost, exhaustive, warnings, errors, added }`:
1. Each `Connect` (:58: `from` = `INSTANCE.INTERFACE` or a board part pin `REF.PIN` with
   `type`; `to` = instance or `INSTANCE.INTERFACE`; optional `name`, `role`, `pins` to pin some
   signals, `bus_pf`) is `prepare`d (:579): the device's ends (its interface must be on fixed
   pins), signal directions, and every `Way` the partner offers: same type in the complementary
   role (`complement`, :123), or a block that `implements` it (PIO; `soft`, cost 100).
   Signals pair through the type's `joins`.
2. Per way, each device signal's candidate partner pins (`Ports::options`): on a circuit port
   that is a signal port, not already on a net, not taken by a fitted part inside the package
   (`inside`, :222; an unfitted `DNF` link on the pad costs 2), matching `pins` if given, and
   passing `level_ok` (:550, `levels::drive`). The product of candidates gives `Pick`s with cost
   = soft 100 + pin-number spread + 3 per ADC-capable pin used non-analog + DNF links + 1e-4 per
   pin number (tie-break). The product is truncated to 20,000 by spread.
3. `Search::go` (:723): connections ordered buses first, then fewest picks; bound = cost so far +
   each remaining connection's cheapest pick; opening a hardware bus costs 4 (not in the bound);
   picks within `TIE` = 0.01 of the best are not explored. `fits`/`take` (:756, :785) enforce
   exclusivity: a non-bus, non-per-pin interface is held whole; bus signals are shared at one
   pin; `per_target` signals (SPI CS) are not shared; PWM channels are shared by signal.
   `exhaustive` = the node limit (2,000,000 in tests and the app) was not reached.
4. Nets are named (`name`, the interface for a shared bus, else `FROM_UPPER`; `_SIGNAL` suffix
   for multi-signal connections); a device signal already on a composition net brings the
   partner pin onto that net. I2C targets sharing a bus: error if their address sets can't be
   distinct, else a warning to check straps.
5. `rules::i2c` (`src/rules.rs:67`): per bus, mode from the slowest device's `max_khz`, Cb =
   `bus_pf` or 10 pF per device + 20 pF wiring (assumptions, stated in `interfaces.json`),
   Rp(min) = (V − VOL)/IOL, Rp(max) = tr/(0.8473·Cb). Pull-ups found on the targets'
   packages (a `Device:R` from the bus port to a supply port); none → add recipe
   `i2c_pullups` as `<iface>_pu`; several pairs in parallel below Rp(min) → `omit` all but the
   first target's.

`levels` (`src/levels.rs`): `rails` (:49) = rail voltages from the composition's `rails`, recipe
`volts`, and `power_out` functions' `nominal_v`. `of` (:112) = a port's levels (`vih`, `vil`,
`voh`, `vol`, `vin_max`, `vin_max_unpowered` + `when`, `iol_ma`, `ioh_ma`, `iin_ua`) evaluated as
expressions of `supply` at both ends of the rail's range, worst case; levels written
`at_supply_v` are ignored more than 5 % away from it. `drive` (:211) refuses a high below VIH, a
high above VIN_MAX, a low above VIL; it scales VOL/VOH linearly down to the receiver's actual
load (its `iin` plus its package's pull resistors) when that is below the datasheet's test
current.

`supply` (`src/supply.rs`). `candidates(packages)` (:59): every package variant with a
`power_out` that is not `source` or `charger`, with its verified layouts (subfolders of
`layouts/` holding `verify.json`). `run(out, comp, pkgs, cands)` (:375):
- `picture` (:225) places every power contract on its net: consumers (`power_in`), feeds
  (`power_out`, each `also` path a feed of its own), sources (`source` topology, and the
  composition's `sources`, e.g. `"VBAT": {"v": [3.0, 4.2], "kind": "lipo"}`).
- Net ranges from the sources outward, iterated up to 8 times; `output` (:265): an LDO follows
  `VIN − dropout_v.max`, a buck or switch `VIN − I·r_on`, `off_below_v` turns it off, a boost's
  top is the higher of `v_out` and its input. Budgets from the loads up (`budget_of`, depth ≤ 8):
  cited draws (typical-only counts as max too, said), plus each fed regulator's input current
  (`input_current`, :289: converters by power over efficiency at the lowest input, lossless
  without a cited efficiency, said; LDO/switch = load + iq; charger = its i_max).
- Every net with consumers: its feeds `judge`d (:308): input range within `from`'s `v`;
  `i_max_a` ≥ max draw; output within the consumers' common range across the input range (the
  reason names the binding consumers); junction = ambient (`ambient_c`, else 25 C, said) +
  θJA × worst dissipation ≤ `tj_max_c` when both are cited. Failures are errors.
- A net named under `rails` with consumers and no feed is `fill`ed (:513): each candidate's
  output judged from each input (`rails.X.from`, else every net with a known range), best input
  kept; ranked by (fails, no layout for `intent.layers`, loss at typical load and mid input in
  10 % log buckets with unknown last, parts, name). `rails.X.supply` pins the choice (placed even
  if it fails, failures as errors). The pick is added as `as`, else `reg_<rail>`: output on the
  rail, input on the input net, grounds on a consumer's ground net (else `GND`), enable left
  open when `on_when_open`, tied to the input when `tie` is the input, else an error; other
  signal ports go to `no_connect`.
- `Report { rails: Vec<Rail>, warnings, errors, added }`; each `Rail` has its range, `fed_by`,
  `budget` with items, `needs_v`, `options: Vec<Judged>` (ok, reasons, out_v, loss_w, parts,
  layout, notes, input_a), `chosen`.
`ccomp check supplies` calls `run(…, None)`: same budgets and checks on a drawn composition's own
regulators.

Data read: `packages/*/ports.json` (`functions` with `levels` and `power`, `variants`,
`interfaces`, `mux`), `packages/*/schematic/circuit.VARIANT.json`, `packages/*/manifest.toml`
(version), `packages/*/layouts/*/verify.json` (existence only), `library/interfaces.json`
(types, `i2c.rules.pullup`), `library/recipes/*.json`, `library/stock.json`.

Invariants: the solver's output must compose (tests check `compose` on every solved board); a
pinned solve of a loose board must reproduce the hand-drawn board pin for pin (battery and bolt
tests); results are deterministic for a given input (tie-break by pin number, ranking ends on
name).

## Solid

All 42 tests of compose/pinsolve/interfaces/library passed on 2026-11-12 (`cargo test -p
compose -p pinsolve -p interfaces -p library`, booked through `reserve`).
- `tests/battery.rs` (reads `boards/rp2350-battery/loose.json`, `compose.json`):
  `the_loose_board_solves_and_composes` (exhaustive; IMU on a hardware I2C at F3 on adjacent
  pins; USB through the package's 27 Ω resistors; GPIO0 avoided because of the flash's DNF link;
  no pull-ups added), `pinned_it_is_the_hand_written_board` (pin sets and designators equal;
  only D1's value differs: compose.json's C72043 is discontinued), `two_imus_share_one_bus`,
  `a_soft_block_when_asked_and_refusals_with_reasons` (PIO at cost ≥ 100; SPI refused because
  the package ties CS; an interface used twice), `packages_given_as_json_solve_the_same`.
- `tests/rules.rs` (battery `loose.json`): `a_bus_without_pull_ups_gets_a_pair` (4.7k for 400
  kHz, ~40 pF, window 967 Ω–8.85 kΩ), `too_many_pull_ups_in_parallel_are_omitted`,
  `a_5v_status_line_goes_to_a_5v_tolerant_pin` (and refused on an ADC pin, with reason),
  `an_led_chain_on_the_cell_needs_a_level_shifter`.
- `tests/supply.rs` (battery `loose.json` with `reg` removed):
  `a_cell_cut_off_at_3v3_gets_the_buck_wired_as_the_drawn_board` (TLV62569 chosen, with its
  4-layer layout; budget 16.3 mA typ; NCP730 fails at 2.82 V against USB_OTP_VDD; wired exactly
  as the hand-drawn `reg`), `a_cell_taken_to_3v0_has_no_supply_and_says_why` (buck fails with
  "the output falls to 2.997 V at a 3 V input; mcu.USB_OTP_VDD needs 3.135 V"),
  `a_pinned_supply_is_placed_with_its_failures`.
- `tests/bolt.rs` (bolt earring): `the_blocks_are_the_bolt_net_for_net` (192 nets, 256 parts,
  with pad aliases), `the_loose_bolt_solves_and_composes` (15 connections, exhaustive, three
  sensors on ADC pins), `pinned_the_loose_bolt_is_the_blocks`.
- Unit: `levels::tests::a_low_scaled_to_what_the_input_draws`,
  `supply::tests::an_ldo_follows_its_input_below_dropout`,
  `supply::tests::a_buck_at_full_duty_drops_its_current_times_r_on`.

## Known wrong or weak

Spec status ("Supply selection", 0def710): built are power contracts and the supply rule with
`ccomp pins` and `ccomp check supplies`; **not built: area and stock in the ranking, recipes as
candidates, the app's views.** In code:
- **Recipes are never candidates.** `candidates` scans only `packages/`; `Candidate.recipe` is
  always false, the `json!({"recipe": …})` branch in `fill` is dead.
- **Ranking lacks area and stock** (spec ranks them 5th and 6th); ties fall to name.
- **Pass elements are candidates.** `outputs_of` (:85) excludes only `source` and `charger`, so
  load switches and ideal diodes (tps22913, lm74700) are judged as regulators for a named rail.
  They fail on output range in practice, but appear in `options`. The spec says candidates are
  those whose `v_out` lies in every consumer's range; the code judges all and does not prefilter.
- **The WASM core does not run the supply rule** (`app/core/src/lib.rs` `pins_solve` calls
  `solve` only), so the app shows no `supplies`.
- **Orchestration lives in the binary.** `ccomp pins` (`crates/ccomp/src/main.rs:975-1031`)
  runs `solve`, then `supply::run` with candidates, then re-loads and re-runs `supply::run`
  without candidates to budget upstream of the added regulators, then merges the two reports,
  then composes. None of that sequence is a library function or tested as one.
- Physics approximations, all stated in the code: LDO dropout uses `dropout_v.max` at every load
  (its `at_a` is ignored; conservative); one efficiency figure is used at every load and input;
  a converter with no cited efficiency is costed lossless; a net fed several ways counts its load
  on every input (conservative); thermal is θJA × worst-case dissipation at 25 C unless
  `ambient_c` is given; switch/diode worst dissipation omits iq; recursion and range iteration
  are capped at depth/passes 8 (a cycle in the power graph would be cut silently).
- `fill`'s enable handling ties EN only to the input port; any other `tie` target is an error
  ("must be driven").
- A layout counts for ranking if its folder name contains `<layers>layer` (substring match).
- A pinned `supply` is matched by package name, spec or display name; with several variants the
  first in rank order wins.
- Two copies of an output's voltage: `levels` reads `nominal_v`, the supply rule reads
  `power.v_out`. Nothing checks they agree (they do in ncp730 and tlv62569 today).
- Pin solver: the 20,000 truncation of a way's candidate product sorts by spread only, ignoring
  the ADC and DNF costs, so it can drop the cheapest pick (only for interfaces with very many
  options; not hit by the tests). Ties within 0.01 are not explored, so `exhaustive` means
  "exhaustive up to ties". `fits` always returns `Some(0.0)`; its "extra cost" is vestigial.
  `excludes` is checked only between a device's own interfaces, never on the partner side
  during search (an MCU interface that excludes another can be used together with it).
- I2C rule: looks for pull-ups only on targets' packages (not the controller's, not board
  parts); takes the first target's port to find the rail; keeps the first target's pair when
  omitting, whatever its value; Cb is an estimate, not a citation.
- `level_ok` errors at net-naming time are swallowed (`if let Ok(notes)`), so a warning can be
  lost; harmless because pins were filtered by it already.
- The chip is assumed to be role `U1` (`port_of`, :214; `levels::of`).
- `candidates` silently skips a variant whose circuit file is missing (RP2350B has none).
- Clippy (rustc 1.95, default lints) warns 6 times: `supply.rs:551` (sort_by_key),
  `lib.rs:293,326,462,587` (very complex type), `lib.rs:688` (matches!). Boardroom's bar is
  "clippy is clean".

## Not covered by tests

- `supply::run` with no candidates (`ccomp check supplies` on a drawn board): no test at all.
- `ambient_c`, thermal failure, `off_below_v`, boost and buck_boost outputs, `also` paths judged
  as feeds (exercised only incidentally by lipo_load_share in the battery board), a source whose
  `i_max_a` is below the budget, consumers whose ranges don't overlap, `rails.X.as`, a candidate
  wired with `enable.tie`, the "no ground net" error.
- The `ccomp pins` two-pass merge described above.
- `prepare`: `role` given and not offered; a board part pin with a multi-signal type; `pwm`
  from a part pin (`A|B`).
- Search with the node limit reached (`exhaustive: false`); the 20,000 truncation.
- I2C: Cb above the mode's maximum; targets on different rails; a pull-up to the wrong rail;
  no `max_khz` anywhere (falls to the first mode).
- `levels`: `at_supply_v` out of range; `vin_max_unpowered` notes other than the 5 V STAT case.
- `from_json` error messages; `summarize` with mixed refusals.

## Lessons to keep

- **Costs** (902b05a): soft block 100, opening a new hardware bus 4, an ADC pin used digitally 3,
  a DNF link on the pad 2, pin spread in pin numbers, 1e-4 per pin number as tie-break. Buses
  searched first and costs within 0.01 treated as ties made the bolt solve exhaustively in under
  a second (5fc7fdf); before that a dozen GPIO connections were tried in every order of nearly
  equal cost.
- **Pinned, the loose board must be the drawn board.** Both demo boards are tested that way; it
  is how the solver and the recipes were validated against real designs.
- **Levels scaled to the real load** (5fc7fdf): a datasheet's VOL is at its test current (RP2350:
  2 mA); a 1 µA input sees ~0.25 mV, so an RP2350 pin can drive the TPS22913's ON (VIL 0.4 V).
  Without `iin_ua` the datasheet figure is used, which refuses that pin. Linear scaling is an
  upper bound in the output transistor's linear region (comment at `levels.rs:9-15`).
- **The LiPo case decides the supply rule:** whether a 3.3 V regulator is acceptable depends on
  what its consumers accept at the bottom of the cell, not on staying in regulation. On an
  RP2350 rail the floor is 3.135 V (VREG_AVDD, USB_OTP_VDD): the NCP730 (480 mV dropout) fails on
  a cell to 3.0 V and to 3.3 V; the TLV62569 at 100 % duty passes at 3.3 V cut-off and fails at
  3.0 V (2.997 V). The result names the consumer that sets the limit.
- **Fixed ranking order, stated** so a person sees why one option outranks another, and a
  pinned choice that fails is still placed with its failures as errors (spec decision log).
- **A 5 V-tolerant pin is tolerant only while IOVDD is up**: modelled as `vin_max_unpowered` +
  `when`, reported as a warning, not a refusal (29d0e8d).
- **Pull-ups in parallel:** two IMUs with 4.7k each (2.35k) stay above Rp(min) 967 Ω at 3.3 V; two
  with 1.5k (750 Ω) do not, and the rule omits one pair.

## For the move

- Lands as `crates/pinsolve`. Depends on `compose` and `interfaces` (same group or above): fine.
- **Test data.** These tests read, relative to the repo root:
  - `tests/battery.rs`, `tests/rules.rs`, `tests/supply.rs`: `boards/rp2350-battery/loose.json`
    (all three) and `boards/rp2350-battery/compose.json` (battery.rs only). rp2350-battery goes
    to Boardroom's `examples/`; point `root().join("boards/rp2350-battery/…")` there.
  - `tests/bolt.rs`: exactly three files, `boards/bolt-earring/compose.json` (11 KB),
    `boards/bolt-earring/loose.json` (6 KB) and `boards/bolt-earring/circuit.json` (75 KB; read
    only for `components.*.pins`, the bolt's netlist). bolt-earring becomes its own project, so
    cut these three into a fixture (e.g. `examples/bolt-earring/` or
    `crates/pinsolve/tests/fixtures/bolt/`). `circuit.json` could be reduced to the
    part → pin → net map. (`shape` tests separately read `boards/bolt-earring/shape.json`.)
  - All four also read `packages/` (rp2350, mcp73831, tlv62569, lsm6dso for the battery board;
    rp2350, lgs4056h, ncp730, usblc6, lsm6ds3trc, tps22913 for the bolt) and `library/`
    (`interfaces.json`, `recipes/`, `stock.json`). `tests/supply.rs` reads **every** package
    through `candidates`, so adding a package with a `power_out` can change its expected ranking
    (it asserts the TLV62569 is first and the NCP730 fails).
  - `packages_given_as_json_solve_the_same` reads `packages/{name}/ports.json` and
    `packages/{name}/schematic/circuit.{variant}.json` directly.
- `load` and `compose::library_dir` assume `library/` is the sibling of `packages/`; `ccomp pins`
  derives `library/interfaces.json` the same way. Keep the sibling layout or pass paths.
- `ccomp pins` and `ccomp check supplies` become `boardroom compose pins` / `… check supplies`.
  Move the two-pass sequence from `ccomp/src/main.rs:975-1031` into a `pinsolve` function
  (`solve_with_supplies`) that the CLI, the server and the WASM core all call; then the app can
  show `supplies`.
- Differently: make recipes candidates (they already expand to `ports.json` shape); prefilter
  candidates to regulating topologies; add area (from the footprint courtyards the floorplanner
  already reads) and stock (needs a stock snapshot per part; see `research-library.md`); read the
  output voltage from one place (`power.v_out`) and drop `nominal_v` for power_out functions;
  enforce partner-side `excludes` in `fits`; give each package its chip role explicitly.
