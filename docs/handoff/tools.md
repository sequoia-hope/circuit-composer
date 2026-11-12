# tools/ → kept, bench/, replaced or gone

Circuit Composer's 24 scripts in `tools/` (Python 3 and one shell script). Each script's
docstring is its manual; read it before moving or replacing the script. Most call `ccomp`
(becoming `boardroom compose …`; see `ccomp.md`) and some import KiCad's `pcbnew` Python module or
run `kicad-cli` (KiCad 9; `pcbgolf_score.py` needs KiCad 10). Wherever a script runs `ccomp` by
path or by name, change it to the `boardroom` binary when it moves.

Why many are Python at all: pcbnew rotates, flips and writes footprints correctly, and Composer
had no KiCad writer of its own (`apply_layout.py`'s docstring says so). In Boardroom the Rust
`kicad` crate is to learn writing; until it can, the pcbnew scripts are how boards get written.
**Before a Rust writer replaces one, check its output against pcbnew's** (footprint rotation and
the back side are where hand-written writers go wrong).

## Replaced by the Rust `kicad` writer (keep them until it can do their job)

| Tool | Does | Uses |
|---|---|---|
| `apply_layout.py` | Puts a generated or lifted block layout on a board in its anchor's frame: parts placed (a `flipped` one on the other side) and locked; tracks, vias, keepouts, pours added and locked; zones refilled; all wrapped in a KiCad group named `<block type>_<anchor>` (or `--group`), replacing one of that name. `--fixture` makes a test bench of the block alone with a GND plane. | pcbnew |
| `apply_placement.py` | Applies `placement.json` (positions, sides; bottom-side parts are its `back` unless `--back` names them), writes the outline on Edge.Cuts, then runs KiCad DRC. | pcbnew, kicad-cli |
| `prepare_route.py` | Makes a placed board router-ready: stack-up, design rules, net classes from intent, planes, each rigid block applied through `apply_layout.py`, and a locked stub from each decoupling capacitor to the pin it decouples (its `near`) where one fits. | pcbnew, `ccomp compose`, `ccomp compile` |

## Kept: no Rust replacement planned

| Tool | Does | Why it stays |
|---|---|---|
| `compose_kicad.py` | composed.json → KiCad schematic, ERC, board. | Writes the **schematic** through Boardroom's own `scripts/kicad_write.py` (`--easy-pcb DIR` or `$EASY_PCB` names the checkout; in Boardroom it is in the same repo). `kicad` writes no schematics. |
| `import_altium.py` | Altium `.PcbDoc` → `.kicad_pcb` (`--refill` refills zones). Tested on an 8-layer, 176-part board; what survives and what is lost is in `boards/rp2350-flight-controllers/README.md` ("Altium .PcbDoc import"). | Uses pcbnew's Altium importer; nothing else reads `.PcbDoc`. |
| `extract_layout.py` | Lifts a regulator's reference layout out of a vendor's evaluation board, in the anchor's frame: the source of "verified layouts" for packages. | Could become Rust later (it only reads); keep until then. |
| `lift_layout.py` | Lifts a reviewed cluster with no generator (half-bridge cell, decoupler field under a BGA, bottom-side members) from a board as a rigid layout plus `roles.json`. | Same. |
| `library_fetch.py` | Downloads the research library's sources into `library/cache/` (155 MB, untracked) and checks each against its `sha256`. | The cited-library pipeline; data mining is a first-class track. |
| `library_import.py` | Imports an extraction (a topic's proposed sources and claims) into the library through `ccomp library`. | Same. |
| `library_quotes.py` | Checks each claim's quoted passage is in the cited document's text. | Same; it enforces the "every figure cited" rule. |
| `circuit_check.py` | Checks a package circuit (`schematic/circuit.*.json`) against the KiCad libraries: symbols, footprints, pins. Looks for KiCad's libraries under the Flatpak path. | Package authoring. |
| `package_circuit.py`, `package_nets.py` | A package's circuit by role, and its nets in its own frame (`ccomp measure`). | Package authoring. |
| `kicad_pins.py` | One part's pin table from a KiCad netlist (`kicad-cli`). | Package authoring. |
| `rp2350_mux.py` | The RP2350 datasheet's GPIO function table → `packages/rp2350/ports.json`. | Package authoring; a model for the next chip's. |
| `compose_from_netlist.py` | Drafts a composition from an existing board's netlist, and checks a composition against it. | Turning existing boards into compositions. |
| `verify_routed.py` | Checks a routed board against its intent and routing.json: DRC, unconnected, length and region violations, the seams between locked blocks and the router. | Becomes part of `examine` once the router's checks are in Rust; until then it is the only end-to-end verdict. Needs updating when `routing.json` goes. |

## To `bench/`

| Tool | Does |
|---|---|
| `fp_bench.py` | Floorplanner benchmark: the example boards plus Cypress's ten small PCB benchmarks (ISPD 2025), fetched by URL; books cores with `reserve`. Results in `docs/floorplan-benchmark.md`. |
| `handcheck.py` | The recogniser's hand check: blind review sheets, truth files, the score. Reads prepared corpus boards from `~/Software/ultraroute-lab/pcbench` by default. |
| `bench_buck.py` | Generates a buck block, puts it on a test bench, verifies it, runs KiCad DRC on it. |
| `compare_buck.py` | Measures one kind of block on several boards (or several blocks on one) the same way: the mining tool behind `boards/rp2350-motor-controller` and `boards/rp2350-flight-controllers`. |
| `pcbgolf_score.py` | comma.ai PCBGolf score: assembly bounding-box volume from KiCad 10's STL export + 50 per via + 5,000 per copper layer. |

## Gone

`route_board.sh`: the whole old pipeline (compose → floorplan → apply_placement → prepare_route →
compile → old ultraroute → verify_routed), described in `README.md` here. Its router flags are a
record of what the old router needed; nothing in Boardroom should copy them.
