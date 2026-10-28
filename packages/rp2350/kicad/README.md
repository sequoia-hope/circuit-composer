# Vendor KiCad libraries

The RP2350 symbol and the three footprints `schematic/circuit.RP2350A.json` names from Raspberry
Pi's own libraries, so a board composed from this package can be written without the vendor
archive. They are copied unchanged from the RP2350A Minimal Board KiCad archive (RP-006440-DD
rev 2; source `rp2350a_minimal` in `sources/sources.json`), as easy_pcb's easy2350 also carries
them.

| File | SHA-256 |
|---|---|
| `MCU_RaspberryPi_RP2350.kicad_sym` | `b1c2668f9b11d85ceab9e2583a18ee3722c96507d1a4a22a3b0998161b6ebcb2` |
| `RP2350_60QFN_minimal.pretty/C_0402_1005Metric_small_pads.kicad_mod` | `19310ad2c1d53fa547ee2c5cceb859269a6b486c68b66468111a834c74c749e8` |
| `RP2350_60QFN_minimal.pretty/L_pol_2016.kicad_mod` | `2912c3a1ae7091cf265e50828e52678a8fe88cd2fbaed2ce0a66fe626fe588e3` |
| `RP2350_60QFN_minimal.pretty/RP2350-QFN-60-1EP_7x7_P0.4mm_EP3.4x3.4mm_ThermalVias.kicad_mod` | `e2c03591ba061acbfe40adb44c212a322be5627d5d45d4e40dc5f48ba5b134d0` |

The archive states no licence for them. The core-regulator layouts were generated on these
pads, which is why the package keeps them rather than KiCad's standard equivalents.
`tools/circuit_check.py` looks here first.
