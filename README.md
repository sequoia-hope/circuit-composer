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
  `packages/rp2350` 0.1.0 (not yet pushed to GitHub).

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

Every command prints one JSON result. Exit status: 0 success, 1 the input failed validation,
2 the command could not run.

## Layout

| Path | What |
|---|---|
| `crates/intent/` | The schema types (`model.rs`), class presets and block types (`presets.rs`), the KiCad netlist reader, the validator, the easy_pcb importer |
| `crates/ccomp/` | The command-line harness |
| `crates/pcb/` | A KiCad board as copper per net (read with ultraroute's `kicad-pcb`, pinned), geometry, and copper-path search |
| `crates/verify/` | Fast-tier verifier checks (`buck.rs`) and SVG drawings of what they measured |
| `crates/blockgen/` | Block generators (`buck.rs`) and the layout format, in the anchor part's frame |
| `tools/` | pcbnew-side steps: apply a layout, the test bench, comparisons |
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

## Next

- **Milestone 3:** the constraint compiler. Agreed with ultraroute: cost grids are regions, not
  rasters. The compiler writes a `routing.json` (polygons with per-layer `must`, `prefer`,
  `avoid` and `never` rules by class or net, plus `max_length` entries) that the router reads
  with `--regions`. Locked copper and parts are already left alone. The first deliverable is a
  sample `routing.json` for easy2350.
- **Package:** a 2-layer layout, and the transplant tool used in context on a real board, not
  only on a test bench. Publish `packages/rp2350` on GitHub.
- Then mine `~/pcb/rp2350-motor-controller` for further blocks.

## Findings for the sibling projects

- easy_pcb's `nets.csv` for easy2350 classified 61 of the 65 connected nets. KiCad's auto-named
  two-pin nets were missing. Fixed on the easy_pcb side: `nets_check.py` now fails such nets.

## License

MIT, see `LICENSE`. Packages are MIT too. They cite vendor documents by URL and hash and do not
include them. `compare/reference.*` are this project's measurements and drawings of Raspberry Pi's
published minimal-board layout, kept for comparison.
