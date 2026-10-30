# circuit-composer

Board design as composition: chips are ingested once into verified, published packages, and
boards are built by combining them. It covers the stage between schematic and routing:
constraint definition and physical design planning. The full design is in
[`Circuit Composer — System Spec.md`](Circuit%20Composer%20—%20System%20Spec.md).

It sits between two sibling projects:

- **[easy_pcb](../easy_pcb)** runs a board from requirements to fab outputs with the
  `pcb-design` skill. Its phase-3 classification (`design/nets.csv`) imports into an intent file,
  and its worked example, easy2350, is the first annotated board here.
- **[ultraroute](../ultraroute)** is the router. It takes this project's output: a KiCad board
  with critical blocks placed, routed and locked, plus rules and cost grids.

## Status

- **Milestone 1, intent schema v0:** done. Rust types, a generated JSON Schema, a validator,
  and one real board annotated by hand (`boards/easy2350/`) that passes it.
- **Milestone 2, one buck end to end:** done, for the RP2350's core regulator, an on-chip buck
  with an external inductor and capacitors. That covers a generator, fast-tier verifier checks,
  a comparison against Raspberry Pi's reference layout, and the first package,
  `packages/rp2350` 0.1.0, published in this repository.
- **Milestone 3, router integration:** done on our side. The constraint compiler, the region
  check and a routed-board seam check are in place. easy2350, with its blocks locked, was routed
  by ultraroute and passes every check (`boards/easy2350/routed/`), with and without
  ultraroute's `--regions` reading the compiled regions.
- **Milestone 4, RP2350 package:** `packages/rp2350` 0.2.0. Core-regulator layouts for QFN-60
  and QFN-80 on 2 and 4 layers, all beating Raspberry Pi's on the same checker. Crystal, USB,
  QSPI flash, buttons and decoupling as intent blocks, mined from the design guide and both
  minimal boards. Pin tables for both variants, 27 cited rules (disputes kept), and the support
  parts by role. `ccomp check package` judges any board against the package by role. Both
  reference boards pass every hard check. easy2350 fails one: its USB pair runs over the
  +3V3 plane, not ground.
- **Milestone 5, floorplanner and composer:** in progress.
  - **Composer:** `ccomp compose` builds a board's circuit from package instances, the board's
    own parts and the nets joining them, with a binding per instance for `ccomp check package`.
    Recomposing the RP2350A minimal board (55 nets) and easy2350 (65) reproduces their netlists
    exactly.
  - **Packages:** three more, each ingested from its datasheet with cited rules: `mcp73831`
    (LiPo charger), `tlv62569` (2 A buck, with a buck intent block) and `lsm6dso` (IMU).
  - **KiCad output:** `tools/compose_kicad.py` writes the schematic (a box per block, a net
    label per pin) and the board through easy_pcb's writer. ERC is clean, and the board's pad
    nets equal the composed nets.
  - **Floorplanner:** `ccomp floorplan` places every part by simulated annealing, using
    pad-to-pad wirelength, courtyard overlap, board area, block cohesion, decoupling hints,
    rigid generated layouts, connectors flush on an edge facing out, through-hole pads that
    block both sides, and separations taken from the packages' rules.
    - In easy2350's own outline it finds a legal placement with 10 % less wirelength than the
      person's (790 mm against 878; KiCad DRC: no courtyard, clearance or short findings).
    - The demo board is in `boards/rp2350-battery/`: USB-C, a charger, load sharing, a buck,
      the RP2350 and an IMU. It composes and floorplans legally at 39 × 31 mm.
  - **Not yet:** design rules and stack-up for composed boards; routing them; package
    schematic sheets; the buck generator for off-chip regulators.
- **Research library:** `library/` holds 143 cited claims from 41 vendor app notes, datasheets
  and articles, on five topics:
  - buck layout,
  - half-bridges and motor drivers,
  - crystal oscillators,
  - USB, return paths and decoupling,
  - CAN, connector protection and trace current.

  It is one database with the packages' rules. 17 disputes are kept as linked pairs, one of
  them against an rp2350 rule. Every quote is checked against its document
  (`library/README.md`).

## The app (prototype)

`app/` is Circuit Composer's own UI, specified in [`docs/app-spec.md`](docs/app-spec.md): a Rust
core compiled to WASM (`app/core`, the floorplanner and the board document) and a Svelte page
(`app/web`). It opens a board's floorplan; you drag, rotate and lock parts, and the floorplanner
anneals live in a worker in the page, from scratch or refining what is there. The Circuit
workspace shows the board as blocks and the nets between them, and Shape embeds the bolt
earring's outline shaper. It runs with no server: it opens the example boards that
`app/examples.sh` exports from `boards/` into the site. That is also how it runs on GitHub Pages.
Boards are drawn through a placeholder until `board-draw`, the renderer shared with UltraRoute
Studio, lands. AI fills, the terminal and the handoff to UltraRoute need the app's server, which
is next.

```sh
app/examples.sh     # export boards/ as the app's example boards (committed)
app/build.sh        # core to WASM, page to app/dist/
./serve.sh          # then open /app/dist/ on the project's registered port
```

URL parameters: `board=NAME`, `ws=circuit|board|shape|copper|rules`, `embed=1` (no header, for
Boardroom's single-window mode; selection is posted to the parent window).

## Build and run

```sh
cargo build --release
ccomp=target/release/ccomp

$ccomp schema -o schema/intent.schema.json             # regenerate the JSON Schema
$ccomp classes                                         # net class presets and block types
$ccomp netlist path/to/board.kicad_sch                 # the netlist as JSON (runs kicad-cli)
$ccomp validate boards/easy2350/intent.json            # pass/fail, findings per net and block
$ccomp resolve boards/easy2350/intent.json             # nets with preset defaults filled in
$ccomp import easy-pcb ../easy_pcb/examples/easy2350/design/nets.csv \
    --board easy2350 --netlist ../easy_pcb/examples/easy2350/hw/easy2350/a/easy2350.kicad_sch \
    -o draft.json                                      # draft intent from easy_pcb
```

Any command can work on a board in a project instead of being given every path:
`--project DIR` (`-C`) and `--board NAME` (`-b`) fill in the inputs it was not given from the
board's files. Boards live under `compose/<board>/` in an easy_pcb project and under
`boards/<board>/` here, each optionally with a `board.json` naming where its files are
(`crates/ccomp/src/project.rs`). Outputs are still written only with `-o`.

```sh
$ccomp boards                                          # this repository's boards
$ccomp -b rp2350-battery boards                        # one board's files, and which exist
$ccomp -b rp2350-battery compose                       # = compose boards/rp2350-battery/compose.json
$ccomp -C ../my-project floorplan --seed 2             # a project's only board
```

Milestone 2's tools:

```sh
# Fast-tier checks on a buck block, with a picture of what was measured
$ccomp check buck board.kicad_pcb --roles vin=U1.49,sw=U1.48,pgnd=U1.47,fb=U1.50,cin=C6,cout=C7,inductor=L1 --svg block.svg
$ccomp check buck board.kicad_pcb --intent boards/easy2350/intent.json --block core_reg

# Generate the block (parts and copper in the IC's frame); --defaults prints the parameters
$ccomp gen buck board.kicad_pcb --intent boards/easy2350/intent.json --block core_reg -o gen.json

# Generate, apply to a test bench with pcbnew, verify, and run KiCad's DRC
~/.claude/bin/reserve -c 8 -l "buck bench" -- python3 tools/bench_buck.py BOARD ROLES.json OUT_DIR
python3 tools/apply_layout.py BOARD gen.json ROLES.json -o OUT.kicad_pcb [--fixture]
python3 tools/compare_buck.py packages/rp2350/layouts/qfn60-4layer/compare.spec.json OUT_DIR
```

Milestone 4 adds net-level measurements, for interface blocks:

```sh
# Pad-to-pad copper paths, track with no ground beside it, nearest decoupling per power pin
$ccomp measure board.kicad_pcb --spec packages/rp2350/reference/rp2350a_minimal/measure.spec.json
python3 tools/kicad_pins.py board.net U1 --rename SWD=SWDIO   # a part's pin table from a netlist

# A board against a package, by role: wiring, block constraints, length limits, part values
$ccomp check package board.kicad_pcb --package packages/rp2350 --binding boards/easy2350/rp2350.binding.json
```

Composing and floorplanning (milestone 5):

```sh
python3 tools/circuit_check.py packages/*/schematic/circuit.*.json --kicad 9   # symbols, footprints, pins
$ccomp compose boards/rp2350-battery/compose.json -o composed.json
python3 tools/compose_kicad.py composed.json OUT_DIR                           # schematic, ERC, board
$ccomp floorplan OUT_DIR/BOARD.kicad_pcb --composed composed.json --spec boards/rp2350-battery/floorplan.json -o placement.json
$ccomp floorplan ... --evaluate PLACED.kicad_pcb                               # score a person's placement
python3 tools/apply_placement.py OUT_DIR/BOARD.kicad_pcb placement.json -o placed.kicad_pcb   # + KiCad DRC
```

Recomposing an existing board:

```sh
$ccomp compose boards/easy2350/compose.json -o composed.json --bindings OUT_DIR   # circuit + bindings
python3 tools/package_circuit.py packages/rp2350 --board qfn60 --variant RP2350A --netlist REF.net --chip U1 --source rp2350a_minimal -o packages/rp2350/schematic/circuit.RP2350A.json
python3 tools/compose_from_netlist.py draft BOARD.net --instance mcu rp2350@0.2.0 RP2350A BINDING.json --keep-designators -o compose.json
python3 tools/compose_from_netlist.py compare BOARD.net composed.json --binding mcu BINDING.json
```

The research library:

```sh
$ccomp library check                                   # ids, sources, links, both sides of each dispute
$ccomp library query --scope circuit=buck --status contested
python3 tools/library_fetch.py && python3 tools/library_quotes.py   # documents, hashes, quotes
```

Every command prints one JSON result. Exit status: 0 success, 1 the input failed validation,
2 the command could not run.

## Layout

| Path | What |
|---|---|
| `crates/intent/` | The schema types (`model.rs`), class presets and block types (`presets.rs`), the KiCad netlist reader, the validator, the easy_pcb importer |
| `crates/ccomp/` | The command-line harness |
| `crates/pcb/` | A KiCad board as copper per net (read with ultraroute's `kicad-pcb`, pinned), geometry, and copper-path search |
| `crates/verify/` | Fast-tier verifier checks (`buck.rs`, `nets.rs`) and SVG drawings of what they measured |
| `crates/floorplan/` | The floorplanner: simulated annealing over part poses, rigid groups, edges, separations |
| `crates/compose/` | The composer: package instances, the board's own parts and nets into one circuit, with a binding per instance |
| `crates/library/` | The research library: cited claims from `library/` and every package's `rules.json`, checked and queried as one database |
| `crates/blockgen/` | Block generators (`buck.rs`) and the layout format, in the anchor part's frame |
| `tools/` | pcbnew-side steps: apply a layout, the test bench, comparisons |
| `library/` | Claims by circuit class, their sources (URLs and hashes; documents cached, not committed) |
| `packages/<chip>/` | Published packages: manifest, intent, cited rules, layouts with verify reports |
| `schema/intent.schema.json` | Generated; a test fails if it is stale |
| `boards/<board>/` | One annotated board each: `intent.json`, `input/` (netlist snapshot), `validate.json` |
| `.claude/skills/circuit-composer/` | The Claude Code skill that drives `ccomp` |

## The intent file (v0)

A JSON sidecar keyed by KiCad net names (`/GPIO0`, `+3V3`) and reference designators. Every
constrained value is `{value, strength: hard|soft, weight (soft only), provenance}`, and a
provenance cites a key of `sources` with a locator (page, section, timestamp) and a kind:
`datasheet`, `expert`, `simulated`, `requirement` (the board's own budget), or `preset`.

Net classes are presets over attributes: a class names the attributes a net must state (`SWITCH`
needs `voltage` and `current`) and supplies soft defaults for the rest (aggressor `strong`). A
default never satisfies a requirement and can never be hard.

The validator refuses a board until every connected net has intent with its class's required
fields, every block has a library type and owns real parts (no part in two blocks), every hard
constraint (including `must`/`never` region rules and protection paths) has a provenance that
resolves to a source, protection-path pads lie on their net, and differential pairs name each
other.

## Decisions

| Decision | Why |
|---|---|
| Intent file is JSON (spec open question) | The schema is generated as JSON Schema from the Rust types; packages already use `intent.json`; serde refuses unknown fields, so typos fail loudly |
| `requirement` and `preset` provenance kinds, beyond the spec's datasheet/expert/simulated | Board budgets (500 mA USB) are the designer's claims, not a datasheet's; presets must be visibly weaker than evidence |
| `max_length` net attribute, beyond the spec's table | easy_pcb's `length_limit` is enforced today and the router needs it |
| Regions may be anchored to a part | Package rules ("cut layer 2 under the inductor's LX pad") are stated relative to parts, before placement exists |
| Boards keep a netlist snapshot in `input/` | Validation is reproducible without the sibling checkout, as ultraroute's `jobs/<board>/input/` |

## Milestone 2: the RP2350 core regulator

**Checks** (`crates/verify/src/buck.rs`, about 1 s on a whole board). Copper paths are found
through tracks, vias and zone fills. Loop areas integrate |winding number|, so a loop that
crosses itself counts both lobes; a plain shoelace sum let them cancel and under-reported the
reference's output loop by half.

| Check | What |
|---|---|
| `hot_loop_area_mm2` | VIN pin → CIN → PGND pin along the copper |
| `output_loop_area_mm2` | SW pin → inductor → COUT → PGND pin |
| `switch_node_area_mm2` | All switch-net copper, overlaps counted once |
| `feedback_clearance_mm` | Feedback path (FB pin to COUT) to switch-node copper, IC pins excluded |
| `switch_node_underlap_mm2` | Other nets' copper on the layer under the switch node |

**Generator** (`crates/blockgen/src/buck.rs`). It places CIN, the inductor and COUT on a
0.1 mm grid in four rotations, by beam search:

1. CIN, kept only where the switch pin can still escape.
2. The inductor, with a lookahead on the output loop.
3. COUT.
4. The feedback run (straight, or one bend).
5. Ground vias, kept clear of a generated In1.Cu keepout under the switch node.

Runs escape each pin past its tip and neck to pin width before widening. Clearances come from
the board's own net classes. No copper may cross under the inductor except on its own pads.
The output is in the IC's frame (`layout.json`), so it transplants onto any board with the
same footprint.

**Result** on a 4-layer test bench (`packages/rp2350/layouts/qfn60-4layer/`): KiCad DRC clean,
30–80 s on 8 shared cores, and deterministic (a rerun reproduces `layout.json` byte for byte). Every row below is measured by the same checker:

| | Hot loop | Switch node | Output loop | FB clearance | Score* |
|---|---|---|---|---|---|
| Raspberry Pi reference (2-layer) | 1.064 | 2.353 | 2.452 | 0.365 | 3.47 |
| Reference transplanted to 4 layers (easy2350) | 1.064 | 2.301 | 2.452 | 0.386 | 3.44 |
| Generated | 1.025 | 1.904 | 2.343 | 0.415 | 3.15 |

\*hot + 0.5 · switch node + 0.5 · output loop, mm². With no prior given, the generator puts
CIN and COUT where Raspberry Pi did, both on x = 2.0 mm in the IC's frame. The inductor sits
0.4 mm further in, so its LX pad lands right over the pin-48 run. Caveat: the reference
uses copper zones; the generator draws tracks (LX 0.2 mm wide, against the reference's 0.3 mm
strip). Some of the smaller switch-node area is narrower copper, and these proxies do not yet
price trace resistance or inductance.

## Milestone 3: the constraint compiler

Agreed with ultraroute: cost grids are regions, not rasters. `ccomp compile` writes:

- **`routing.json`**, which the router reads with `--regions`. It holds polygons in board
  millimetres, each with per-layer rules on lists of nets (classes are expanded from the intent
  file), and hard `max_lengths`. Part-anchored regions are placed on their part. Intent weights
  in (0, 1] become the router's extra-length multipliers, × 10.
- **A `.kicad_dru`**: the board's own rules kept, plus a marked block of `length` rules, so
  KiCad's DRC checks the same limits. A rerun replaces the block only.

Region rules use the router's meanings: `must` (these nets' copper may not leave the region on
its layers), `never`, `avoid` and `prefer` (prices by `weight`). A rule may target the nets'
vias instead of their tracks.

`ccomp check regions BOARD --routing routing.json` lists copper that breaks a rule. Its tests
plant every kind of violation on the package bench. `tools/verify_routed.py` adds KiCad's DRC
(unconnected items, errors, length limits) and the buck metrics. Together they check a routed
board's seams.

On easy2350 (inputs pinned to easy_pcb 2d56198 in `boards/easy2350/input/`): 3 regions,
12 rules and 17 length limits. easy_pcb's released route passes all of it.

**Routed by ultraroute** (ad79cd7, without `--regions` for now; see `boards/easy2350/routed/`):
0 DRC errors and 0 unconnected after a refill. All 17 lengths and 12 region rules hold, and all
232 locked items, the regulator block among them, come back unchanged.

## Mining: rp2350-motor-controller

The fabricated rev A of the motor controller (`boards/rp2350-motor-controller/`), measured with
the same checks. Its RP2350B (QFN-80) regulator matches Raspberry Pi's layout within 2 %, but
sits over a solid In1.Cu ground plane, which the datasheet asks to be cut away. That is recorded
as a datasheet-versus-practice dispute. Its XL7005A 60 V buck has a hot loop 25× the RP2350's.
Mining it added asynchronous bucks (a `diode` role), divider feedback, and plane shielding to
the checks.

## Next

- Rerun the routed check with `--regions` once ultraroute has it.
- Milestone 4: the RP2350 package's QFN-80 variant, using the motor controller's regulator as
  its measured reference.
- The asynchronous buck in the generator, then the half-bridge (milestone 7).
- **Package:** a 2-layer layout, and the transplant tool used in context on a real board, not
  only on a test bench.
- Then mine `~/pcb/rp2350-motor-controller` for further blocks.

## Findings for the sibling projects

- easy_pcb's `nets.csv` for easy2350 classified 61 of the 65 connected nets. KiCad's auto-named
  two-pin nets were missing. Fixed on the easy_pcb side: `nets_check.py` now fails such nets.

## License

MIT, see `LICENSE`. Packages are MIT too. They cite vendor documents by URL and hash and do not
include them. `compare/reference.*` are this project's measurements and drawings of Raspberry Pi's
published minimal-board layout, kept for comparison.
