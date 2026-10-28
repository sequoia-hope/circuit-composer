# Lightning-bolt earring

A fat, gently zigzagged ⚡ earring, 72 × 33.7 mm, 6 layers. It hangs from a USB-C plug in a
narrow tab at its top. The front is entirely LEDs: 134 WS2816C-1313 on nested bolt-shaped
rings, and the field is symmetric under a half turn about the bolt's centre. The back carries
simple_earring's rev-2 circuit (vendored at 7a83b33) under a 10 × 30 mm LiPo.

**Status: routed.** ultraroute 184f640, seed 2 (`routed/best.kicad_pcb`). `scripts/validate.py`
reports:

| Check | Result |
|---|---|
| KiCad DRC after a zone refill, with the project's JLC rules | 0 errors, 0 unconnected; warnings are silk, 2 isolated pour islands, 1 dangling track and 1 dangling via |
| Board nets against the circuit | equal, 884 pins |
| Parts under the cell | all at most 1.1 mm |
| LED chains | every zone runs LED to LED; every LED's VDD and GND on its zone's VLED and GND |

The RP2350 core regulator, which the router laid out here, measures:

| | This board | Package's own layout |
|---|---|---|
| Hot loop | 1.11 mm² | 1.03 mm² |
| Output loop | 3.05 mm² | 2.34 mm² |
| Switch node | 3.08 mm² | 1.90 mm² |
| Feedback clearance | 0.54 mm | 0.415 mm (the limit) |

One leg of the hot and output loops reaches GND through the plane, so the checker measures it
between pad centres. Viewer: `pcbview.toml` builds `viewer/`.

`./build.sh` rebuilds everything up to routing:
- geometry
- circuit
- board
- rail-via and bypass-cap reservation
- floorplan (eight seeds, the best legal one kept)
- vias and caps
- planes

## Shape

- **Body:** a band along an axis leaning 14°, 20.4 mm (2 × HALF_W) across its middle,
  tapering to a point at each end. The centreline steps sideways at the middle (an odd `tanh`
  step), which gives the obtuse zigzag. The half-width is even along the axis and the step odd,
  so a half turn about the centre leaves the body unchanged.
- **Tab:** the USB-C tab is the one part that is not symmetric. It sits straight above the
  body's top point and flares into it; J1's through-holes are a front keep-out.

## LEDs

The rings are inward offsets of the body. On a ring that is one loop, the first half of the LEDs
are placed and each partner is the half turn of one of them (rotated 180°). Where a ring splits
into two islands, one island is placed and the other is its image. An LED goes in only if it
and its partner both pass the exact checks (pads against rails, courtyards, keep-outs). The
check that every LED has an exact partner passes for all 134.

Otherwise it is the flame earring's LED topology (`../flame-earring/README.md`):
- **Orientation:** each LED is turned 45° to its ring; data runs pad to pad on F.Cu, with GND
  and VLED rails beside it.
- **Spacing:** rings are 2.55 mm apart, which leaves via streets between them.
- **Vias:** rail vias and bypass-cap slots are reserved before the back is placed.

**Seams.** Each ring's seam is on a straight stretch of its left flank. The light sensor sits
in the gap opposite ring 2's seam, so the seam keeps room for zone C's data via; the one data
link that crosses the sensor is routed under it.

**Zones:** L is ring 0, R ring 1, C the rest.

**Power.** About 100 mAh of cell against 134 × 0.7–1.0 mA of LED idle current: 95–135 mA with
every zone on, so about 45–65 minutes with everything lit. Fewer zones lit, or a lower-idle LED
on the same footprint, lasts longer.

## Back

**Placed by `board.py`:**
- J1 in the tab, mouth up.
- U5, the USBLC6, where its USB pins are nearest J1's, with a reserved escape via for its VBUS
  pin (that pin sits between the USB pair's pins).
- BT1's wire pads beside the cell's upper end, found by search.
- SW2 at the board edge, facing out: its body face on a straight stretch of the left edge, the
  actuator 0.7 mm past it.
- simple_earring's MCU cluster under the cell, at the rotation that keeps it furthest inside.

**Placed by `ccomp floorplan`:** the rest, inside the outline. The floorplanner gained a repair
pass for parts its legaliser pushed off a shaped board.

## Layers

| Layer | |
|---|---|
| F.Cu | LEDs, rails, data |
| In1.Cu | GND, the router's one plane |
| In2.Cu | VLED pours, filled after routing, plus signals |
| In3.Cu, In4.Cu | signals |
| B.Cu | parts, signals |

The rails carry each zone's VLED. With In2's thin bands declared as planes, the router fed a
zone switch through a via where its band did not reach; with In1 alone, it routes the switches
to the rail vias as tracks.
