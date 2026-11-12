# boards/ → two designs, fixtures, mined data

Circuit Composer's eight `boards/` folders, 178 MB on disk. The spec says "the real designs
become projects in their own repos; fixtures are cut down and kept under `examples/` or `bench/`".
Only two are real designs. Each folder's README (where there is one) is its manual.

| Board | Size | What it is | Goes to |
|---|---|---|---|
| `bolt-earring` | 36 MB, 75 tracked files | **Design.** 72 × 33.7 mm, 6-layer lightning-bolt LED earring: 134 WS2816C-1313 on the front, simple_earring's rev-2 circuit (vendored at 7a83b33) on the back. Routed with ultraroute 184f640, seed 2 (`routed/best.kicad_pcb`); its own `build.sh`, `scripts/validate.py`, `pcbview.toml`, `viewer/`. | Its own project repo. **Cut a fixture first** (below). |
| `flame-earring` | 40 MB, 77 tracked files | **Design.** 59 × 20.7 mm, 6-layer flame earring, 117 LEDs, the same simple_earring rev-2 circuit. Same layout as bolt-earring. | Its own project repo. No test reads it. |
| `rp2350-battery` | 1.7 MB | **Fixture and demo.** Composed board: USB-C sink, LiPo charger, load sharing, TLV62569 buck, RP2350. The one board the whole old pipeline runs on end to end (0 DRC errors, 0 region violations over 4 seeds). `loose.json` (for `ccomp pins`), `compose.json`, `composed.json`, `intent.json`, `floorplan.json`, `placement.json`, `kicad/`, `routed/`. | `examples/` |
| `rp2350-matrix` | 1.4 MB | **Fixture and demo.** rp2350-battery's power with an RP2350A driving an 8 × 8 WS2816C chain, all parts on top; `make.py` writes the composition and floorplan spec at any LED pitch. Routes clean in 27 s at 6 mm pitch (b7d66fb). ultraroute's quick demo board. | `examples/` |
| `easy2350` | 4.5 MB | **Fixture.** Composer's files for the board that also lives at Boardroom's `examples/easy2350`: `input/` (the KiCad board, rules, netlist), `intent.json`, `compose.json`, `floorplan/` (spec, composed, placed), `rp2350.binding.json`, `rp2350.check.json`, `shape.json`, `compiled/`, `routed/`. | Boardroom's `examples/easy2350/compose/easy2350/` (each mode writes its own folder), **with its own pinned board**. Boardroom's copy has none of these files, no `.net`, and its `hw/easy2350/a/easy2350.kicad_pcb` is a later re-route with only 16 of 58 parts locked (Composer's has all 58), so tests that rely on locks or the expected package-check fail would break on it. Details in `floorplan.md` and `ccomp.md`. |
| `rp2350-motor-controller` | 12 MB | **Mined data.** Results of measuring the owner's fabricated motor controller (github sequoia-hope/rp2350-motor-controller, tag `rev-A-fab`, 49338a4): bucks, half-bridges, ideal diode, protection, sensing. The board itself is not here; specs read it from `~/pcb/`. | `bench/` or a mining area. Not a project. |
| `rp2350-flight-controllers` | 67 MB (68 MB of it untracked SVG renders) | **Mined data.** Seven open RP2350/RP2354 flight controllers (OpenFC-Lite, -Mini, OpenAIO, OpenAIO-Whoop, Kolibri v0.5/v0.6, Simple-FC) and the Altium import test. CERN-OHL-S-2.0 and GPL-3.0 upstreams, so no upstream file is copied; specs read `~/pcb/flight-controllers/`. SVGs are rebuilt by `tools/compare_buck.py`. | `bench/` or a mining area. Not a project. |
| `pcbgolf` | 19 MB (all in untracked `input/`) | **Benchmark.** comma.ai's PCBGolf challenge board (245 parts, 309 nets) as a dense test; README, research notes on connectors and silicon, scored by `tools/pcbgolf_score.py`. `input/` is a clone of comma.ai/PCBGolf at d0fd5a7, not committed. | `bench/`. |

## Which tests read which board

Tests are the reason fixtures must survive the move. Paths are relative to this repo.

| Test | Reads |
|---|---|
| `crates/intent/tests/easy2350.rs` | `boards/easy2350` |
| `crates/ccomp/tests/compose.rs`, `package_check.rs` | `boards/easy2350` |
| `crates/ccomp/tests/floorplan.rs`, `project.rs` | `boards/easy2350`, `boards/rp2350-battery` |
| `crates/ccomp/tests/shape.rs` | `boards/rp2350-battery` |
| `crates/ccomp/tests/shape_interface.rs` | `boards/easy2350`, `boards/rp2350-battery`, `boards/bolt-earring` (its other `boards/<name>` paths are temporary folders the test makes) |
| `crates/pinsolve/tests/battery.rs`, `rules.rs`, `supply.rs` | `boards/rp2350-battery` |
| `crates/pinsolve/tests/bolt.rs` | `boards/bolt-earring` |
| `crates/shape/src/lib.rs`, `generators.rs` (unit tests) | `boards/bolt-earring`, `boards/easy2350` |

**bolt-earring fixture:** the tests read exactly `compose.json`, `circuit.json`, `loose.json`
(`crates/pinsolve/tests/bolt.rs`) and `shape.json` (`crates/shape/src/lib.rs:699,1074`; line
712 also reads `app/web/public/examples/bolt-earring/shape.json`). `generators.rs` only cites
`scripts/geometry.py` in comments as the source of its outline. These are small; copy them to
`examples/bolt-earring/` (or a test fixtures folder) and leave the 36 MB of build output, renders
and routed boards with the design's own repo.

`ccomp`'s board registry (`ccomp boards`, `crates/ccomp/src/project.rs`) finds boards by the
`board.json` in each folder; see `ccomp.md`.
