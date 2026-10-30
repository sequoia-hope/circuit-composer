# lgs4056h 0.1.0

Legend-Si LGS4056H: a single-cell Li-ion linear charger. It charges in four stages:
short-circuit, trickle, constant current and constant voltage. It takes up to 24 V in
operation (28 V absolute maximum), but charges only between its 4.0 V start threshold and its
6.5 V overvoltage stop. The charge current is set by one PROG resistor, up to 1 A. It has two
open-drain status outputs, CHRG and DONE.

The variant is **LGS4056HDA** (LCSC C5124109): DFN2x2-8 with an exposed pad, regulating at
4.20 V. This package models the chip **as the bolt earring uses it**: part U2 in
`boards/bolt-earring/circuit.json`, from simple_earring rev 2's `scripts/circuit.py`. It has
no NTC, CE is tied high, it charges at 50 mA, and CHRG/DONE go to RP2350 GPIOs (no LEDs).

Built from the LGS4056H datasheet Rev.C V0.4 (Mar 2022), as LCSC links it. The datasheet is
in English. Its hash is in `sources/sources.json`. Page numbers below are the printed ones,
which match the PDF pages.

## Files

| File | What |
|---|---|
| `manifest.toml` | Variant, datasheet revision, LCSC parts with stock on 2026-10-30 |
| `ports.json` | The nine pins (Table 3.2, p.3): functions, classes and limits. Levels on CE and the status pins. Two interfaces, `chrg` and `done` |
| `schematic/circuit.LGS4056HDA.json` | The bolt's charger by role |
| `rules.json` | 15 cited claims: capacitors, input range, PROG, CE, TEMP, status outputs, termination, thermal pad, layout |
| `sources/sources.json` | The datasheet: URL, SHA-256, size, date retrieved |

## Pins

The datasheet's names (Table 3.2, p.3) differ from the bolt's net names. Pin by pin:

| Pin | Name | Function | On the bolt |
|---|---|---|---|
| 1 | TEMP | Battery NTC input. Charging pauses outside 45-80 % of VCC; grounded or floating turns the check off | GND (NTC off) |
| 2 | PROG | Charge-current set and monitor, RPROG to GND | PROG, R8 18k |
| 3 | GND | Chip ground | GND |
| 4 | VCC | Input | VBUS |
| 5 | BAT | Charge output to the cell | VBATT |
| 6 | DONE | Open drain, low when charge is complete | CHRG_DONE → GPIO28 |
| 7 | CHRG | Open drain, low while charging | CHRG → GPIO19 |
| 8 | CE | Charge enable, high = on | VBUS (always enabled) |
| 9 | PAD | Exposed pad, to GND and copper | GND |

The task brief listed pin 8 as a second VBUS pin. It is actually **CE**, tied to VBUS. The
simple_earring source says so too (`# CE high = enabled`).

KiCad has no LGS4056H symbol. `Battery_Management:TP4056-42-ESOP8` has the same nine pins:
its `~{STDBY}` is DONE and its `EPAD` is PAD. I checked it pin by pin.

The footprint is KiCad's own `Package_DFN_QFN:DFN-8-1EP_2x2mm_P0.5mm_EP0.9x1.5mm`. The bolt's
`parts.pretty` copy has the same name and differs only in its 3D model. Its pads sit inside the
p.13 drawing:

- leads are 0.18-0.32 mm wide, and the pads are 0.25 mm;
- the EP is 0.9 ± 0.1 × 1.6 ± 0.1 mm, and the pad is 0.9 × 1.5 mm.

The left-hand drawing of Picture 3 (p.3) labels VCC as "3". That is a typo: the table and the
DFN drawing both say pin 4.

## The circuit

| Role | Bolt ref | Value | LCSC | Why |
|---|---|---|---|---|
| U1 | U2 | LGS4056HDA | C5124109 | The chip |
| CIN | C16 | 10 uF 10 V X5R 0603 | C19702 | VCC needs at least 10 uF *effective* ceramic, close to pins 4/3 (p.3, p.6, p.11). **Short at bias**, see below |
| CBAT | C17 | 10 uF 10 V X5R 0603 | C19702 | BAT needs at least 10 uF effective ceramic (p.3, p.6). **Short at bias** |
| RPROG | R8 | 18 kohm 1 % 0402 | C25762 | Sets **50 mA**, below |

C28 (100 nF on VBUS) is left out. It sits at the USBLC6 ESD part, by simple_earring's own
comment, so it belongs to the board's USB input.

**Charge current.** The datasheet gives two formulas, with I_BAT in mA and R_PROG in kohm
(Table 3.2 p.3, Set Output Current p.7):

- above 300 mA: I_BAT = 1000 / R_PROG;
- below 300 mA: I_BAT = 900 / R_PROG.

The p.7 form of the same thing is IBAT = (VPROG / RPROG) × 900 mA, where VPROG is 1 V in
constant current.

For the bolt: **900 / 18 = 50 mA**. That is 0.36C for its LPR302823 cell, and under the cell's
70 mA maximum (simple_earring's reasoning).

The datasheet only tabulates 1, 1.2, 2 and 9 kohm (Table 4 p.4). It states no tolerance on
ICC and no maximum RPROG, so 50 mA at 18 kohm is the formula extrapolated. The Component
Selection table (p.6) gives only the ×1000 form, which would mean 55.6 mA.

Other currents that follow from 50 mA:

| | Fraction of ICC | Current |
|---|---|---|
| Trickle charge (below 2.9 V) | 10 % | 5 mA |
| Short-circuit charge (below 0.6 V) | 5 % | 2.5 mA |
| Termination | 1/10 | 5 mA |

Dissipation is at most about 0.14 W.

**TEMP grounded is sanctioned.** The datasheet says so twice:
- p.5: "NTC function is not used, ground the TEMP pin."
- p.8: "If the battery temperature monitoring function is not required, the TEMP pin must be
  grounded."

Table 3.2 also allows floating. The consequence is that the charger never pauses for a hot or
cold cell. The bolt's cell has two wires and no thermistor.

**CE tied to VCC is sanctioned** (Table 3.2: "Connected to VCC or MCU control").

## Ports and interfaces

| Port | Kind | Pins | What |
|---|---|---|---|
| `VIN` | supply | U1.4, U1.8 | USB 5 V: VCC, and CE tied high |
| `VBAT` | supply | U1.5 | The cell (+), 4.20 V regulation (`nominal_v` for the pin solver) |
| `GND` | ground | U1.1, U1.3, U1.9 | Ground, the EP, and TEMP (no NTC) |
| `CHRG` | signal | U1.7 | Open drain, low while charging |
| `DONE` | signal | U1.6 | Open drain, low when done |

The interfaces `chrg` and `done` are both `{"type": "logic", "dir": "out", "drive":
"open_drain"}`. The pin solver can put them on any RP2350 GPIO. In a smoke test it put them on
GPIO4/GPIO5, and the result composed. The host enables its internal pull-ups.

Status truth table (p.7):

| State | CHRG | DONE |
|---|---|---|
| Charging | low | Hi-Z |
| Done | Hi-Z | low |
| No battery | blinks | low |
| Undervoltage or temperature fault | Hi-Z | Hi-Z |

**Levels.**
- STATUS has `vin_max` 28 V, the pin's absolute maximum, so any pull-up rail is fine. It has no
  `vol`: the datasheet gives none, only a 5 mA typical LED drive current. A host's VIL
  therefore can't be judged against it. At an internal pull-up's ~66 µA, the low will be near
  0 V.
- CE has `vih` 1.37 V, `vil` 1.16 V (typical, not scaled with VCC) and `vin_max` 28 V.

## Where the bolt deviates from the datasheet

These are kept as built and noted in the claims.

1. **Capacitors below 10 uF effective** (`lgs4056h.vin.cin`, `lgs4056h.vbat.cbat`). C16 and C17
   are 10 uF nominal in a 10 V 0603 X5R part. At 5 V and 4.2 V DC bias such a part keeps much
   less than its nominal value, typically about half. I did not read this part's own curve. The
   nominal-value check passes. A 25 V 0805 part, or two in parallel, would meet the
   requirement.
2. **DONE power-up pulse risk** (`lgs4056h.status.done_no_pulse`). This is unverified. The
   datasheet (p.5, p.7) says DONE must not see a pulse when VCC powers up, and that a pull-up
   to another rail must not pulse then; "the system will not work". On the bolt, plugging in
   USB does three things close together:
   - it powers VCC;
   - through R18 it enables the 3.3 V LDO;
   - once firmware sets the pin up, the RP2350's internal pull-up on DONE turns on.

   So DONE's pull-up rail and pull-up arrive during or just after VCC's rise. The datasheet
   doesn't say what counts as a pulse. Bench-check that charging starts on every plug-in.

## What is verified

- `python3 tools/circuit_check.py packages/lgs4056h/schematic/circuit.LGS4056HDA.json` prints
  `ok: true` for both `--kicad 9` and `--kicad 10`.
- `python3 tools/library_quotes.py --claims packages/lgs4056h/rules.json` finds all 15 quotes.
  `layout.rprog_short` quotes only the first clause of its sentence, because the rest
  interleaves with the other column.
- `ccomp library check` prints `ok: true`, with 15 claims and the two interfaces.
- `ccomp compose` of a one-instance composition works. `ccomp pins` of a loose composition
  (rp2350-battery with this charger in place of the MCP73831, and `chg.chrg` and `chg.done`
  connected to `mcu`) solves them onto GPIOs and composes.
- The LCSC parts were in stock on 2026-10-30.

## What is not

- **No layout.** The p.11 layout rules are only claims.
- **No θJA for the DFN2x2** is in the datasheet.
- **ICC at 18 kohm has no specified tolerance** (see above).
- **VOL of CHRG/DONE is not specified.**
- **The DONE power-up behaviour on the bolt** has not been tested (above).
- **The bolt's `design/parts/r8-rprog.md`** (the cell's 70 mA limit) is not vendored. The cell's
  maximum comes from simple_earring's comment, not a datasheet read here.
