# eg3113 0.1.0

EGmicro (屹晶微电子) EG3113: a half-bridge gate driver for N-channel MOSFETs and IGBTs. It has a
bootstrap high side rated to 600 V, a 2.8-20 V supply, about 2 A source and 2.5 A sink, internal
dead time and an interlock. **HIN is active high and LIN is active low.** The variant is the
**SOP-8 EG3113** (LCSC C480666).

The package models **one phase of the RP2350 motor controller rev A**
(`~/pcb/rp2350-motor-controller`, `hardware/rev-A/`, tag `rev-A-fab`): bridge A, with driver U1
and the high-side and low-side MOSFETs AH1 and AL1. It includes the parts on their nets that serve
the bridge. Bridges B (U6, BH1/BL1), C (U7, CH1/CL1) and D (U12, CH2/CL2) are wired the same way.
Their gate, bootstrap and turn-off networks match U1's part for part. One exception: bridge D has
no ceramic bus capacitor of its own, which the board's README records.

**The datasheet is Chinese only.** EGmicro publishes no English EG3113 datasheet; its product page
lists two Chinese documents. The package was built from **V1.2**, as egmicro.com serves it (PDF of
2024-05-27). Its hash is in `sources/sources.json`. Quotes are the Chinese text exactly as the text
layer gives it, and statements translate them. For this, `tools/library_quotes.py`'s `norm()` now
keeps Chinese characters and drops the spaces the text layer puts beside them. Page numbers below
are **PDF pages**: the body is printed "n/9" on PDF page n+3.

The MOSFET's datasheet, Infineon BSC030N08NS5 Rev. 2.3, is the second source. Its page numbers
equal the PDF pages.

## Files

| File | What |
|---|---|
| `manifest.toml` | Variant, both datasheet revisions, LCSC parts with stock on 2026-11-02 |
| `ports.json` | The eight SOP-8 pins (Figure 4-1 and §4.2, p.5). Each pin's function, class and limits. HIN and LIN logic levels. Two interfaces: `hin` and `lin` (logic in) |
| `schematic/circuit.EG3113.json` | Bridge A's 22 parts by role, with the board's values. Five of them are not fitted (DNP) |
| `rules.json` | 15 cited claims: supply, drive voltage, bypass, input levels (two, contradicting), input defaults, input limits, interlock, dead time, timing, output current, bootstrap, high-side limits, turn-off diodes, MOSFET ratings |
| `sources/sources.json` | Both datasheets: URL, SHA-256, size, date retrieved, page and text-layer notes |
| `kicad/Driver_FET_EG3113.kicad_sym` | The symbol. KiCad has none for the EG3113 |

## The circuit

| Role | Board ref | Value | LCSC | Why |
|---|---|---|---|---|
| U1 | U1 | EG3113, SOP-8 | C480666 | The driver, from the datasheet's typical application (Figure 6-1, p.6) |
| QH | AH1 | BSC030N08NS5 | C501507 | High-side MOSFET, Figure 6-1's Q1 |
| QL | AL1 | BSC030N08NS5 | C501507 | Low-side MOSFET, Figure 6-1's Q2. The source goes straight to GND: this board senses current in the phase line, not in the low-side source |
| RGH | R13 | 10.2 Ω 0603 | C2933129 | Gate resistor, HO to QH (Figure 6-1's R1, which has no value) |
| RGL | R14 | 10.2 Ω 0603 | C2933129 | Gate resistor, LO to QL (Figure 6-1's R2) |
| DOFFH | D6 | CDBU0130R-HF (LCSC: CDBU0230-HF) | C2886025 | Turn-off diode across RGH, anode at the gate (Figure 6-1's D1, a 1N4148) |
| ROFFH | R10 | 0 Ω | C21189 | Link in series with DOFFH, where a separate turn-off resistor could go |
| DOFFL | D7 | as DOFFH | C2886025 | Turn-off diode across RGL (Figure 6-1's D2) |
| ROFFL | R11 | 0 Ω | C21189 | As ROFFH |
| QOFF | Q1 | FMMT591 PNP, SOT-23 | C686646 | Low-side turn-off: emitter to QL's gate, collector to GND. It discharges the gate locally when LO falls. The board's addition, not in the datasheet |
| RQB | R12 | 100 Ω | C22775 | QOFF's base resistor, from LO |
| RQE | R15 | 10.2 Ω | C2933129 | QOFF's emitter resistor, to QL's gate |
| CGSL | C6 | DNP, 0603 | — | QL's gate to GND. Not fitted on the board |
| DBOOT | D3 | DSS110, SOD-123F | C2923942 | Bootstrap diode, VCC to VB (Figure 6-1's D3, an FR107). A 100 V Schottky, per LCSC |
| RBOOT | R9 | 4.7 Ω | C2907035 | In series with DBOOT, limiting the charging current. The board's addition |
| CBOOT | C10 | 10 µF 35 V X5R 0603 | C194427 | Bootstrap capacitor, VB to VS: Figure 6-1's C2 is 10 µF too |
| CVCC | C2 | 4.7 µF 35 V X5R 0603 | C329940 | VCC bypass (§4.2 asks for 0.1 µF). It is U1's nearest Vdrive capacitor by copper, 2.1 mm away |
| CBUS | C1 | 2.2 µF 100 V X7S 1206 | C650969 | Per-bridge ceramic bus capacitor, VMOT to GND beside the MOSFETs. It closes the commutation loop (library `half_bridge.power.hf_cap_at_bridge`) |
| RSNH, CSNH | R17, C7 | DNP, 0805 | — | RC snubber across QH: VMOT – R17 – C7 – phase. Not fitted |
| RSNL, CSNL | R18, C9 | DNP, 0805 | — | RC snubber across QL: phase – R18 – C9 – GND. Not fitted |

The board's netlist was checked against the circuit file, role pin by role pin. All 18 nets and
ports match, and they contain no other package parts.

**Two values that are not the part.**
- DOFFH and DOFFL are valued CDBU0130R-HF on the board. LCSC C2886025, which JLC fits, is
  Comchip's **CDBU0230-HF** (30 V, 200 mA Schottky, SOD-523F). The package keeps the board's
  value and LCSC number, and gives the LCSC part as the MPN.
- Rev A values the MOSFETs `MOSFETSO-8`. Its later schematic names them BSC030N08NS5, which is
  what C501507 is.

**Which parts on the phase's nets are the board's.** The phase's nets also reach these parts, and
each stays out:

- **R1 and R2** (10 kΩ to GND on A_PWM_H and A_PWM_L). They sit 35 mm away at the MCU. They set
  the board's reset state, which is the board's choice. R2 overrides LIN's internal pull-up (see
  `eg3113.input.defaults`). HIN and LIN are ports.
- **TP14 and TP15** (on HIN and LIN), and **TP1 and TP2** (on the gates, DNP). These are the
  board's test points. A board adds them as taps.
- **R62/R63** (50k/50k, 0.1 %), with R64 (5.1k) to GND. This divider takes the phase voltage to
  the RP2350's ADC (A_PHASE_SENSE, GPIO43) and sits 30 mm away. It is the MCU's measurement, not
  the bridge's.
- **R20 and R23**, the 8 mΩ shunts from the phase to the motor terminal /C_N, belong to the
  **ina240** package with U5.
- **D9** (SMAJ51A), the motor-terminal TVS. It is on /C_N, beyond the shunts, at the connector:
  the board's protection.
- **The rest of Vdrive**: C11 (100 µF electrolytic), C5, the 100 nF parts, and C15/C27/C48, which
  are the other drivers' VCC capacitors. The XL7005A buck (U28) that makes Vdrive is also the
  board's.
- **The rest of VMOT**: C39/C42/C80 (100 µF electrolytics), C81, the other bridges' bus capacitors,
  and the LM74700 path, which is the **lm74700** package.

The snubber footprints **R17/C7 and R18/C9** are in the package because they bridge its own
nets: VMOT to phase, and phase to GND. They sit on F.Cu among the MOSFETs, 3.5–9 mm from them
(library `half_bridge.snubber.rc_across_fet`). **C6** is in for the same reason. All five are DNP,
as on the board. Compose writes them with the value `DNP`, which `ccomp check package` reads as
not fitted.

**Pins.** All eight were checked against the pin definition (Figure 4-1, p.5) and the pin
description (§4.2, p.5), one pin at a time:

- 1 is VCC (Vdrive).
- 2 is HIN (A_PWM_H), active high.
- 3 is LIN (A_PWM_L), active low.
- 4 is GND.
- 5 is LO.
- 6 is VS (the phase).
- 7 is HO.
- 8 is VB.

The board's symbol, `MotorBuck:EG2131`, is embedded in rev A's schematic; the project's
sym-lib-table does not list a MotorBuck library. It has the same numbers. It calls pin 4 COM and
draws LIN as ~{LIN}. The package's symbol keeps that symbol's geometry and pin types, names pin 4
GND as the datasheet does, and is valued EG3113. The truth table (p.11) heads its HIN column "pin
4"; that is a slip, because HIN is pin 2.

**Footprints.**
- U1 uses KiCad's `Package_SO:SOIC-8_3.9x4.9mm_P1.27mm`, as the board does. That matches the
  datasheet's SOP-8 (§9.1, p.12): a 3.9 × 5.0 mm body, 1.27 mm pitch, 6.0 mm over the leads.
- The MOSFETs use KiCad's own `Transistor_FET:BSC030N08NS5` symbol on `TDSON-8-1`, numbered 1–3
  S, 4 G, 5 D. The board used its own `motor:PowerPAK_SO-8_123`, a Vishay PowerPAK SO-8 pattern
  numbered 1 S, 2 G, 3 D. Both have a 1.27 mm lead pitch. Their land patterns were not compared
  pad by pad.
- QOFF uses `Transistor_BJT:Q_PNP_BEC`. That is the board's `Device:Q_PNP_BEC` under its KiCad 9
  and 10 name; `Device:Q_PNP` now has lettered pins.

Ports:

| Port | Kind | Pins | Board net | What |
|---|---|---|---|---|
| `VMOT` | supply | QH.5, CBUS.1, RSNH.1 | VMOT | The motor bus. The MOSFETs are rated 80 V |
| `GND` | ground | U1.4, QL.1–3, CBUS.2, CVCC.1, CGSL.1, QOFF.3, CSNL.2 | GND | Driver ground and power ground are one net on this board |
| `PHASE` | signal | QH.1–3, QL.5, U1.6, CBOOT.1, CSNH.2, RSNL.1 | /A_P | The switch node, which goes to the motor (here through the ina240 package's shunts). Its kind is `signal` so that compose does not treat it as a supply, but always connect it |
| `VDRIVE` | supply | U1.1, CVCC.2, DBOOT.2 | Vdrive | Driver supply, about 5.0 V on the board |
| `HIN` | signal | U1.2 | A_PWM_H | High-side input, active high |
| `LIN_N` | signal | U1.3 | A_PWM_L\ | Low-side input, **active low**. The `_N` says so |

## Interfaces and levels

- **`hin`** and **`lin`** are logic inputs. `ports.json` gives each the levels below, with
  `supply` = VCC:
  - `vih` = 2.5 V and `vil` = 1.0 V (§8.2, p.10);
  - `vin_max` = `supply + 0.3` for HIN, but a fixed **6 V** for LIN (Table 7.1, p.7).

  The datasheet contradicts itself on the high threshold. Table 7.2 (p.8, at VCC 12 V and 25 °C)
  says 3 V minimum; §8.2 (p.10) says "2.5V 以上" (above 2.5 V) and that MCU outputs can drive the
  inputs directly, and LCSC's older build of the same V1.2 has 2.5 V in the table too. The
  package follows 2.5 V (`eg3113.input.levels_text`); the table's 3 V is `eg3113.input.levels`,
  contested.

  **Consequence:** `ccomp pins` accepts an RP2350 GPIO on `hin` and `lin` (the RP2350 package's
  VOH is 2.62 V, 0.12 V above 2.5 V), as the reference board wires them. On the table's 3 V it
  would refuse them.
- **Floating inputs are off.** HIN has a 200 kΩ pull-down, and LIN a 200 kΩ pull-up to an
  internal 5 V (§2, p.4; Figure 5-1, p.6).
- **One signal on both inputs makes a complementary pair**, with the internal dead time of
  50–300 ns. An inverted copy on LIN does not: it turns both outputs off. The truth table (p.11)
  gives HIN/LIN → HO/LO as 00 → 01, 01 → 00, 10 → 00, 11 → 10.

## How the board uses it

**Drive voltage.** Vdrive is about 5.0 V: the XL7005A with R5/R6 at 10k/3.3k, which are the
board's. That is inside VCC's 2.8–20 V (`eg3113.vcc.range`). LO reaches 5 V, and HO 5 V less
DBOOT's and RBOOT's drops.

**Bootstrap.** CBOOT has the datasheet's value of 10 µF. The BSC030N08NS5 takes about 35 nC (typ)
to reach 5 V (Diagram 14), so CBOOT droops by under 10 mV per turn-on (`eg3113.bootstrap.circuit`).
DSS110 blocks VMOT + Vdrive while the high side is on.

**Gate network.** It follows Figure 6-1, with two additions: a 0 Ω link that can become a
turn-off resistor, and the PNP turn-off on the low side (`eg3113.gate.turnoff_diode`).

## Measured reference layout

Rev A's bridges were measured with `ccomp check half-bridge`. The results are in
`boards/rp2350-motor-controller/README.md` (section "Half-bridges and current sensing") and in
`boards/rp2350-motor-controller/mined/bridges/` (`bridge_a.check.json`, `bridge_a.svg`, and B to
D). For bridge A, this package's layout:

- Hot loop: 25.4 mm² through C1 (30.1 mm² placed).
- Phase path: 2.21 mm.
- Gate loops: 12.8 mm² (HS) and 17.3 mm² (LS).
- Gate paths: 11.4 mm and 10.4 mm.
- Bootstrap loop: 4.91 mm².
- CVCC: C2, 2.1 mm from pin 1, on B.Cu with the driver.

The driver is on B.Cu under the MOSFETs, which are on F.Cu. Those numbers are not judged here:
the EG3113 datasheet has no layout guidance. The library's `half_bridge` claims (hot loop, bus
capacitor at the bridge, driver near the FETs, bootstrap and supply decoupling) are the layout
rules, and this package's claims link to them where they apply.

## Where the board departs from the datasheet

- **5 V drive for a normal-level MOSFET** (`eg3113.vcc.drive_voltage`, contested).
  - The EG3113 datasheet recommends 10–15 V for MOSFETs that need a high gate voltage, and
    2.8–10 V for those that turn on low (§8.1, p.10). It does not define either kind.
  - The BSC030N08NS5 guarantees RDS(on) only at 10 V (3.0 mΩ) and 6 V (4.5 mΩ). Its Miller plateau
    is 4.6 V typical. Its typical curves at 5 V show RDS(on) about 4.7 mΩ, turning vertical near
    70 A (Diagrams 5–6, p.7).
  - Through 10.2 Ω, the plateau current is tens of mA, so the Miller interval is hundreds of ns.

  The package keeps the board's value. A board that composes this package for high current should
  consider a 10–12 V VCC, which the 35 V capacitors already allow.
- **The library's VCC-to-bootstrap ratio.** `half_bridge.bootstrap.recharge_loop` asks for the VCC
  capacitor to be at least 10 × CBOOT. C2 is 0.47 × C10. The EG3113 datasheet asks only for
  0.1 µF (`eg3113.vcc.bypass`, which C2 meets). The Vdrive rail's 100 µF C11 is the board's, and
  14 mm away.
- **LIN is held low at reset.** R2 (10 kΩ to GND, the board's) against the 200 kΩ internal pull-up
  puts LIN at about 0.24 V while the GPIO floats. That turns every low side on during reset and
  boot, which shorts the motor's terminals to GND. It may be intended as a brake. The package
  leaves R2 to the board and records what the inputs do alone (`eg3113.input.defaults`).

## What is verified

- `python3 tools/circuit_check.py packages/eg3113/schematic/circuit.EG3113.json` prints
  `ok: true` with `--kicad 9` and with `--kicad 10`, with no warnings.
- `kicad-cli sym export svg` loads the package's symbol.
- The circuit's nets and ports match the board's netlist (rev A, `kicad-cli sch export netlist`),
  role pin by role pin.
- `python3 tools/library_quotes.py --claims packages/eg3113/rules.json` finds all 29 quotes,
  Chinese and English, each on the page its locator names. Over the whole library, the extended
  `norm()` gives the same result for every other claim as before.
- `ccomp library check` prints `ok: true` with 15 eg3113 claims and no eg3113 warnings.
  `ccomp interfaces packages/eg3113` lists `hin` and `lin`.
- A two-phase composition composes: shared VMOT, GND and VDRIVE; each phase's inputs and PHASE on
  connectors. `tools/compose_kicad.py` writes it with `nets_match: true` and no ERC findings.
- Stock on 2026-11-02 is in `manifest.toml`. **The EG3113 (C480666) had no stock at LCSC or JLC.**
  Every other fitted part was in stock with lifecycle normal.

## What is not

- **No English datasheet.** The statements are translations. Each quote is the Chinese as printed,
  so a reader can check the translation against it.
- **The dead time, delays and drive currents are characterised only at VCC 12 V.** The datasheet
  gives nothing at 5 V.
- **The 5 V MOSFET figures are typical curves read off plots** (Diagrams 5, 6 and 14), not limits.
- **The MOSFET land pattern** (KiCad TDSON-8-1, the board's PowerPAK pattern, Infineon's Figure 3)
  was not compared pad by pad.
- **The diodes' and the PNP's datasheets were not read.** Their ratings (DSS110 100 V 1 A,
  CDBU0230-HF 30 V 200 mA, FMMT591 60 V 1 A) are LCSC's parameter listings.
- **VS below GND** is not limited anywhere except through VB's −0.3 V absolute maximum. Negative
  phase transients are not a claim.
- **The datasheet's DFN8 version** (p.12) is not modelled.
