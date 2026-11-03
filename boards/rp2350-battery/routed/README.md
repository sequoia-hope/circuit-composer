# rp2350-battery composed, placed and routed

Milestone 5's demo board, from its composition to a routed board with no hand edits in between:
USB-C, the MCP73831 charger, LiPo load sharing, the TLV62569 3.3 V buck, the RP2350A with its
crystal, flash, USB and core regulator, the LSM6DSO IMU, SWD and a user LED. `index.html` shows
the copper and the checks.

    ULTRAROUTE=/path/to/ultraroute tools/route_board.sh rp2350-battery OUT_DIR [SEED]

| Step | Tool | What it adds |
|---|---|---|
| Compose | `ccomp compose --intent` | The circuit, and the board's intent: each package's `intent.json` in board names (42 nets, 11 blocks), the composition's own `intent` over it (VBUS's current, CC1/CC2, a 3.3 V default for logic nets) |
| Place | `ccomp floorplan --intent` | Every part. Both regulators are their packages' generated layouts (rp2350 `qfn60-4layer`, tlv62569 `sot23-5-4layer`), moved as one, whose copper keeps other parts off. Nets with a length limit keep their pins' box under 0.75 of it. A part with a `near` pin turns its pad on that net toward it, and the line between them is kept clear of other courtyards |
| Prepare | `tools/prepare_route.py --composed` | 4 copper layers, JLCPCB rules, a net class per net from its intent class, GND on In1.Cu and +3V3 on In2.Cu, the two layouts' copper applied and locked (core regulator: 14 tracks, 2 vias, a keep-out; TLV62569: 20 tracks, 6 vias), and a locked stub from each decoupling capacitor to its pin where one fits |
| Compile | `ccomp compile` | `routing.json`: the rp2350 package's block constraints as regions (below) |
| Route | ultraroute f179fcb | easy2350's recipe (with `--diff-pairs`), plus `--regions`, one thread |
| Check | `tools/verify_routed.py --refill`, `ccomp check package` | KiCad DRC, the regions, the buck checks (`verify.json`); the board against the rp2350 package by role (`rp2350.check.json`, `mcu.binding.json`) |

## The package's rules reach the router

The rp2350 package states what its blocks need as constraints its checker measures: no vias on the
crystal nets (soft, weight 0.7), and USB, crystal and QSPI tracks over ground. USB_DP and USB_DM
are hard; the rest are soft, at weight 0.5 to 0.7. `ccomp compile` turns them into regions over
the whole board:

- **No vias:** a via rule on the nets. Hard: `never`. Soft: `avoid`, each via costing 10 × weight
  mm of track.
- **Over ground:** the nets keep off layers with no ground fill beside them (In1.Cu and B.Cu
  here). On the other layers (F.Cu, In2.Cu) they keep off the voids in the ground fill beside
  them, sent as rectangles: J1's shield and peg holes, the core regulator's In1 cut-out.
  A hard one also gets a `clear_under` entry: no other net's via or hole may open its plane
  antipad within 0.05 mm under these nets' tracks, so the router cannot make a void under them
  either.

29 regions and one `clear_under` (USB_DP and USB_DM on F.Cu and In2.Cu, over In1.Cu), none broken
on any seed.

## Decoupling: stubs, and room for them

A decoupling path is the copper from a power pin to its nearest capacitor (4.9 mm on the
reference boards). Placement already put every capacitor 1.1 to 2.4 mm from its pin; the paths
failed because the router wired each capacitor into its net anywhere (DVDD at pin 23: 11.9 mm,
through the far end of the net). So `prepare_route.py` draws each capacitor's stub itself and
locks it, as it does the core regulator's layout. It tries a straight track from the pin, two
right-angle and two 45° shapes, and the same from a point just out of the pin past its
neighbours, shortest first. It keeps the first that clears every other net's copper by 0.15 mm.

For the stubs to fit, the floorplanner now measures a `near` hint from the part's own pad on
that net, not its centre, and prices any courtyard across the line between them (`corridor`,
5 per mm). With that alone, all 10 stubs fit, but the capacitors on pins 20 and 23 fenced in
XIN and XOUT and pushed the crystal out (XIN routed 28 mm, and USB_DP broke a region on every
seed). So the rp2350 package's circuit gives the crystal, XOUT's damping resistor, the USB
series resistors and the flash `near` pins too (`package_circuit.py`'s `SIGNAL_NEAR`), and
their lines are kept clear the same way.

With the TLV62569 generated too (below), routed by ultraroute f179fcb with `clear_under` and
`--diff-pairs`:

| Seed | Route time | DRC errors | Unconnected | Region violations | Tracks | Vias | rp2350 rules pass / fail | Decoupling fails | Hard fails |
|---|---|---|---|---|---|---|---|---|---|
| 0 | 43.7 s | 0 | 0 | 0 | 377 | 91 | 54 / 9 | 1 | 0 |
| 1 | 36.8 s | 0 | 0 | 0 | 395 | 92 | 52 / 11 | 2 | 0 |
| 2 | 28.8 s | 0 | 0 | 0 | 430 | 101 | 50 / 13 | 1 | 0 |
| 3 | 28.8 s | 0 | 0 | 0 | 393 | 94 | 51 / 12 | 1 | 0 |

Before the decoupling stubs, the chain had 6 decoupling fails, 12 to 14 package fails, and a
hard fail on seed 2 (USB_DP over a QSPI via's plane hole). With the stubs and the signal hints
it had 13 to 15 package fails and none hard; with the TLV62569 generated, 11 to 13 (ultraroute
e3d5cce, no `--diff-pairs`); now 9 to 13, none hard, and the crystal passes every check. 8 of
10 stubs fit: C16 (pin 20) ends more than 4.9 mm away and C20 (pin 53) is blocked. The flash's
QSPI lines are what fail most, partly without ground under them, and QSPI_SD2 is 16.6 mm
against 13.2 on seed 1. The placement is legal at 27.6 × 53.9 mm with 263 mm of wire. The USB
pair's skew is tuned from 9.4 mm to 4.2 to 6.8.

The board here is seed 1. The router reports 7 connections failed and 0 open, all pads to
their planes, which the pours join once KiCad refills them. DRC runs after that refill, and its
warnings are silkscreen only.

**The two bucks are generated now.** The RP2350's core regulator: hot loop 1.03 mm² on every
seed. The TLV62569 (`packages/tlv62569/layouts/sot23-5-4layer`): hot loop 3.53 mm², output loop
11.05, switch node 5.21, FB clearance 2.67 mm, the same on every seed and on its bench. Placed
and routed freely, its hot loop had ranged from 2.98 to 11.4 mm² with the placement. Its
ground returns through the plane, since a SOT-23's GND pin sits beside SW across from VIN, and
its divider is placed by the generator too (`packages/tlv62569/README.md`).

## Fixed in ultraroute

Reported to the ultraroute session on 2026-11-03 and fixed on its main at f179fcb:

1. **Pair tuning kept out of regions.** With `--diff-pairs`, e3d5cce's tuning laid 54 mm of
   serpentine on USB_DM through a `never` region. Now no pass (tuning, `--tighten`) may break a
   region on a board with `--regions`, and route_board.sh uses `--diff-pairs` again.
2. **Large `must` polygons are fast.** A `must` region shaped as the In1 ground fill's outline
   (683 vertices) made prefanout take 156 s and escape 0 of 26 pads; the raster is now per edge
   band. The compiler still sends voids as rectangles.
3. **`clear_under`.** Keeping a net over ground also means keeping other nets' vias (their plane
   clearance holes) out from under it, as on seed 2 before. `routing.json` carries it now.

## Fixed on the way

- The floorplanner put two parts on the core regulator's locked copper: it knew the layout's
  parts, not its tracks and vias. Now a rigid layout's copper rides with the group as pad-less
  parts no courtyard may cover (`floorplan_cmd.rs`).
- With no outline set, a side an edge connector sits flush on lost the 0.5 mm margin for every
  part along it. C3's pads ended 0.287 mm from the edge, under the fab's 0.3 mm. Parts now keep
  the margin from a flush side (`flush_miss`).
- Separations were a fixed-weight cost, so an anneal could end just short of one (easy2350,
  seed 3: core_reg 1.11 mm from the crystal, 2 needed). Their weight now rises with the overlap
  weight, as in the analytical placer.
- `ccomp measure` and `check package` lost a net where two tracks' copper overlaps but no end lies
  inside the other track. ultraroute leaves such joins, and KiCad counts them as connected. They
  join now when the centrelines are within the half-widths' sum (`pcb/src/graph.rs`).
- `apply_layout.py` refused a layout made for the same footprint from another library: it
  compared pcbnew's UTF8 name to a Python string.
