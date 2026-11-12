# recognise → boardroom crates/recognise

Drafts intent from a netlist alone: reference designators, values, footprint names, which pad is
on which net, net names, and pin functions when present. Every class, attribute and block it
writes is `soft`, with provenance kind `recognised`, a `confidence` and `evidence` (one fact per
string), so the validator never lets it bind as a hard constraint. It also estimates each power
rail's current from its parts (never from copper), and scores its output against a hand-written
truth (`compare`). Size: 1,965 lines of `src` including about 260 lines of inline tests; no
`tests/` directory. Depends on `intent`, `pcb` (only for `netlist_from_board`), `serde`;
dev `serde_json`. Depended on only by `ccomp` (`recognise board`, `recognise corpus`).

## Model in five minutes

Files: `src/lib.rs` (passes, output, `compare`), `src/current.rs` (current estimate),
`src/names.rs` (what a net name says), `src/parts.rs` (what a part is, family tables, ratings).

Entry points:

- `recognise(&Netlist, board_name, location) -> Recognised { intent, report }` (lib.rs:205).
- `netlist_from_board(&pcb::Board) -> Netlist` (lib.rs:25): parts and pad nets from a
  `.kicad_pcb` (the PCBench corpus has no schematics, so no pin functions).
- `compare(ours, truth) -> Comparison` (lib.rs:766): per class agree/wrong/missed (net names
  compared without a leading `/`; SIGNAL nets the recogniser leaves out count as unclassified,
  not missed), blocks per type (recognised, truth, recognised sharing any part with a truth
  block of the same type), and `(net, ours continuous, ours peak, truth continuous)` per net.
- `parts::classify(ref, value, footprint) -> (Kind, why)` (parts.rs:31); `names::*` predicates.

The passes run in a fixed order (lib.rs:220-229), each recording guesses through `Cx::guess`
(lib.rs:152): the more confident class wins and the loser is kept in `report.disputes`.

1. `grounds`: by name (names.rs:22); failing that, the net 3+ capacitors return to (0.6).
2. `rails_by_name`: a voltage in the name (`+3V3`, `VCC3V3`, `VBUS` = 5 V) or a supply word;
   a voltage-named net with no decoupling and an underscore is skipped (`LED_5V_EN`). Above 50 V
   is HV.
3. `crystals`: crystal nets CLOCK, load caps and a series resistor to an IC pin; block
   `crystal_<Y>`. Oscillators: their only undecoupled net.
4. `pairs`: partner by suffix swap (names.rs:132), kind by keywords or a shared USB connector;
   DIFF_HS with impedance (USB 90, Ethernet/LVDS/HDMI 100, CAN/RS-485 120 but SIGNAL); block
   `usb_<connector>`.
5. `inductors`: one side a switch node (no cap to ground, an IC/diode/transistor on it), the
   other a decoupled rail = buck; a diode from the switch node to another decoupled rail = boost.
   Roles `inductor`, `sw`, `pgnd`, `vin`/`cin` (buck), `cin`/`cout`/`vout` (boost); block
   `buck_<L>` / `boost_<L>`. Records a `Reg` for the current estimate.
6. `rails_by_structure`: 2+ caps to ground and an IC pin, no name (0.55).
7. `feedback`: two resistors and one IC pin, one resistor to ground, one to a POWER net; ANALOG;
   sets `fb` on a buck/boost only if the divided rail is that converter's output (2bbb620).
8. `by_name_and_function`: clock names, switch names next to an inductor, fast buses (QSPI,
   SDIO, DDR/RGMII...) SE_HS, analog names.
9. `small_blocks`: LDO and MCU families by value (parts.rs:140, :148), indicator LED+resistor,
   suppressor next to a connector (`connector_protection`).
10. `current::estimate` (current.rs:26): POWER/POWER_HI/HV rails joined through beads, fuses,
   resistors <= 1 ohm, diodes, transistors and non-converter inductors form one group; a group's
   current = typical loads by part kind (parts.rs:285: MCU 50 mA, Wi-Fi SoC 350 mA, IC 10 mA,
   LED 10 mA, addressable LED 40 mA, FPGA core by family) + off-board connector loads
   (current.rs:243) + the input draw of regulators it feeds (switching: `out * vo / (vi * 0.85)`
   when both voltages are named, else equal), capped by the lowest fuse/regulator/USB rating.
   A group with no recognised load gets its source's rating, confidence 0.2; otherwise 0.35. The
   switch node carries the inductor side's current; grounds get the sum of the root rails' drawn
   current at 0.3.

Output: `IntentFile` with `sources.recogniser`, nets with `class` + `class_provenance`, soft
`voltage`/`impedance`/`current`/`bandwidth`/`safety` (weight 0.5), blocks with provenance.
`Report` counts parts per `Kind`, nets, named nets, classes, blocks, disputes.

Data: `ccomp recognise corpus DIR --out OUT` writes `OUT/<id>/{intent,report}.json` and
`OUT/summary.json`. The PCBench run lives at `ultraroute-lab/pcbench/intent/` (1,182 boards,
51,601 nets, 28,605 named, 7,324 classified, 0 failed; summary dated 2026-11-06). The hand check
lives at `ultraroute-lab/pcbench/handcheck/` (`GUIDE.md`, 50 `<id>/{sheet.md,truth.json,
comparison.json}`, `strata.json`), driven by `tools/handcheck.py`. Corpus-derived data never
enters a repository (licences).

Invariants:

- Everything written is soft, `recognised`, with confidence in [0,1] and non-empty evidence
  (asserted in `recognises_a_small_board`). Never read copper (tracks, pours, widths): designer
  copper is what the router is benchmarked against (current.rs:1-2; handcheck GUIDE).
- A role whose part is not found is left out, never guessed (lib.rs:519).

## Solid

All 9 tests pass (2026-11-12, `cargo test -p recognise`); all are self-contained (no files):

- `tests::recognises_a_small_board`: buck with catch diode and divider, crystal, USB pair, MCU,
  indicator: classes, buck roles `sw`/`fb`/`vin`, block membership, currents (+5V, VIN, switch
  node, GND all 0.06 A), every guess soft/recognised/with evidence.
- `tests::recognised_intent_has_no_hard_or_provenance_errors`: output passes the validator's
  provenance, pad, part and net checks.
- `tests::boost_roles_take_the_output_past_the_diode` (76f1042).
- `tests::connectors_off_the_board_are_loads`: servo + switched motor + Raspberry Pi on +5V
  capped at the AMS1117's 1 A; the barrel jack is the source; the LDO passes 1 A to VIN (415e568).
- `tests::compare_counts_agreement`.
- `names::tests::{rails, pairs_and_names}`, `parts::tests::{values, kinds}`: the name and part
  tables, including `QFN-60_ThermalVias` being an IC, `600R@100MHz` a bead, `8.2nH` an RF part,
  inductor size classes.

Measured precision (hand check, 50 boards, at 415e568; aggregated read-only from the
`comparison.json` files of 2026-11-07, unweighted by strata): GROUND 48 agree / 1 wrong
(precision 0.98, recall 0.84), POWER 132/12 (0.92, 0.68), SE_HS 56/2 (0.97, 0.45), DIFF_HS 44/8
(0.85, 0.34), ANALOG 32/5 (0.86, 0.09), CLOCK 24/6 (0.80, 0.47), SWITCH 18/3 (0.86, 0.60). Blocks
(recognised, hand, recognised overlapping a hand block): buck 20/17/16, ldo 18/28/18,
mcu 14/21/14, crystal 12/17/12, indicator 57/91/57, usb 8/8/4, connector_protection 6/13/3,
boost 1/1/1. On easy2350 and rp2350-battery, from the board file alone, no class wrong against
the hand-written intent (af5894b). Whole corpus in 10 s (af5894b).

## Known wrong or weak

- **Currents are rough.** Hand check: continuous within 2x (or 20 mA) on 59 of 171 nets at
  debf1c1 ("mostly low", spec decisions log), 66 of 177 after 415e568; our peak covers the hand
  peak on 85 of 168 (was 67 of 162). In the 415e568 comparisons, of the 111 nets outside 2x,
  50 are low and 61 high. This is why nothing recognised may set a hard limit (see compile.md).
- **`peak_a` is not a peak.** current.rs:197 puts the bounding source's rating (fuse, regulator,
  USB-C 3 A / USB 2.0 0.5 A, inductor size class) in `peak_a`: "the most this rail could
  deliver", not a load's worst case. The schema doesn't distinguish; `compile::widths` does, by
  provenance kind.
- **Classes never produced:** GATE, SWITCH_HI, RF, POWER_HI, ANALOG_DIFF (recall 0 on 26, 12, 54,
  4, 17 hand nets). HV only from a name voltage above 50 V (0 of 56 hand HV nets found). ANALOG
  recall 0.09.
- **Blocks never produced:** half_bridge, memory, sensor, level_shifter, led_chain, button,
  connector, test_point (hand check had 269 connectors, 51 test points, 16 level shifters).
- **Buck roles are incomplete for `verify`.** `verify::buck::BuckRoles::from_map`
  (crates/verify/src/buck.rs:56) requires `vin, sw, pgnd, fb, cin, cout, inductor`; the
  recogniser omits `vin`/`cin` when no IC is on the switch node (discrete or controller+FET),
  omits `fb` without a divider, and never writes `diode`, `rfb_top`, `rfb_bot`, `cff`. An async
  buck's catch diode is put in `parts` but not in `roles.diode`, so a hot loop measured from these
  roles would take the synchronous path through `pgnd`. Relevant because the spec has the router
  call `recognise` for roles on a board with no intent.
- Role picking is first-found: `sw` is the IC's first pad on the switch net; `pgnd` is the IC's
  pad on whichever ground comes last in `BTreeSet` order (lib.rs:494-498); `vin` is the first
  other decoupled POWER-or-unclassed net of the IC in name order (lib.rs:501), which could be an
  IC's internal-regulator output pin (unverified on real boards).
- Heuristics with known edges (by reading): SOT-23 pin 1 is assumed to be the gate/base
  (current.rs:76); an `L` part whose value ends in `R` is a bead (an inductor written `4R7` would
  be misread); prefix `M` is an IC, `B` a switch; `pair_kind` calls anything with `TD`/`RD`/`PHY`
  Ethernet; `names::fast_bus` checks `n.contains("QSPI")` twice (names.rs:161; the second was
  probably meant to be something else); a net named for a voltage with no caps is dropped only if
  it contains `_` and does not start with `+`, `-` or `V`.
- `VERSION` is `"0"` (lib.rs:21) although commits call the current one v0.1; corpus summaries say
  `v0`.
- Recognised intent does not pass the validator in full: classes require fields (e.g. SWITCH needs
  voltage and current) the recogniser may not fill; only provenance/reference errors are tested
  absent.
- Component `fields` (MPN, LCSC) are ignored even when a schematic netlist has them; the spec's
  milestone 6 says "part numbers where present".
- The System Spec's tool table says the recogniser reads the package library; it does not.
- Block ids are `<type>_<anchor>` with the anchor being the inductor for bucks/boosts, the crystal,
  the LED, the IC for LDO/MCU, the connector for usb/connector_protection (lib.rs:179). This is
  not the KiCad group naming of `apply_layout.py` (`<type>_<IC>`); see intent.md.

## Not covered by tests

- `netlist_from_board` (the corpus path) has no unit test; it is exercised only by
  `ccomp recognise board|corpus` and the hand check.
- `grounds` fallback by capacitor count; `rails_by_name`'s skip rule for `LED_5V_EN`-style
  names; HV classing and `safety`; oscillator nets; non-USB pairs (Ethernet, CAN, LVDS, HDMI);
  `rails_by_structure`; `by_name_and_function` (pin functions, fast buses, analog names on
  nets); `ldo_sides` ranking; `connector_protection` blocks; disputes.
- In `current.rs`: fuse caps, USB connector caps, rail joining through beads/diodes/transistors
  (including the 3-rail P-FET gate rule), the no-load fallback at 0.2, FPGA core figures,
  stepper modules, `offboard_loads` source choice for host headers and the Pi-with-own-power-entry
  case, switching-regulator input draw with named voltages (`/ 0.85`), regulator loops.
- `compare`'s block and current outputs.
- No regression test pins the hand-check numbers; they are reproduced only by
  `tools/handcheck.py score` with the corpus present.

## Lessons to keep

- **Never estimate from copper.** A corpus board's track widths are the designer's answer, and the
  router is scored against them (current.rs:1-2). The hand check is blind for the same reason:
  sheets show parts and pads only, no guesses, no copper (tools/handcheck.py docstring; GUIDE.md).
- **Hand check method** (76f1042): 50-board stratified sample (`pcbench/intent/sample50.json`),
  blind sheets, hand `truth.json` per board, scored by `ccomp recognise board --compare` and
  weighted back to the corpus by stratum (`strata.json`). "Continuous within 2x or 20 mA" and
  "our peak covers 0.8 of the hand peak" are the current criteria (handcheck.py:152-169).
- **Recognizer before more hand-annotated boards**: four related boards can't show the schema
  generalises; the corpus run can (spec decisions log).
- A rail with no recognised load gets its source's rating as a low-confidence (0.2) upper bound,
  and that bound is not pushed upstream (debf1c1; `Est.drawn` vs `amps`, current.rs:314-324).
  `compile` skips recognised currents under 0.35, which is exactly this case.
- A microcontroller that is the IC of its own core regulator (RP2350) is still a load on every
  rail it touches (current.rs:40-48).
- Off-board loads (415e568): a rail's connectors count as loads when they carry a ground pin or
  are switched by a transistor: stepper module 0.4 A, switched motor/solenoid 0.5 A, named hobby
  servo 0.2 A, Raspberry Pi 0.5 A on its 5 V pins (2 and 4), expansion header 0.1 A, small sensor
  header 10 mA. On an unfed rail the power-entry connector, else the host header (20+ pins),
  else the fewest-pin connector is the source. That change moved continuous-within-2x from 59/171
  to 66/177.
- Class-D output filters are not bucks: an LC into a connector and nothing else, on an unnamed
  rail, is an output stage (debf1c1; lib.rs:448-454).
- A boost's inductor side is its input; `cout` and `vout` come from past the diode (76f1042).
- A divider sets a converter's `fb` only when the divided rail is that converter's output, or a
  battery-sense divider into an MCU with an on-chip buck steals it (2bbb620).
- A designator of a real part beats a footprint name (`QFN-60_ThermalVias` is an IC); `G` is
  KiCad's graphic prefix; `REF**` is an unnamed footprint (parts.rs:39-46).
- A pin with several functions (`GPIO26/ADC0`) says what it can do, not what it does; ignored
  (lib.rs:608).

## For the move

- Lands as `crates/recognise` almost unchanged. Swap `pcb::Board` in `netlist_from_board` for
  Boardroom's `board` (the `pcb` module keeps its API, per the spec, so at first only the path
  changes), and `intent::netlist::{Netlist, Net, Node, Component}` for wherever the netlist type
  lands (the spec moves Composer's netlist reader to `kicad`).
- Add `group: None` to the `Block` literal at lib.rs:182 when `Block.group` arrives.
- The spec gives it three uses: draft intent for a new design, give the router roles on a bare
  board, and read a designer's finished board for the scoreboard. For the second, first make the
  buck/boost roles complete enough for `examine`'s hot-loop measure (write `diode`; find `vin`
  for controller+FET bucks; fall back gracefully when `fb` is absent), and test that with the
  `verify` buck roles parser. For the third, reading copper is then allowed but must stay a
  separate function from the copper-blind drafting, so the benchmark's labels stay uncontaminated.
- Tests read no files, so no paths change. `tools/handcheck.py` goes to `bench/`; its
  `--pcbench` default (`~/Software/ultraroute-lab/pcbench`) and `CCOMP` path must become the
  spec's configured, fail-loudly corpus path and the `boardroom` binary.
- `ccomp recognise board|corpus` becomes `boardroom compose recognise …` or a top-level
  `boardroom recognise`, as the command layout decides.
- Differently: put the role vocabulary per block type in one place shared with `intent::presets`
  and `examine`; add a small fixture test per untested pass (each is a 5-10 net netlist like the
  existing ones); make `peak_a`'s meaning explicit (a separate `source_limit_a`, or a note in the
  provenance) instead of overloading the schema field.
