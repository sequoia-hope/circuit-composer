# Flame earring

A slim LED earring shaped like a point-tipped S flame: 59 × 20.7 mm, 6 layers. The front is
entirely LEDs, 117 WS2816C-1313 on four nested flame-shaped rings. The back carries the
electronics under a 150 mAh-class prismatic LiPo. The circuit is simple_earring's rev 2
(RP2350A + W25Q16JV, LGS4056H charger, VBUS/cell power path, NCP730 LDO, one-button power latch
and BOOTSEL, three switched LED zones, LSM6DS3TR-C IMU, ALS-PT19 light sensor, USB-C). Only the
LED field changes: its count, its zones, its chain order.

`./build.sh` rebuilds everything up to routing from `scripts/`:

| Step | Script | Writes |
|---|---|---|
| Shape, cell, LEDs, rails | `scripts/geometry.py` | `geometry.json`, `preview.svg` |
| Circuit | `scripts/circuit.py` (simple_earring's, vendored) | `circuit.json` |
| Board: outline, front, fixed back parts, LED copper | `scripts/board.py` | `build/flame.kicad_pcb`, `floorplan/` |
| Rail vias and bypass-cap slots, reserved | `scripts/stitch.py reserve` | `floorplan/reserved.json` |
| The rest of the back | `ccomp floorplan` | `floorplan/placement.json` |
| Vias, caps | `scripts/stitch.py place` | `build/stitched.kicad_pcb` |
| Planes | `scripts/planes.py` | `build/unrouted.kicad_pcb` |

## Shape

The flame is two edge splines (`LEFT`, `RIGHT` in `geometry.py`).
- **Lower body:** stays at least 14 mm wide over the cell's 32 mm.
- **Upper third:** the left edge sweeps over and the right edge pulls in under the curl, then
  swells again, so the point hooks right.
- **Base:** blunt, with a 10 mm flat where USB-C's mouth sits.
- **Hanging:** a 1.0 mm plated ear-wire hole 6.2 mm below the point. simple_earring hangs from
  a plug left in its USB-C; a pointed flame cannot.

## The cell

The board is designed to a 12.5 × 32 mm envelope, plus 0.3 mm of hand placement, starting
7.6 mm above the base just clear of J1. That fits a 501230-class cell (EEMB LP501230:
140 mAh, 5.3 × 12.5 × 31 mm). A 401240 (about 150 mAh, 40 mm long) does not fit in 59 mm once
USB-C takes the base.

The envelope stays 0.37 mm inside the outline. Everything under it is at most 1.1 mm tall,
simple_earring's cap. J1 (3.2 mm) and SW2 (1.35 mm) sit outside it.

## The LED field

LEDs sit on nested flames: inward offsets of the outline, 2.55 mm apart. Each ring is a smaller
flame inside the last.

**Orientation.** Each WS2816C is turned 45° to its ring:
- DO and DI lie on the ring line, so data runs corner to corner along the ring on F.Cu.
- GND and VDD point across the ring, under a GND rail inside and a VLED rail outside. The rails
  are offset curves 0.75 mm either side of the ring line.

**Vias.** No LED needs a via. Rails stitch to the planes at chosen gaps, and data enters and
leaves each ring at its seam: an LED left out on the left flank, beside the MCU on the back.

**Via streets.** Between rings, rail edges and pad tips leave a 0.71 mm street, where a 0.4 mm
via fits with clearance. At 2.3 mm spacing there would be none, and nothing on the back could
reach the inner layers.

**Checks.** Every LED is checked exactly before it is placed:
- its GND pad touches an inside rail and its VDD pad an outside rail;
- every pad clears every other net's rail by 0.127 mm;
- courtyards do not overlap.

**Keep-outs.** No LEDs, rails or data links over the ear-wire hole or J1's through-hole span.

**Zones.** simple_earring's three switched zones, by ring: L is ring 0, R ring 1, C rings 2–3.

**Power.** The WS2816C idles at 0.7–1.0 mA per LED. 117 LEDs draw 80–117 mA whenever their
zone is on, even dark: about 1.2–1.7 h on a 140 mAh cell. The zones switch off separately. A
lower-idle LED on the same footprint is the fix if that is too short.

## The back

- **Fixed by `board.py`:** J1 at the base, BT1's wire pads either side of it, SW2 on the left
  edge above the cell, and simple_earring's MCU cluster (U1, its decoupling, L1, Y1; poses from
  its routed board, vendored) flipped onto the back.
- **The RP2350's footprint** is the vendor's without the nine thermal vias in its exposed pad:
  they would have come up through the LED field. The pad reaches GND through B.Cu.
- **The rest** is placed by `ccomp floorplan` inside the flame outline: a polygon outline, the
  fixed parts kept, the seams, rail vias and cap slots as keep-outs.
- **LED bypass:** 20 × 100 nF, one per 6 LEDs, each on the back between a GND and a VLED rail
  via. Each zone also has its In1/In2 plane pair under it and 10 µF at its switch.

## Layers

| Layer | |
|---|---|
| F.Cu | LEDs, rails, data |
| In1.Cu | GND |
| In2.Cu | VLED_L, VLED_R and VLED_C: concentric bands between ring midlines |
| In3.Cu, In4.Cu | signals |
| B.Cu | parts, signals |

JLC06161H-3313, 0.127 mm track and space, 0.4/0.2 mm vias: the rules simple_earring's 4-layer
build used.

## Vendored

`vendor/simple_earring/` holds simple_earring at 7a83b33: `circuit.py`, `parts.yaml`, the
footprint library, and the MCU cluster's poses.

## Where it ended

Superseded by the lightning-bolt earring (`../bolt-earring/`), on request. Best route: ultraroute
184f640, seed 1, `routed/best.kicad_pcb` (zones filled):
- **DRC:** 0 errors against the board's own rules, after a zone refill.
- **Checks passed:** the board's nets match the circuit (812 pins); all 87 parts under the cell
  are at most 1.1 mm tall; the LED chains are intact.
- **Open:** 2 connections, `LED_EN_C` (U1.34–R34) and `VBUS` at U5.5, the USBLC6's middle pin
  walled in by the USB pair.

An escape via for U5.5, reserved before the floorplan, was added afterwards (`stitch.py`
ESCAPES), but its routes did not finish. `pcbview.toml` builds the viewer into `viewer/`,
which is not committed.
