# easy2350, compiled

Made by `ccomp compile boards/easy2350/intent.json --pcb <easy_pcb>/examples/easy2350/hw/easy2350/a/easy2350.kicad_pcb -o routing.json --dru easy2350.kicad_dru --dru-base <that board's .kicad_dru>`.

- `routing.json`: for `ultraroute route ... --regions routing.json` (format agreed with ultraroute
  on 2026-10-27). It holds 3 regions with 12 rules, and 17 hard length limits. Weights: router =
  10 × intent weight.
  - `lx_inner_cutout`: nothing on In1.Cu under VREG_LX, tracks or vias (RP2350 datasheet §6.3.8
    p.412, Figure 27). The polygon is easy2350's own cut-out rule area, carried in L1's frame.
  - `crystal_quiet`: no vias on XIN/XOUT/XOUT_R, no VREG_LX, and QSPI and USB priced away around Y1.
  - `crystal_plane`: In1.Cu under the crystal stays ground only.
- `easy2350.kicad_dru`: easy_pcb's rules file, kept as it is, plus a circuit-composer block of
  `length` rules. KiCad 9 parses it and enforces it: tightening XIN to 1 mm makes DRC report XIN's
  9.86 mm.
- `regions-check.json`: `ccomp check regions` on easy_pcb's routed board. 0 violations.
