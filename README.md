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

Milestone 1, **intent schema v0**, is done: Rust types, a generated JSON Schema, a validator, and
one real board annotated by hand (`boards/easy2350/`) that passes it.

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

Every command prints one JSON result. Exit status: 0 success, 1 the input failed validation,
2 the command could not run.

## Layout

| Path | What |
|---|---|
| `crates/intent/` | The schema types (`model.rs`), class presets and block types (`presets.rs`), the KiCad netlist reader, the validator, the easy_pcb importer |
| `crates/ccomp/` | The command-line harness |
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

## Next

- **Milestone 2:** one buck regulator end to end. The RP2350 core regulator (`core_reg` in
  easy2350) is the first candidate. Raspberry Pi's layout is the vendor reference to score
  against, and fast-tier verifier checks (hot-loop area, switch-node area, feedback clearance)
  run on KiCad geometry.
- **Milestone 3:** the constraint compiler. ultraroute already reads `.kicad_pro` net classes,
  `.kicad_dru` minimum widths and rule areas (as via-avoid areas), so the first output needs no
  router change. Per-class cost grids need a reader on the router side.

## Findings for the sibling projects

- easy_pcb's `nets.csv` for easy2350 classifies 61 of the 65 connected nets. KiCad's auto-named
  two-pin nets (`Net-(D1-A)`, `Net-(R7-Pad2)`, `Net-(R8-Pad2)`, `Net-(U4-Y)`) are missing, and
  `nets_check.py` does not flag them. The intent here covers them by hand.
