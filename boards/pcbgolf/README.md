# PCBGolf

comma.ai's [PCBGolf](https://github.com/commaai/PCBGolf) challenge: make the panda-jungle-style
board as small as possible while keeping its interfaces and function. The deadline (2026-10-12)
has passed. This attempt is a test of Circuit Composer and ultraroute on a dense real board.

    score = PCBA bounding-box volume (mm³) + 50 × vias + 5,000 × copper layers

**Input:** `input/`, the repo at commit d0fd5a7: KiCad 10 schematic (5 sheets), an unplaced and
unrouted 2-layer board (245 parts, 309 nets, no outline), footprints and STEP models.
`input/pcbgolf.net` is the netlist, exported with KiCad 10.0.7.

**Scoring tool:** `tools/pcbgolf_score.py BOARD` exports the assembly to STL with KiCad 10 and
takes the bounding box over it, counts vias and copper layers with pcbnew, and prints the score.

## The leaderboard

First is 8,886; second is 10,345, marked 👑 (the page doesn't say what the mark means). Two copper
layers already cost 10,000, so both top entries are almost certainly **single-layer** boards,
and the leader's volume is under about 3,900 mm³.

## The circuit (from `input/pcbgolf.net`)

| Sheet | Parts | Blocks |
|---|---|---|
| Power | 35 | 12 V barrel jack (PJ-002AH-SMT), SMAJ16CA TVS, SI7101DN reverse-protection P-FET, two AP62300 bucks (5 V, 3.3 V) |
| STM32H7 | 33 | STM32H725ZGT (LQFP-144), 25 MHz oscillator, RGB LED, push button |
| USB + SD | 36 | USB2517 hub (QFN-64) with 24 MHz crystal, USB-C host port (J3), microSD (J2) |
| CAN-FD | 49 | 4 × MCP2542FD transceivers with chokes, 4 × OPA197 (current sense), CAN header J4 (2×4) |
| Channels | 92 | 4 × OBD-C ports (vertical USB-C, J5–J8), 4 × TPS25944 eFuses, 8 × QS5K2 FET pairs, 8 bicolour LEDs |

The four OBD-C ports are full 24-pin USB-C. Each one carries all four CAN buses on its
high-speed pairs (TX1, TX2, RX1, RX2), bused across all four ports, plus its own USB D+/D−,
SBU1/2 and switched VBUS. Each signal appears on both the A and B rows. On a single copper layer,
every crossing between those buses needs a jumper part, which costs area. The rules allow more
than one PCB.

## What sets the score

| Lever | Original | Cost in the score |
|---|---|---|
| Copper layers | 2 | 5,000 each: going from 2 to 1 saves as much as 5,000 mm³ of volume |
| Vias | none yet | 50 each: 100 vias cost as much as a whole layer |
| Height | vertical USB-C ×4 (about 9 mm above the board), barrel jack, LQFP-144 | multiplies the whole footprint area |
| Area | 2,634 mm² of footprints as drawn; LQFP-144 alone is 545 mm² | multiplies the height |
| Board thickness | 1.6 mm | adds to the height everywhere |

The rules allow firmware changes. Mating connectors must stay mechanically and electrically
compatible, the board must be manufacturable and assemblable at JLCPCB, and it must work.

## Substitutions

Researched in `research/silicon.md` and `research/connectors.md`: every dimension there is cited
to a datasheet page, and JLC stock is as of 2026-10-27.

**The barrel jack sets the height.** A plug 5.5 mm across needs a jack about 7 mm tall. The
lowest Ø2.0 mm-pin jack stocked at JLC is the G-Switch DC-S007A-D020-15A (C20611871). It is
mid-mounted in a board-edge cutout and stands 5.92 mm above the board top, and its pin tail
reaches 1.51 mm below. The original PJ-002AH-SMT is 11 mm tall. Once the box is about 7.4 mm
tall anyway, a part shorter than that costs only its area.

**So the USB-C ports go vertical.** The XYECONN TYPE-C-G08B-2TDWH6.4 (C20883029, full 24
contacts) stands 6.40 mm and covers 8.94 × 4.30 mm. A horizontal mid-mount part covers about
11 × 7.9 mm. The vertical part brings its A-row pads out on one side and its B-row pads on the
other, single rows at 0.5 mm pitch. So every pad escapes on one copper layer, which is not true
of the horizontal parts' two staggered rows. The drawing's recommended board is 0.8 ± 0.1 mm,
which is JLC's thinnest 1-layer board. Height becomes 6.40 + 1.51 ≈ 7.9 mm.

| Part | Original | Substitute | Why |
|---|---|---|---|
| OBD-C ×4, host USB-C | GCT USB4115 vertical 9.30 mm; JAE DX07S024 | XYECONN TYPE-C-G08B-2TDWH6.4 ×5 | 6.40 mm, one-layer escape; the host port uses the same part |
| DC jack | PJ-002AH-SMT, 11 mm | DC-S007A-D020-15A mid-mount | Ø2.0 pin, mates the same 5.5 mm plug |
| microSD | Molex 47219-2001 | XUNPU TF-128-ARP9, 1.15 mm, 11.52 × 8.06 | the card overhangs the edge |
| CAN header | 2×4 vertical 2.54 mm | right-angle 2×4 (C42453962), 6.90 mm | mates the same female 2×4; its posts point out of a notch |
| Button | EVQ-Q2 6.5 × 6.0 | KXT321LHS 3.0 × 2.0 × 0.6 | |
| MCU | STM32H725ZGT6, LQFP-144 | STM32H723VGT6, LQFP-100 (C730142) | same core, FDCAN ×3, SDMMC1, USB FS, I2C2. FDCAN3 moves to PD12/PD13, ADC PF11 to another channel. LDO only, so the SMPS parts go |
| USB hub | USB2517, QFN-64 9 × 9 | 2 × CH334P, QFN-16 3 × 3, cascaded, 12 MHz crystals | 7 ports for the 5 used. Loses the SMBus setup |
| CAN activity comparators | 4 × OPA197 SOIC-8 | 1 × OPA4991 X2QFN-14 2 × 2 | 40 V, rail-to-rail; TI says it works as a comparator |
| eFuses | 4 × TPS25942A WQFN-20 | 4 × TPS259470A QFN-10 2 × 2 | 23 V, 5.5 A. Its current monitor shares the limit pin, so firmware rescales it |
| Bucks | AP62300 TSOT-26 | AP62300Z6 SOT-563 | same die. TPS62933 (30 V) if the 22 V abs max against the TVS's 26 V clamp matters |
| FETs, LEDs, TVS | QS5K2, SOT-23 bicolour, SMAJ16CA | DMN3055LFDB 2 × 2, APHB1608 bicolour, SMF4L16CA | |
| CAN transceivers | MCP2542FD TDFN-8 2 × 3 | kept | nothing smaller at JLC with VIO and STBY |

**One copper layer.** A 0 Ω 0402 jumper over a crossing adds about 0.5 mm² × 7.9 mm ≈ 4 mm³.
A via costs 50, and the second layer costs 5,000. On two layers, double-sided placement would
have to save more than 5,000 / 7.9 ≈ 630 mm² of area to pay for the layer, and it can't:
- **What it can save:** the connectors and the LQFP-100 (1.6 mm tall) have to stay on top. On a
  0.8 mm board the bottom has only 0.71 mm of room before it adds height (the jack's tail sets
  the limit at 1.51 mm below the top). So the bottom takes only the small parts, and the board
  shrinks from about 1,600 mm² to about 900: roughly 700 mm² saved.
- **What it costs:** every net between a top part and a bottom part needs a via. The CAN block
  on the bottom alone needs about 23. A double-sided board needs 80–120, which is 4,000–6,000
  points.
- **The sum:** 900 × 7.9 + 10,000 + 4,000–6,000 ≈ 21,000–23,000, against 17,000–18,500 on
  one layer. On one layer, holes are unplated: the through-board parts (jack,
CAN header, USB-C pegs) solder on the copper side.

**Estimate before placement:** about 1,200 mm² of courtyards (connectors 530, MCU 256, the
rest under 450) plus routing room gives 1,500–1,700 mm², or roughly 12,000–13,500 mm³. That puts
the score at 17,000–18,500. The leader's 8,886 needs under 3,900 mm³, which at this height is
about 500 mm². The 4 USB-C ports, jack and microSD alone are about 470, so the leader is doing
something this plan does not yet.

**Not yet decided:** how the new silicon enters the KiCad schematic. The plan is to keep each
symbol's geometry and renumber its pins to the new package, so the sheets stay readable.

## Plan

1. Write the board's intent (`intent.json`) from the schematic: blocks (12 V input and the two
   regulators, four OBD-C ports with TPS25942 eFuses and current sensing, four CAN-FD
   transceivers, USB hub, MCU, microSD), net classes, current and voltage.
2. Decide the substitutions, each checked against the rules and cited: MCU package (STM32H725
   also comes smaller than LQFP-144), connector orientation and profile (low-profile parts that
   take the same plugs), passive sizes, board thickness.
3. Choose the layer count by trial: a 1-layer layout with jumper parts against a 2-layer layout
   with vias, scored by the same tool.
4. Place with a floorplanner (milestone 5's composer), route with ultraroute, run KiCad 10 DRC,
   export STEP, score.
