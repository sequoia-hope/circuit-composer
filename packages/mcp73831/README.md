# mcp73831 0.1.0

Microchip MCP73831: a single-cell Li-Ion/Li-Po linear charger in SOT-23-5. It does constant
current, then constant voltage, from one PROG resistor. The variant is **MCP73831T-2ACI/OT**
(LCSC C424093): 4.20 V regulation, options AC (10 % preconditioning below 66.5 % of VREG, 7.5 %
termination, recharge at 96.5 %). The variant name in the package files is `MCP73831T-2ACI-OT`,
because the variant also names `schematic/circuit.VARIANT.json` and a file name can't hold `/`.

Built from the datasheet DS20001984H, Revision H (June 2020). Its hash is in
`sources/sources.json`, and every page number below is the datasheet's printed page.

## Files

| File | What |
|---|---|
| `manifest.toml` | Variant, datasheet revision, LCSC parts with stock on 2026-10-27 |
| `ports.json` | The five SOT-23-5 pins (Table 3-1, p.11): their functions, classes and limits |
| `schematic/circuit.MCP73831T-2ACI-OT.json` | The typical application (Figure 6-1, p.17; values from p.1) by role |
| `rules.json` | 17 cited claims: capacitors, PROG, charge rate, thermal, layout, STAT, input protection |
| `sources/sources.json` | The datasheet: URL, SHA-256, size, date retrieved |

## The circuit

| Role | Value | Why |
|---|---|---|
| U1 | MCP73831T-2ACI/OT | The chip. KiCad `Battery_Management:MCP73831-2-OT` has the datasheet's pin numbers and names (checked pin by pin) |
| CIN | 10 uF 25 V X5R 0805 | VDD needs at least 4.7 uF (§3.1 p.11). The 10 uF part still has more than 4.7 uF left at 5 V DC bias (`buck.cap.dc_bias_effective`) |
| COUT | 10 uF 25 V X5R 0805 | VBAT needs at least 4.7 uF, for loop stability when no battery is connected (§3.2 p.11, §6.1.1.4 p.18) |
| RPROG | 4.7 kohm 1 % | IREG = 1000 V / RPROG = **213 mA** (§5.1.2 p.15) |
| R_STAT, D_STAT | 470 ohm, red 0603 LED | The LED runs from VDD into STAT and is lit while charging (Table 5-1 p.15). This is the p.1 value, about 5.5 mA |

Ports:

| Port | Kind | Pins | What |
|---|---|---|---|
| `VIN` | supply | U1.4 | The USB 5 V |
| `VBAT` | supply | U1.3 | The cell's positive terminal |
| `GND` | ground | U1.2 | VSS |
| `STAT` | signal | U1.1 | Optional; can be listed in `no_connect` |

**Charge current.** The datasheet prefers 1C and allows at most 2C (§6.1.1.1 p.17). The chip
can be set to at most 500 mA. For a 500-1000 mAh cell, 213 mA is:
- 0.43C for a 500 mAh cell, which charges in about 2.5-3 h;
- 0.21C for a 1000 mAh cell, about 5 h.

1C for the smallest cell (500 mA, with a 2 kohm RPROG) would meet the rule. It costs more heat
than this SOT-23-5 can shed, though, and the datasheet itself shows that case running in thermal
regulation (Figure 6-3, §6.1.1.3 p.18). If a board's cell allows a faster charge and the board
has the copper for it, the board can change RPROG.

**Thermal.** The worst case comes when the cell crosses from preconditioning to fast charge:
P = (VDDmax − VPTHmin) × IREGmax (§6.1.1.3 p.18). θJA is 230 °C/W on minimum copper, and
130 °C/W or better on large copper (p.5).

| Case | Inputs | Dissipation | Rise at 230 °C/W | Rise at 130 °C/W |
|---|---|---|---|---|
| Nominal | 5.0 V in, VBAT 2.79 V, 213 mA | 0.47 W | 108 °C | 61 °C |
| Worst | 5.5 V in, 2.67 V, 236 mA | 0.67 W | 154 °C | 87 °C |

The die never actually reaches those worst-case temperatures. Thermal regulation reduces the
current from about 80 °C junction (Figure 4-2), and charging stops at 150 °C (§4.10 p.14). With
the heat-sink pours in `mcp73831.layout.heatsink_vias`, expect a short current fold-back at the
start of fast charge and then full current. Dissipation falls as VBAT rises: 0.28 W at 3.7 V.

**STAT: an optional LED and a port.** The LED is optional by role: an instance writes
`"omit": ["D_STAT", "R_STAT"]` to drop it. STAT is also a signal port, so a host can read the
charge state, or a board can list it in `no_connect`. I did it this way, rather than only an LED
or only a port, because the datasheet gives both uses (§3.3 p.11, §6.1.1.7 p.18), and so that a
board can pick either one without editing the package. Composition was tried with the LED fitted
and with it omitted. Both composed cleanly.

The catch is on the host side. STAT's high level is VDD − 0.4 V, which is about 4.6 V from USB
(p.4). A 3.3 V host such as the RP2350 needs a divider, a series resistor into a 5 V-tolerant
pin, or a level shifter. The package does not include one (`mcp73831.stat.host`).

## What is verified

- `python3 tools/circuit_check.py packages/mcp73831/schematic/circuit.MCP73831T-2ACI-OT.json`
  prints `ok: true`. Every symbol and footprint exists in KiCad 10, every pin is a pad, and every
  symbol pin is on a net or port.
- `ccomp library check` prints `ok: true` with these 17 claims loaded.
- Every quote was found in the datasheet text by a script. In `stat.output`, "High-Impedance" is
  split across a line break in the PDF.
- `ccomp compose` works with the LED fitted and with it omitted.
- The LCSC parts were in stock on 2026-10-27 (counts are in `manifest.toml`). U1 is an extended
  part; the others are basic.

## What is not

- **No layout.** Figure 6-4/6-5 (`layout.parts_adjacent`, `layout.heatsink_vias`) describe one
  in words, but it has not been generated or measured.
- **No `intent.json`.** `ccomp check package` has not been run against a board.
- **Charge current at 4.7 kohm is assumed ±10 %.** The datasheet specifies IREG limits only at
  2, 10 and 67 kohm (p.3), and the 2 kohm row is not production tested.
- **The ~80 °C thermal-regulation onset is read off a plot** (Figure 4-2, at 2 kohm). The
  datasheet does not say whether it moves with RPROG.
- **The 1 % RPROG tolerance and the 10 uF capacitors are this package's choices.** The datasheet
  gives only a 4.7 uF minimum and no resistor tolerance.
- **The input TVS is left to the board** (`input.iovp`). Figure 6-1's SMAJ5.0A has VRWM 5.0 V,
  which is the VBUS nominal. Check the board's TVS against its VBUS maximum.
- **This is not a power-path design.** If the board's buck runs from VBAT, then:
  - the charger sees a battery even when none is fitted (`vbat.battery_detect`);
  - a system load above ITERM (7.5 % × 213 mA = 16 mA) means charging never terminates, and the
    cell is held at 4.20 V while USB is connected.

  Both are questions for the board.
