# rp2350-motor-controller, mined

Source: github.com/sequoia-hope/rp2350-motor-controller, tag `rev-A-fab` (49338a4), the fabricated
rev A. `hardware/rev-A/` in a local checkout is byte-identical to the tag (sha256 of the board
8fd41c08…, schematic 9f81f369…, project a3576c6d…). It is a 6-layer board with 291 parts and
188 nets. The board is not copied here; `mine.spec.json` reads it from `~/pcb/`.

Rebuild: `python3 tools/compare_buck.py boards/rp2350-motor-controller/mine.spec.json boards/rp2350-motor-controller/mined`.

## Measured

| Block | Hot loop mm² | Switch node mm² | Output loop mm² | FB clearance mm | Copper under SW mm² |
|---|---|---|---|---|---|
| RP2350B core regulator (U8, L2, C17, C18) | 1.054 | 2.207 | 2.464 | 0.415 | **2.223** |
| XL7005A, VMOT → Vdrive (U28, L3, D4, C81, C11) | 26.33 | 31.58 | 53.73 | 0.507 | 31.39 |

For reference, the RP2350A regulator measures 1.064 / 2.301 / 2.452 / 0.386 / 0 on easy2350,
and the package's generated layout measures 1.025 / 1.904 / 2.343 / 0.415 / 0.

## Findings

1. **The In1.Cu ground plane is solid under the RP2350B's VREG_LX**, on rev A and on rev B as it
   stands. This is 2.22 mm², essentially all of the switch node. The RP2350 datasheet (§6.3.8
   p.412, Figure 27) asks for the layer under LX to be cut away on boards with inner layers (rule
   `rp2350.vreg.lx_cutout` in `packages/rp2350`). Many engineers argue the opposite for switch
   nodes in general: a solid plane underneath is the return path and reduces emissions. So this
   is recorded as a dispute between the datasheet and expert practice, to be settled by
   simulation (the spec's research library), not as a defect. The rest of the block matches
   Raspberry Pi's reference within 2 %.
2. **The XL7005A loops are 25× the RP2350's.** The input capacitor C81 (1 µF 100 V, 3.1 mm from
   the VIN pin) and the freewheeling diode D4 sit on opposite sides of the IC. The switch-node pour
   on B.Cu is 27.7 mm². This is measured but not judged: the XL7005A datasheet is not ingested,
   so no cited limit applies yet. Its feedback trace crosses over the switch pour on In3.Cu; In4.Cu
   (a ground fill) lies between them and shields the crossing. That shielding is why the check
   now discounts crossings with a plane between (see below).
3. **C42 (100 µF 100 V) is 3.09 mm from the VIN pin**, as close as C81. At 100 µF it is an
   electrolytic, so C81 is the one that carries the high-frequency current and is used as CIN.

## What mining this board changed in the tools

- Buck checks take an optional `diode` role (asynchronous bucks): the loops close through the
  diode instead of the IC's ground pin.
- The output net is the inductor's other side. The feedback path runs to COUT, or to the
  divider when FB sits behind one.
- Feedback clearance discounts crossings on other layers that a plane between them shields.
  Published numbers do not move: easy2350 0.386, the package layout 0.415.

## Recognized, not yet measured

These are the circuit elements here for the block library and the later milestones:

- **Four half-bridges** (milestone 7): EG3113 drivers U1, U6, U7, U12 with PowerPAK SO-8 pairs
  AH1/AL1, BH1/BL1, CH1/CL1, CH2/CL2, SMAJ51A phase TVS diodes D9, D13, D17, D21, and bootstrap
  diodes.
- **Phase current sensing:** INA240A1D U5, U9, U10 across 2 × 8 mΩ shunts.
- **Bus protection:** SMDJ85A TVS D5 on VMOT. The LM74700 U29 ideal-diode controller with Q5 on
  VBUS is the first real disconnect among the boards here, for protection paths and protected
  zones.
- **USB-C** with an FUSB302 and a USBLC6-2P6; **isolated CAN** (CA-IS3052, ESDCAN24); the
  **encoder interface** (SIT3088 line drivers, TS5A3159 switches, DLW21 common-mode chokes).
- **RP2350B (QFN-80) support parts:** crystal, QSPI flash W25Q16JV, and decoupling: the
  package's QFN-80 variant (milestone 4).
