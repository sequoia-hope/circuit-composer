# rp2350-battery composed, placed and routed

Milestone 5's demo board, from its composition to a routed board with no hand edits in between:
USB-C, the MCP73831 charger, LiPo load sharing, the TLV62569 3.3 V buck, the RP2350A with its
crystal, flash, USB and core regulator, the LSM6DSO IMU, SWD and a user LED. `index.html` shows
the copper and the checks.

    ULTRAROUTE=/path/to/ultraroute tools/route_board.sh rp2350-battery OUT_DIR [SEED]

| Step | Tool | What it adds |
|---|---|---|
| Compose | `ccomp compose --intent` | The circuit, and the board's intent: each package's `intent.json` in board names (42 nets, 11 blocks), the composition's own `intent` over it (VBUS's current, CC1/CC2, a 3.3 V default for logic nets) |
| Place | `ccomp floorplan` | Every part; the core regulator as the package's generated qfn60-4layer layout, its copper keeping other parts off |
| Prepare | `tools/prepare_route.py` | 4 copper layers, JLCPCB rules, a net class per net from its intent class, GND on In1.Cu and +3V3 on In2.Cu, and the layout's 14 tracks, 2 vias and keep-out applied and locked |
| Compile | `ccomp compile` | `routing.json` and the `.kicad_dru` length block. Both are empty here: the rp2350 package's length limits are soft, and no package carries a region yet |
| Route | ultraroute e3d5cce | easy2350's recipe plus `--regions`, one thread |
| Check | `tools/verify_routed.py --refill` | KiCad DRC, regions, the buck checks (`verify.json`) |

The board here is seed 1: `rp2350-battery.kicad_pcb` with its `.kicad_pro` and `.kicad_dru`,
`route.log` and `verify.json`.

| Seed | Route time | DRC errors | Unconnected | Region violations | Tracks | Vias | Locked kept |
|---|---|---|---|---|---|---|---|
| 0 | 18.4 s | 0 | 0 | 0 | 422 | 105 | 16 |
| 1 | 18.1 s | 0 | 0 | 0 | 417 | 107 | 16 |
| 2 | 19.8 s | 0 | 0 | 0 | 435 | 111 | 16 |
| 3 | 24.0 s | 0 | 0 | 0 | 416 | 105 | 16 |

On every seed the router reports 3 connections failed and 0 open. The 3 are pads to their planes,
which the pours join once KiCad refills them. DRC is run after that refill. The warnings are
silkscreen only: overlaps, silk on copper and silk at the edge.

**The two bucks.** The RP2350's core regulator is placed and routed by its generator and
locked: hot loop 1.03 mm² on every seed, the same as the package's layout. The TLV62569 has no
generated layout, so the floorplanner places it as loose parts and the router wires it: hot loop
2.98 mm², output loop 16.2 mm², and 5.99 mm² of switch node without ground under it. A generator
for off-chip bucks (the spec's next step) would lock it like the core regulator.

**Against the rp2350 package's rules** (`ccomp check package`, `rp2350.check.json`, binding
`mcu.binding.json`): 41 pass, 22 fail. KiCad's DRC is clean, but the router was never told what
the package knows, so it put the crystal, QSPI and USB nets on B.Cu, over the +3V3 plane, with
vias. Two of the fails are hard: USB_DP and USB_DM run 12 and 10 mm over +3V3, as easy2350's
did. The rest are soft: XIN and XOUT have 3 and 2 vias against none; 11 more nets run partly
without ground under them, mostly over +3V3; and 7 traces are longer than the reference boards'
(FLASH_SS 14.3 mm against 2.7).
With GND on both inner layers it is 42 against 21, so the stack-up is not the cause. The fix
belongs in the package: these rules as routing constraints (regions, and hard length limits)
in its intent, so that every board composed from it routes with them.

**Fixed on the way.**
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
