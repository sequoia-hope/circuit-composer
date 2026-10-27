# rp2350 0.1.0

A Circuit Composer package for the Raspberry Pi RP2350. This release holds one verified sub-block:
the **core voltage regulator**, the chip's on-die buck with an external 3.3 µH inductor, CIN and
COUT on pins 47–50.

| File | What |
|---|---|
| `manifest.toml` | Family, variants, source revisions, LCSC parts |
| `intent.json` | The block in the intent schema: roles, nets, objectives, hard constraints, ports |
| `rules.json` | 11 claims from the datasheet and design guide, each cited, with what enforces it |
| `sources/sources.json` | The documents, with SHA-256 hashes |
| `layouts/qfn60-4layer/` | The generated layout and its evidence (below) |

## layouts/qfn60-4layer

- `layout.json`: parts and copper by role, in U1's frame (mm, degrees added to U1's rotation).
  Tracks, two ground vias, and a keepout on In1.Cu under the switch node. Apply it with
  `tools/apply_layout.py`.
- `params.json`: the generator parameters it was made with.
- `fixture.kicad_pcb` and `fixture.kicad_pro`: the 4-layer test bench. It holds U1, the three
  parts and a ground plane on In1.Cu, under easy2350's net-class rules.
- `verify.json`: the fast-tier measurements on the bench, and KiCad 9.0.8's DRC (no errors).
- `compare/`: the reference board, its 4-layer transplant and this layout, measured by the same
  checker. Rebuild with `python3 tools/compare_buck.py packages/rp2350/layouts/qfn60-4layer/compare.spec.json packages/rp2350/layouts/qfn60-4layer/compare`.

| | Hot loop mm² | Switch node mm² | Output loop mm² | FB clearance mm |
|---|---|---|---|---|
| Raspberry Pi reference | 1.064 | 2.353 | 2.452 | 0.365 |
| This layout | 1.025 | 1.904 | 2.343 | 0.415 |

The reference uses copper zones (its LX strip is 0.3 mm wide). This layout uses tracks, with LX
at 0.2 mm, which is ample for 0.2 A but narrower. The checks do not yet price trace resistance or
inductance.

Not in this release: the QFN-80 (RP2350B) layout, a 2-layer layout, USB, the crystal, QSPI
flash, ports for the whole chip, and the reference schematic snippet. They come with milestone 4.
