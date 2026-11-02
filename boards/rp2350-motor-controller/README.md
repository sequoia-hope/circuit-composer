# rp2350-motor-controller, mined

Source: github.com/sequoia-hope/rp2350-motor-controller, tag `rev-A-fab` (49338a4), the fabricated
rev A. `hardware/rev-A/` in a local checkout is byte-identical to the tag (sha256 of the board
8fd41c08…, schematic 9f81f369…, project a3576c6d…). It is a 6-layer board with 291 parts and
188 nets. The board is not copied here; `mine.spec.json` reads it from `~/pcb/`.

Rebuild:

```
python3 tools/compare_buck.py boards/rp2350-motor-controller/mine.spec.json boards/rp2350-motor-controller/mined
python3 tools/compare_buck.py boards/rp2350-motor-controller/bridges.spec.json boards/rp2350-motor-controller/mined/bridges
python3 tools/compare_buck.py boards/rp2350-motor-controller/sense.spec.json boards/rp2350-motor-controller/mined/sense
python3 tools/compare_buck.py boards/rp2350-motor-controller/protection.spec.json boards/rp2350-motor-controller/mined/protection
python3 tools/compare_buck.py boards/rp2350-motor-controller/ideal_diode.spec.json boards/rp2350-motor-controller/mined/ideal_diode
ccomp measure ~/pcb/rp2350-motor-controller/hardware/rev-A/rp2350_driver.kicad_pcb --spec boards/rp2350-motor-controller/usb_esd.measure.json -o boards/rp2350-motor-controller/mined/protection/usb_esd_paths.json
```

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

## Half-bridges and current sensing

Four half-bridges, each an EG3113 driver on B.Cu under a pair of PowerPAK SO-8 MOSFETs on F.Cu.
Three of them have an INA240A1 that reads the phase current in line, across two 8 mΩ 2512 shunts
in parallel (4 mΩ). Measured with `ccomp check half-bridge` and `ccomp check sense` (outputs in
`mined/bridges/` and `mined/sense/`). The gate resistors, bootstrap capacitors and driver supply
capacitors are found from the nets.

| Bridge | Bus cap | Hot loop mm² (placed) | Phase path mm | Gate loop HS / LS mm² | Gate path HS / LS mm | Bootstrap loop mm² | VCC cap, path mm |
|---|---|---|---|---|---|---|---|
| A: AH1/AL1, U1 | C1 | 25.4 (30.1) | 2.21 | 12.8 / 17.3 | 11.4 / 10.4 | 4.91 | C2, 2.1 |
| B: BH1/BL1, U6 | C8 | 25.5 (29.9) | 2.28 | 11.2 / 18.8 | 10.5 / 11.4 | 4.81 | C15, 5.9 |
| C: CH1/CL1, U7 | C44 | 25.5 (30.4) | 2.21 | 12.1 / 20.4 | 9.5 / 11.9 | 4.78 | C27, 5.4 |
| D: CH2/CL2, U12 | C1 (phase A's) | **91.2** (97.3) | 2.06 | 11.2 / 17.5 | 12.3 / 10.2 | 4.71 | C48, 2.6 |

| Sense | Shunts | Loop mm² | + path mm, Kelvin offset mm | − path mm, Kelvin offset mm |
|---|---|---|---|---|
| A: U5 | R20 ‖ R23 | 2.66 | 4.81, 0 | 7.48, 0.30 |
| B: U9 | R39 ‖ R40 | 7.14 | 3.38, 0 | 8.98, 0.24 |
| C: U10 | R55 ‖ R56 | 3.28 | 5.19, 0 | 7.73, 0.51 |

None of these is judged: the half-bridge claims in `library/claims/half_bridge.json` ask for
small loops and short paths but give no number for them. The exception is TI's driver-supply rule (a
capacitor within 5 mm on the same layer): C2, C15, C27 and C48 all meet it, 2.0–4.2 mm from the
VCC pin, on B.Cu with the driver.

### Findings

1. **Bridge D has no ceramic bus capacitor.** Bridges A, B and C each have a 2.2 µF 100 V 1206
   (C1, C8, C44) about 4 mm from the high-side MOSFET. Their commutation loops are 25.4–25.5 mm²
   and match each other within 1 %. Bridge D (CH2/CL2, 12 mm below A) has none. Its loop closes
   through C1, 91 mm², or through the 100 µF electrolytic C39, 52 mm², whose inductance makes
   it a poor high-frequency return anyway. A 2.2 µF 1206 beside CH2, placed as at the other
   three, would bring D in line with them.
2. **The sense lines are close to Kelvin.** Each INA240 sits on B.Cu directly under its shunts,
   so the sense loops are 2.7–7.1 mm². Every + input lands on its shunt's pad. Every − input
   joins the phase-node pour 0.24–0.51 mm before the pad, so a fraction of a millimetre of
   load-carrying copper is in series with the shunt.
3. **Each amplifier reads one shunt of its pair.** Both inputs tap the same shunt (R23, R40 and
   R56). The reading is that shunt's share of the phase current, which the pour around the pair
   sets. A fixed split is calibrated out with the gain; it is not measured here.
4. **The low-side gate loops are 1.35–1.7× the high side's.** The low side returns through the
   ground planes to COM; the high side returns over the phase node to VS. All gate paths are
   9.5–12.3 mm, because the driver is on the other side of the board, under the MOSFETs.
5. **The motor-side net names cross over.** U5, on bridge A, measures into `/C_N`, and U10, on
   bridge C, measures into `/A_N`. The names say nothing about which bridge drives which
   terminal of J1 or J2, so the firmware's phase mapping should follow the bridges, not the
   names.

At 4 mΩ and the INA240A1's gain of 20, with REF1 at 3.3 V and REF2 at ground (mid-supply
reference), the full scale is about ±20 A. A two-pad shunt is acceptable there: the library's
four-terminal rule starts below 1 mΩ.

### What mining these changed in the tools

- `ccomp check half-bridge` and `ccomp check sense`: the checks above, with their SVGs.
- A pin with several pads (a MOSFET's three source leads, its drain tab and leads) is one
  terminal in the copper-path search. Published buck numbers do not move.
- `tools/compare_buck.py` takes a spec's `check` (`buck`, `half-bridge`, `sense`).

## Bus protection and the LM74700

The board has two power entries. The motor bus VMOT comes in on XT30 J9 and passes out on J10,
with the TVS D5 and the bulk capacitors on it. USB-C VBUS reaches VMOT through the LM74700 U29
and its MOSFET Q5, an ideal diode, and the bridged solder jumper JP1. The USB data lines pass
through U11 (USBLC6-2P6). Measured with `ccomp check protection` and `ccomp check ideal-diode`
(outputs in `mined/protection/` and `mined/ideal_diode/`).

| Entry | To TVS mm | Vias before | TVS lead mm (first pad) | Stub mm | Clamp loop mm² | Ground via mm, count within 1 mm |
|---|---|---|---|---|---|---|
| VMOT from J9 to D5 | 6.96 | 1 | −2.34 (C80) | 6.91 | 25.3 | 0, 5 |
| VMOT from J10 to D5 | 32.4 | 1 | −26.5 (CH2's drain) | 29.9 | 183 | 0, 5 |
| USB D+ from J3.A6 to U11 | 15.9 | **2, fails** | in line | in line | 13.4 | 1.12, 0 |
| USB D− from J3.A7 to U11 | 15.0 | **2, fails** | in line | in line | 17.2 | 1.12, 0 |

A negative lead means another pad on the net is reached before the TVS. The two USB entries
are judged by `interface.esd.no_via_before_tvs` (no via between the connector and the TVS).
The VMOT entries are measured, not judged: the ESD layout rules are scoped to signal connectors,
and a supply input has no cited layout limit.

| LM74700 (U29) with Q5 | mm |
|---|---|
| GATE to Q5's gate | 12.4 |
| ANODE to Q5's source; Kelvin offset | 18.6; 0 |
| CATHODE to Q5's drain; Kelvin offset | 11.6; 0 |
| VCAP → C79 → ANODE | 3.2 |

The LM74700 numbers are measured, not judged: its datasheet is not in the library yet.

### Findings

1. **The firmware asks for 20 V at boot, so U11 conducts on any 9–20 V charger.** The board's
   review records U11's VBUS pin on the PD rail as F-39 (MED): a 5 V-class clamp, abs max 5.5 V,
   which the library has as `usblc6.vbus.working_voltage` (VRWM 5 V, VBR 6 V min). The review
   counts it as safe for now because "today firmware never requests above 5 V". But
   `firmware/platformio.ini` builds with `HAS_USB_PD=1`, and `firmware/src/main.cpp:2187` calls
   `pd_ufp->init(PIN_PD_INT, PD_POWER_OPTION_MAX_20V)`, and has since the firmware was added
   (344bbfc, 2026-05-26). A charger that offers 9 V or more gets a contract above 5 V at boot,
   and U11's internal zener conducts. Until rev B moves pin 5 to +3V3, the firmware should ask
   for 5 V.
2. **USB ESD protection is far from the connector, behind two vias.** U11 is 15–16 mm of copper
   from J3, through two vias; the library's rules ask for no via before the TVS
   (`interface.esd.no_via_before_tvs`) and for the TVS near the connector, with the protected IC
   much farther from it than the connector is (`interface.esd.tvs_near_connector`). Here the
   RP2350 is 12.6 mm (D+) and 14.1 mm (D−) past U11, through the 27 Ω series resistors: 0.8 and
   0.94 times the connector-to-U11 distance, not much more than 1 (`mined/protection/usb_esd_paths.json`).
   The 27 Ω resistors limit what reaches the RP2350. U11's ground pad has no via within 1 mm;
   the nearest is 1.12 mm away.
3. **D5 sits behind J9, but the bulk capacitor is reached first.** D5 is on B.Cu 5 mm from J9's
   pins; the VMOT pour is on F.Cu only, so the surge reaches D5 through a via cluster at its pad,
   after 7 mm of copper. C80 (100 µF) is 2.3 mm nearer J9 along the copper, which is harmless:
   a bulk capacitor absorbs a surge too. The clamp loop is 25 mm², and D5's ground pad has five
   vias in or beside it.
4. **From J10, a surge reaches bridge D before D5.** J10 is the pass-through for a daisy chain.
   Energy entering there (a neighbouring board's regeneration, a hot plug at the far end)
   reaches CH2's drain 26.5 mm of copper before D5, and the clamp loop is 183 mm². There is no
   TVS near J10.
5. **Rev A's D5 cannot protect its MOSFETs; the current schematic has fixed it.** The fabricated
   rev A has D5 = SMDJ85A (LCSC C2990214): its 85 V standoff is above the 80 V MOSFETs (the
   current schematic names LCSC C501507 as BSC030N08NS5). The README and the power review
   describe SMDJ64A, which the current schematic has.
6. **The LM74700 senses Q5 at its pins.** Both sense paths land on Q5's own pads (Kelvin offset 0).
   The anode pin reaches Q5's source through the EN pin, over 18.6 mm of 0.2 mm track around the
   RP2350. It carries only the controller's supply current, so its drop is negligible against
   the forward voltage it regulates. Rev B replaces U29 and Q5 (an LTC4359 and a PowerPAK
   MOSFET), so these numbers describe rev A only.

### What mining these changed in the tools

- `ccomp check protection` and `ccomp check ideal-diode`, with their SVGs.
- The Kelvin-tap search behind `ccomp check sense` is shared with the ideal-diode check;
  capacitors on the sensed net are not load copper there. The sense numbers do not move.

## Recognized, not yet measured

These are the circuit elements here for the block library and the later milestones:

- **Half-bridge protection:** SMAJ51A phase TVS diodes D9, D13, D17, D21 and the bootstrap
  diodes, the gate turn-off paths (a Schottky and 0 Ω on each gate, and a transistor on the low side).
- **USB-C** with an FUSB302 and a USBLC6-2P6; **isolated CAN** (CA-IS3052, ESDCAN24); the
  **encoder interface** (SIT3088 line drivers, TS5A3159 switches, DLW21 common-mode chokes).
- **RP2350B (QFN-80) support parts:** crystal, QSPI flash W25Q16JV, and decoupling: the
  package's QFN-80 variant (milestone 4).
