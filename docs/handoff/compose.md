# compose → boardroom crates/compose

The composer. It builds one flat circuit from a **composition**: package instances (each package's
reference circuit, `packages/NAME/schematic/circuit.VARIANT.json`), **recipe** instances (small
parametric circuits from `library/recipes/`), the board's own parts, and the nets that join them.
Out come parts by designator, nets by pin (`U3.4`), the departures from each package (taps, joins,
added pins), and per package instance a **binding** (package role → board designator) that
`ccomp check package` reads. It also assembles the composed board's `intent.json` from the
packages' intent files (`board_intent`), and holds the arithmetic (`expr`) the recipes and the pin
solver's level rules evaluate. Size: 1,965 lines of `src` (lib 602, recipe 584, board_intent 532,
expr 247, unit tests included) and 55 lines in `tests/`. Depends on `serde`, `serde_json` and
Composer's `intent` crate. Used by `pinsolve`, `ccomp` (`compose`, `pins`, `check supplies`,
`recipe`, `library check`, `floorplan` for `kicad_net_name`) and `app/core` (the WASM core:
`pins_solve`, `compose`, `recipe_expand`).

## Model in five minutes

Types (`src/lib.rs`):
- `Composition` (:27): `board`, `instances` (name → `Instance`), `parts` (the board's own, by
  designator), `nets` (net → endpoints `INSTANCE.PORT` or `REF.PIN`), `no_connect`, `connect`
  (loose connections for `pinsolve`; compose refuses them), `rails` and `sources` (read by
  `pinsolve`), `intent` (an `board_intent::Overlay`). Written by hand as `compose.json`, or as
  `loose.json` (with `connect`) for the pin solver.
- `Instance` (:59): `package` = `name@version` (must equal `version = "…"` in the package's
  `manifest.toml`), `variant`, or `recipe` + `params`; departures: `omit` (roles not fitted),
  `parts` (per-role overrides of value/footprint/symbol/any field), `join` (internal nets made
  one), `reversed` (two-pin parts fitted 1↔2), `designators` (keep a role's designator).
- `Circuit` (:99) is the package circuit file: `variant`, `parts` (role → `CircuitPart`:
  symbol, value, footprint, `reference_ref` whose letters prefix the designator, lcsc, mpn,
  `near` placement hint), `nets` (internal, by role pin `U1.21`), `ports` (`kind` =
  `ground`/`supply`/`signal`, `pins`).
- `Composed` (:160): `parts`, `nets`, `unconnected_ports`, `taps`, `joined`, `added_pins`,
  `bindings`, `errors`, `ok`. `Binding` (:151): `package`, `variant`, `chip` (role `U1`'s
  designator), `parts`.

Entry points: `compose(c, packages_dir)` (:479) = `circuits` (:469, loads packages and expands
recipes from `library_dir(packages)` = `packages/../library`, :447) then `compose_with(c,
circuits)` (:204), which is pure and is what the WASM core calls. `package_dir` (:179) finds a
package folder and checks its version. `recipe::expand(recipe, params, stock)`
(`src/recipe.rs:308`) → `Expanded { circuit, ports (a ports.json-shaped Value), volts, chosen }`.
`board_intent::board_intent(c, circuits, composed, packages)` (`src/board_intent.rs:126`) →
`BoardIntent { intent: IntentFile, nets, unplaced, merged, defaulted, uncovered, errors }`.
`expr::eval` (`src/expr.rs:207`), `parse_value` (:7, `4k7`, `100n`, `3V3`), `format_value` (:37).

How `compose_with` names things: board parts keep their designators; each instance's parts get
`<prefix of reference_ref><n>`, numbered after every taken designator, in role order (BTreeMap
order for packages, natural order for recipes); internal nets become `/INSTANCE/NET` (KiCad's
hierarchical style); a board net naming an internal net (`mcu.DVDD`) **taps** it and takes it
over with every net joined to it; `INSTANCE.ROLE.PIN` names a pin the package leaves open
(`mcu.FLASH.9`) and is reported in `added_pins`. Errors: a pin on two nets, a ground or supply
port left unconnected, unknown ports/parts/roles, a port named twice, a kept designator taken.
Unconnected signal ports not in `no_connect` are listed, not errors.

`board_intent` maps each package's `intent.json` (written in the package's own names) to board
names: a package net is found through its pins (`schematic/nets.json` anchor first, then the
circuit's net or port), parts through the binding, blocks/regions/domains renamed
`INSTANCE/NAME`. Two package nets on one board net are merged: higher `voltage.peak_v`, higher
`current.continuous_a`, shorter `max_length`, otherwise first wins, each merge reported. Then the
composition's `intent` overlay (its fields win), then `default_net` for any net still uncovered.
Net names are KiCad's as `tools/compose_kicad.py` labels them: `kicad_net_name` (:84) turns
`/mcu/XIN` into `/mcu_XIN` and `+3V3` into `/+3V3`.

Recipes (`src/recipe.rs`): `library/recipes/NAME.json` is a circuit with `params` (defaults,
named `choices` whose fields a part takes via `from`), parts that `choose` the stocked part
nearest a `target` (log ratio) between `min` and `max` from `library/stock.json`, `for: "1..n"`
repetition with `{expr}` in names, `if` options (also `PIN if expr`, `PIN for 1..n` inside pin
lists), `requires` checks, `interfaces`, and port `power` contracts whose numbers may be
expressions (keys listed in `POWER_NUMBERS`, :147). Expansion yields one variant called `recipe`.

Invariants a change must keep: `compose_with` is deterministic and pure (the WASM core and tests
depend on byte-equal nets); designators of a recomposed board stay stable (`designators` and
board parts are taken first); a package's chip is role `U1` (bindings, `pinsolve::port_of`,
`levels`, `board_intent` all assume it); `/INSTANCE/NET` naming and `kicad_net_name` must agree
with whatever writes the KiCad schematic, or intent and board net names drift apart.

## Solid

- Unit tests, `src/lib.rs`: `two_instances_share_rails_and_keep_their_own_nets`,
  `power_must_be_connected_and_pins_not_shorted`, `taps_and_joins`,
  `a_pin_the_package_leaves_open`, `omitted_and_changed_parts` (designator numbering, overrides,
  reversed parts, kept designators).
- `src/board_intent.rs`: `package_intent_lands_on_board_names` (nets, blocks, domains, regions in
  board names; overlay; default_net), `merged_nets_keep_the_stricter_values`.
- `src/recipe.rs`: `a_resistor_from_the_parameters`, `repetition`,
  `power_contracts_take_expressions`, `options`. `src/expr.rs`: `arithmetic_names_and_suffixes`,
  `values`.
- `tests/library_recipes.rs`: every real recipe expands with defaults, cites only
  `library/sources.json` sources, every part has an LCSC number; the LED resistor, a 134-LED
  chain, the I2C pull-up window.
- Through other crates' tests (all passing 2026-11-12): `ccomp/tests/compose.rs` recomposes
  easy2350 (65 nets) and the RP2350A minimal board (55) to their netlists exactly and matches the
  hand-written binding; `pinsolve/tests/bolt.rs` composes the bolt earring (8 packages, 15
  recipes) to its netlist net for net (192 nets, 256 parts); `pinsolve/tests/battery.rs` and
  `supply.rs` compose solved boards.

## Known wrong or weak

- `package_dir` (`src/lib.rs:179`) reads the version by scanning for a line starting
  `version = "`; not a TOML parser. `pinsolve::supply::candidates` has a second copy of the same
  scan.
- The chip is assumed to be role `U1`. A package whose main part is not `U1` gets an empty
  `Binding.chip` silently (:279).
- Package roles are numbered in BTreeMap (lexical) order, so role `C10` gets a lower designator
  than `C2`. Deterministic, not natural. Recipes use natural order.
- `compose_with` indexes `circuits[name]` and panics if an instance has no circuit; `compose`
  always supplies one, a direct caller may not.
- The module doc says "two departures" and lists three.
- `board_intent` merges a shared net's `current` as the **maximum** of the packages' figures,
  not the sum. For a rail feeding two packages that undercounts; the supply rule
  (`pinsolve::supply`) does sum draws, so the two disagree.
- `board_intent` silently skips an instance whose package folder fails to resolve or has no
  `intent.json` (:152-157); only 4 of 12 packages have one (rp2350, tlv62569, mcp73831, lsm6dso),
  and recipes have none ("intent for the remaining packages and the recipes" is in README's
  Not yet). The overlay's `sources` overwrite package sources of the same id without a word.
- `board_intent` names nets as `compose_kicad.py` (Python, pcbnew) writes them. That coupling is
  a convention, not checked here.
- Recipe `choose` picks by value only: no power rating, tolerance or JLC part type preference;
  `stock.json` has one table (`resistor_0402`, 29 JLC basic/preferred values, checked
  2026-10-30).
- `" for "` and `" if "` in recipe pin strings are found by string split; a net or role name
  containing those words would break.

## Not covered by tests

- `load_circuit` error for a variant with no circuit file (rp2350 ships only
  `circuit.RP2350A.json`; an `RP2350B` instance cannot compose).
- `package_dir` version mismatch message.
- `board_intent` error paths: a diff pair's partner not on the board, a protection path or energy
  source off the board, a package polygon region with no anchor, a block role whose part is
  omitted, two packages' sources with one id and different content, an overlay net not on the
  board, an overlay block clashing with a package block.
- `expand_recipes` with an unknown recipe name; recipe `choose` with no part in range;
  unclosed `{` in a name; `supply` naming a non-port; `volts` with three values.
- `no_connect` naming an unknown port.

## Lessons to keep

- Departures from a package are stated, never inferred: `omit`, `parts`, `reversed`, `join`,
  taps, `added_pins`, kept `designators`. That is what let the bolt earring be rebuilt from 8
  packages and 15 recipes and equal its netlist net for net (5fc7fdf), and easy2350 and the
  RP2350A minimal board reproduce exactly (fc9f13b). Keep reporting them in the result.
- A recomposed board keeps its annotation (`designators`), so a composition can be checked
  against an existing board pin for pin. Tests depend on that comparison, not on names.
- `mcu.FLASH.9`: a footprint swap can bring a pad the package's circuit never mentions (the bolt's
  USON flash's exposed pad); naming it explicitly beat widening the package (f6b47a8).
- Package intent is written once in package names and mapped by pins, not by net names, so it
  survives renaming at board level (16726a8). The battery board's composed intent validated
  42 of 42 nets against its netlist.
- Recipes rather than packages for board-level blocks named by function (USB-C sink, load
  sharing, LED chain): parameters and repetition, values chosen from stocked parts (29d0e8d).
  The LED chain test (134 LEDs, a capacitor every six) is the scale case.
- The LED recipe's resistor is chosen as the stocked value whose current stays at or below the
  target (`1k` not `680` at 2 mA from 3.3 V through a red LED): a `min` bound, not nearest.

## For the move

- Lands as `crates/compose` with its name. Depends on `intent` (Understanding group), which is
  allowed. Nothing in it touches `pcb`, `kicad-pcb` or the board.
- Paths: `library_dir` hard-codes `packages/../library`. In Boardroom, `packages/` is at the root;
  the spec's layout does not place the `library/` **data** directory (only `crates/library`).
  Put it at `boardroom/library/` beside `packages/` to keep this working, or better, pass the
  library path explicitly (`ccomp pins` already derives `library/interfaces.json` the same way).
- `tests/library_recipes.rs` reads `../../library` from the crate dir: becomes the repo root's
  `library/`.
- Tests elsewhere that exercise this crate on real data read `boards/easy2350/compose.json` and
  `input/easy2350.net`, `packages/rp2350/reference/rp2350a_minimal/compose.json`, and the
  bolt-earring and rp2350-battery files listed in `pinsolve.md`.
- `ccomp compose` (`crates/ccomp/src/main.rs:796`) becomes `boardroom compose compose …` or
  similar; its body (load, compose, optional `--intent` and `--bindings` writing) should become a
  library function per the spec's "every action is a function first".
- `kicad_net_name` must follow whatever the Rust `kicad` writer does when it replaces
  `compose_kicad.py`; make the writer call this function rather than re-deriving names.
- Differently: parse `manifest.toml` with the `toml` crate once (one `Manifest` type shared with
  `pinsolve`); make the chip role explicit in the circuit file instead of assuming `U1`; sum
  currents on merged rails (or take them from the supply rule's budget); return errors instead of
  skipping packages silently in `board_intent`.
