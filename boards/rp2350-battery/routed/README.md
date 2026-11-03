# rp2350-battery composed, placed and routed

Milestone 5's demo board, from its composition to a routed board with no hand edits in between:
USB-C, the MCP73831 charger, LiPo load sharing, the TLV62569 3.3 V buck, the RP2350A with its
crystal, flash, USB and core regulator, the LSM6DSO IMU, SWD and a user LED. `index.html` shows
the copper and the checks.

    ULTRAROUTE=/path/to/ultraroute tools/route_board.sh rp2350-battery OUT_DIR [SEED]

| Step | Tool | What it adds |
|---|---|---|
| Compose | `ccomp compose --intent` | The circuit, and the board's intent: each package's `intent.json` in board names (42 nets, 11 blocks), the composition's own `intent` over it (VBUS's current, CC1/CC2, a 3.3 V default for logic nets) |
| Place | `ccomp floorplan --intent` | Every part. The core regulator is the package's generated qfn60-4layer layout, whose copper keeps other parts off. Nets with a length limit keep their pins' box under 0.75 of it |
| Prepare | `tools/prepare_route.py` | 4 copper layers, JLCPCB rules, a net class per net from its intent class, GND on In1.Cu and +3V3 on In2.Cu, and the layout's 14 tracks, 2 vias and keep-out applied and locked |
| Compile | `ccomp compile` | `routing.json`: the rp2350 package's block constraints as regions (below) |
| Route | ultraroute e3d5cce | easy2350's recipe without `--diff-pairs` (see "For ultraroute"), plus `--regions`, one thread |
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

29 regions in all, 0 broken on every seed. Against the package: 22 fails before, 12 to 14 now.

| Seed | Route time | DRC errors | Unconnected | Region violations | Tracks | Vias | rp2350 rules pass / fail | Hard fails |
|---|---|---|---|---|---|---|---|---|
| 0 | 40.0 s | 0 | 0 | 0 | 395 | 93 | 50 / 13 | 0 |
| 1 | 56.8 s | 0 | 0 | 0 | 401 | 92 | 50 / 13 | 0 |
| 2 | 43.7 s | 0 | 0 | 0 | 410 | 99 | 49 / 14 | USB_DP, 0.15 mm |
| 3 | 51.6 s | 0 | 0 | 0 | 396 | 93 | 51 / 12 | 0 |

The board here is seed 1. Every check the crystal has passes now: no vias, over ground, and
within its lengths (it was 24 mm from the chip before the floorplanner read the limits). The
router reports 8 connections failed and 0 open. The 8 are pads to their planes, which the
pours join once KiCad refills them. DRC runs after that refill, and its warnings are silkscreen
only.

**What still fails (seed 1, all soft):**
- Six decoupling paths are over the 4.9 mm the reference boards measure (DVDD at pin 23,
  11.9 mm). The floorplanner places each capacitor near its pin, but nothing prices the
  copper path.
- QSPI_SCLK, SD0 and SD3, and the USB_DP_PIN stub, run partly without ground under them.
  Their rules are soft prices, and the router pays them.
- FLASH_SS is 3.8 mm against 2.7, QSPI_SD3 12.7 against 12.5, and R_BOOTSEL is 4.103 mm from
  the flash against 4.1.

**Seed 2's hard fail** is one the router makes itself: USB_DP passes between two QSPI_SD3 vias,
and their clearance holes in the In1 plane leave 0.15 mm of the pair over a gap. No region
drawn before routing can foresee it.

**The two bucks.** The RP2350's core regulator is generated and locked: hot loop 1.03 mm² on
every seed. The TLV62569 has no generated layout, so it is placed and routed as loose parts:
its hot loop is 4.14 mm² with this placement, 2.98 with the last. A generator for off-chip bucks
would lock it like the core regulator.

## For ultraroute

Not yet sent; the ultraroute session was not running.

1. **Pair tuning ignores regions.** With `--diff-pairs`, `tune: /USB_DP//USB_DM skew 54.34 ->
   0.16 mm` laid serpentines on USB_DM through the region `mcu/usb:over_ground:0:void0`, a
   `never` rule (`regions: ... 2 pieces break its Never rule`). The 54 mm skew itself came from
   one net of the pair detouring round the regions. route_board.sh leaves `--diff-pairs` out
   until tuning keeps to the regions.
2. **Large `must` polygons are slow.** A `must` region per layer set, shaped as the In1 ground
   fill's outline (683 vertices), made prefanout take 156 s instead of 4 s and escape 0 of 26
   pads. The compiler sends voids as small `never`/`avoid` rectangles instead.
3. **Other nets' vias under a referenced net.** Keeping a net over ground also means keeping
   other nets' vias (their plane clearance holes) out from under it. Seed 2's USB_DP fail is
   this.

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
