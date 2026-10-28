# rp2350 0.2.0

A Circuit Composer package for the Raspberry Pi RP2350 family: RP2350A (QFN-60) and RP2350B
(QFN-80). The core voltage regulator has generated, verified layouts for both packages on 2 and 4
layers. The crystal, USB, QSPI flash, buttons and decoupling are intent blocks with cited rules
and with measurements taken from Raspberry Pi's two minimal boards.

| File | What |
|---|---|
| `manifest.toml` | Family, variants and their layouts, source revisions, LCSC parts |
| `intent.json` | The blocks in the intent schema: roles, nets, objectives, constraints, ports. Part names are roles; pins are QFN-60 numbers |
| `ports.json` | Every pin of both variants by number and name, and each function's supply domain, default net class and notes (datasheet §1.2) |
| `rules.json` | 27 claims from the datasheet and design guide, each cited, with what enforces it; disputes kept as `contested` |
| `schematic/support.json` | The support parts by block and role as fitted on both minimal boards: value, footprint, designator; decoupling per pin |
| `schematic/nets.json` | The package's nets by role pin, and the pin pair each length limit is measured on (`tools/package_nets.py`) |
| `reference/*/` | `ccomp measure` specs and results, and `ccomp check package` bindings and reports, for the two minimal boards |
| `sources/sources.json` | The documents and boards, with SHA-256 hashes |
| `layouts/*/` | Core-regulator layouts and their evidence (below) |

## Variants

The two packages keep the same pin *names*; numbers differ. The regulator's pins 47–50 on QFN-60
are 62–65 on QFN-80, in the same order (PGND, LX, VIN, FB). The QFN-80 adds 18 GPIOs, four of
them ADC inputs, and two more IOVDD pins. To compose a block onto an RP2350B, translate its pins
by name through `ports.json`.

## Core regulator layouts

Each layout folder holds `layout.json` (parts and copper by role, in U1's frame; apply with
`tools/apply_layout.py`), `params.json`, the test bench `fixture.kicad_pcb`/`.kicad_pro`,
`verify.json` (fast-tier measurements and KiCad DRC) and `compare/`, which measures the reference
and the layout with the same checker. `crates/verify/tests/package_layouts.rs` re-measures every
bench on each test run.

| Layout | Bench | Hot loop mm² | Switch node mm² | Output loop mm² | FB clearance mm | Score | Reference score |
|---|---|---|---|---|---|---|---|
| `qfn60-4layer` | easy2350's stack-up | 1.025 | 1.904 | 2.343 | 0.415 | 3.15 | 3.47 (RP2350A Minimal) |
| `qfn60-2layer` | RP2350A Minimal's parts | 1.025 | 1.904 | 2.343 | 0.415 | 3.15 | 3.47 |
| `qfn80-4layer` | RP2350B Minimal's parts, 4 layers | 1.031 | 1.927 | 2.416 | 0.415 | 3.20 | 3.43 (RP2350B Minimal) |
| `qfn80-2layer` | RP2350B Minimal's parts | 1.031 | 1.927 | 2.416 | 0.415 | 3.20 | 3.43 |

Score = hot loop + 0.5 × switch node + 0.5 × output loop. The QFN-80 comparison also measures
rp2350-motor-controller rev A's regulator (3.39). It matches Raspberry Pi's layout except for a
solid In1 ground under LX, which the datasheet asks to cut away (rule `rp2350.vreg.lx_cutout`).
On 2-layer boards the layer under LX is the bottom ground, kept solid as on both minimal
boards, so the cut-out check does not apply there. The generator draws tracks: LX is 0.2 mm wide
where the references pour a 0.3 mm zone. The checks do not yet price trace resistance.

All four benches are KiCad DRC clean. The one finding, `footprint_type_mismatch` on U1, comes with
the vendor footprint: Raspberry Pi's own boards show it too. The bench reports it as a footprint
note.

## Interface blocks, measured on the minimal boards

`ccomp measure` on both boards (`reference/*/measured.json`):

| | RP2350A Minimal | RP2350B Minimal |
|---|---|---|
| XIN pin → crystal | 10.71 mm | 11.17 mm |
| XOUT pin → 1 k → crystal | 2.97 + 5.54 mm | 2.96 + 5.47 mm |
| USB pin → 27 Ω (DP / DM) | 4.13 / 3.88 mm | 3.97 / 3.72 mm |
| QSPI pin → flash (SCLK, SD0–SD3) | 8.7–16.7 mm | 7.2–17.3 mm |
| Vias on crystal, USB, QSPI | 0 | 0 |
| USB, connector side, track with no ground beside it | 0 mm | 0 mm |
| USB, chip side, same (DP / DM) | 1.11 / 1.76 mm | 0.85 / 1.41 mm |
| QSPI, same | 0.82 mm (QSPI_SS) | 1.3–3.1 mm top + about 6 mm bottom per net |
| Decoupling: worst copper path from a power pin | 4.35 mm | 4.86 mm |

What the measurements found:

- **Shared decoupling.** USB_OTP_VDD and QSPI_IOVDD share one capacitor on both boards. The
  design guide says so and defends it as a space trade-off. ADC_AVDD also shares one with the
  IOVDD pin beside it, which neither document mentions. The datasheet asks for a capacitor on
  each pin. Both claims are kept, linked as contradicting; the package follows the reference
  and says to split them where there is room.
- **The QFN-80 board's second memory.** The optional U204 sits on the bottom directly under the
  flash, and its stubs run on B.Cu. So each QSPI net has about 6 mm of track with no ground
  beside it, and the bottom ground under the top-side QSPI tracks is cut. The QFN-60 board's QSPI
  has ground under every millimetre except 0.8 mm of QSPI_SS.
- **USB return path near the chip.** Where USB leaves the QFN, 0.9–1.8 mm per line runs over cut
  ground on both boards, against the guide's own "solid, uninterrupted" rule. At full speed and
  these lengths it is unlikely to matter. The rule is kept as `contested`.

The length limits in `intent.json` are soft and set at the longer of the two references: what
the vendor achieved, not a datasheet limit.

## Judging a board by role

`ccomp check package BOARD --package packages/rp2350 --binding BINDING.json` judges a board
against this package:
- **Binding:** the board's bindings name which board part plays each package part (`CIN` →
  `C14`) and which variant the chip is.
- **Nets:** it finds the package's nets on the board through `schematic/nets.json`. That file is
  generated by `tools/package_nets.py` from both minimal boards' netlists and lists each net's
  pins by role, in RP2350A numbers. RP2350B pins are translated by name.
- **Wiring:** it compares signal nets pin by pin. A resistor or capacitor fitted the other way
  round is accepted and listed.
- **Judging:** it measures and judges every block constraint, every net length limit (between
  the pin pair the reference was measured on), and every part-value check in `rules.json`. Each
  verdict carries its source.

A value string that names no part number ("12MHz" for the crystal) is reported as
unverifiable, not failed. A package part with no board part bound to it is reported as unbound.

| Board | Hard failures | Soft failures | Notes |
|---|---|---|---|
| RP2350A Minimal (`reference/rp2350a_minimal/check.json`) | 0 | 3 | USB chip side and 0.8 mm of QSPI_SS over cut ground |
| RP2350B Minimal (`reference/rp2350b_minimal/check.json`) | 0 | 8 | the second memory's bottom-side stubs; its decoupling caps are not bound (their roles are QFN-60 pins) |
| easy2350 (`boards/easy2350/rp2350.check.json`) | 2 | 8 | USB_DP/DM run 6.1 and 8.5 mm on B.Cu over the +3V3 plane, not ground. QSPI runs 4–6 mm longer than the reference and mostly over +3V3 |

The BOOTSEL resistor's distance to the flash was 3.0 mm in 0.2.0's first draft. Both reference
boards measure 3.9–4.1 mm, so the soft limit is now 4.1 mm, what the vendor achieves.

Not in this release: generators for the interface blocks (milestone 5 places them).
