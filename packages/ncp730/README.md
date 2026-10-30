# ncp730 0.1.0

onsemi NCP730: a 150 mA CMOS LDO with a 2.7-38 V input and about 2 uA quiescent current. The
variant is **NCP730BMT330TBG** (LCSC C892821). It is the fixed 3.3 V output in WDFN6 2x2,
version B, so it has a power-good output. The package models it **as the bolt earring uses it**:
U3 in `boards/bolt-earring/circuit.json`, which is simple_earring rev 2's "3.3V LDO". There, VSYS
(the cell or USB) feeds the LDO, the LDO feeds the 3V3 rail, and the power latch drives EN on net
PWR_EN.

Built from the datasheet NCP730/D Rev. 7 (December 2025). Its hash is in `sources/sources.json`.
Page numbers below are the datasheet's printed pages, which are the same as the PDF pages for
pp.1-30.

## Files

| File | What |
|---|---|
| `manifest.toml` | Variant, datasheet revision, LCSC parts with stock on 2026-10-30 |
| `ports.json` | The six pins plus the exposed pad (p.1, p.3). Each pin's function, class and limits. EN and PG logic levels. Two interfaces: `en` (logic in) and `pg` (logic out, open drain) |
| `schematic/circuit.NCP730BMT330TBG.json` | The bolt's U3, C18 and C19, by role |
| `rules.json` | 18 cited claims: capacitors, input range and dropout, output accuracy, EN, PG, NC pin, pad, thermal, quiescent current |
| `sources/sources.json` | The datasheet: URL, SHA-256, size, date retrieved |
| `kicad/Regulator_Linear_NCP730.kicad_sym` | The symbol. KiCad has none for the NCP730 |

## The circuit

| Role | Bolt ref | Value | LCSC | Why |
|---|---|---|---|---|
| U1 | U3 | NCP730BMT330TBG | C892821 | The chip, from the datasheet's fixed-output application with PG (Figure 3, p.2) |
| CIN | C18 | 1 uF 25 V X5R 0402 (CL05A105KA5NQNC) | C52923 | IN needs 1 uF or more of X5R or X7R, as close as possible (p.14). The bolt's netlist calls C18 the "NCP730 IN decap" |
| COUT | C19 | 1 uF 25 V X5R 0402 (same part) | C52923 | OUT is stable with 1-100 uF *effective* and 1-200 mohm ESR, close to OUT and GND (p.14). The bolt adds C19 in the same line as C18, with C18's part |

**Which capacitors are the LDO's.** VSYS also carries C29 (10 uF 0603), and the bolt's notes put
it in the power path block, beside D3 and Q2. It is the board's bulk capacitor and stays out of
this package. The 3V3 rail also carries other parts that stay out:

- C11 (4.7 uF), which the bolt's notes call the RP2350 VREG's CIN;
- eleven 100 nF capacitors: the RP2350's IOVDD/DVDD decoupling, plus the decoupling for the
  flash, the IMU (C23, C24) and U8 (C77).

Each belongs to its own chip's package. Only C18 and C19 sit on the LDO's pins for the LDO's sake.

**Pins.** All of these were checked against the WDFN-6 pin description (p.3) and pin assignment
(p.1), one pin at a time:

- 1 is OUT (3V3).
- 2 is NC on every fixed version. The datasheet allows tying it to ground for heat. The bolt
  leaves it open, and so does this package (`chip_pins_unconnected`).
- 3 is GND.
- 4 is EN (PWR_EN).
- 5 is PG on version B. The datasheet says it "could be left unconnected or shorted to GND". The
  bolt leaves it open. This package brings it out as an optional `PG` port, so a board lists it
  in `no_connect`.
- 6 is IN (VSYS).
- EP goes to GND. It is pin 7 here, because KiCad's WSON-6 footprint numbers it that way. The
  bolt's own footprint calls it `EP`.

**Footprint.** The package uses KiCad's `Package_SON:WSON-6-1EP_2x2mm_P0.65mm_EP1x1.6mm`. Its
pitch and pad width match onsemi's recommended footprint (p.32: 0.45 x 0.40 mm pads, 2.30 mm
overall, 1.12 x 1.72 mm EP). Its pads are 0.075 mm shorter, and its EP is 0.12 mm smaller each
way. The bolt used simple_earring's own `WDFN-6-2x2`, which has 0.45 x 0.30 mm pads at ±0.975 mm.

Ports:

| Port | Kind | Pins | Bolt net | What |
|---|---|---|---|---|
| `VIN` | supply | U1.6, CIN.1 | VSYS | 2.7-38 V (40 V absolute maximum) |
| `VOUT` | supply | U1.1, COUT.1 | 3V3 | 3.3 V, 150 mA |
| `GND` | ground | U1.3, U1.7, CIN.2, COUT.2 | GND | Pin 3 and the exposed pad |
| `EN` | signal | U1.4 | PWR_EN | Active high, pulled up internally |
| `PG` | signal | U1.5 | (open) | Optional power-good output, open drain |

## Interfaces and levels

- **`en`** is a logic input. Its levels are in `ports.json` under the `EN` function, with
  `supply` = IN:
  - `vih` = 1.05 V, the rising threshold's maximum;
  - `vil` = 0.5 V, the lowest falling threshold (0.7 V minimum rising threshold − 0.2 V maximum
    hysteresis, p.5);
  - `vin_max` = `supply + 0.3`. EN's absolute maximum is VIN + 0.3 V, not a fixed voltage (p.4).

  An internal pull-up of up to 1 uA turns the LDO **on** if EN floats (p.3, p.5). The pin solver
  was tried with an RP2350 GPIO driving EN. With VSYS given as 2.9-4.95 V it refused the
  connection, because the 3.3 V high is above 2.9 + 0.3 V.
- **`pg`** is a logic output, open drain. `vol` is 0.4 V at 1 mA, and `vin_max` = `supply + 0.3`.
  It goes low below 93 % (typical) of 3.3 V, after a 75-270 us deglitch (p.5).
- **OUT** is a `power_out` with `nominal_v` 3.247-3.373 V. The pin solver uses that range as the
  rail's voltage. It is the worst-case sum of three datasheet limits (p.4):
  - output accuracy, −1 % / +2 % over TJ −40 to 125 °C (±1 % up to 85 °C);
  - line regulation, 0.2 %;
  - load regulation, 0.4 %, which only lowers the output.

  The range does not cover dropout.

## How the bolt uses it

**Input range.** VSYS comes from the cell through the P-FET Q2, which gives about 3.0-4.2 V. From
USB it comes through the silicon diode D3, which gives about 3.7-4.95 V. Either way it is far
inside 2.7-38 V (`ncp730.vin.range`).

**Dropout at the load.** The only guaranteed figure is VDO of 480 mV maximum (290 mV typical) at
150 mA (p.5). The bolt's 3V3 load is tens of mA; its notes give about 17 mA for the MCU. The
typical curves (Figure 15-16, p.7) give about 70-100 mV at 20 mA and 130-170 mV at 50 mA, over
25-125 °C. On the cell, the 3V3 rail therefore leaves regulation once the cell falls below about
3.4 V, and then follows the cell down. Expect about 3.0 V on 3V3 at a 3.1 V cell
(`ncp730.vin.headroom`). This is the design, not a fault. Firmware that scales readings to 3.3 V
should know it.

**EN from the power latch.** PWR_EN is fed from two places:

- the latch output SW_ON, through diode 1 of the BAV99W D2. That gives about VBATT − 0.6 V
  while the button is held or the CPU holds PWR_HOLD high;
- VBUS, through the R18/R17 100k/100k divider. That gives about VBUS/2.

D2's second diode clamps PWR_EN to VBATT + Vf. This keeps EN within VIN + 0.3 V even when USB is
present with a flat cell (`ncp730.en.abs_max`). R17 (100 kohm) holds EN at most 0.1 V against
the 1 uA maximum pull-up when the board is off (`ncp730.en.pulldown`; the limit is 500 kohm
before leakage). The network as a whole belongs to the board's latch block, not to this package:
D2, R17, R18, Q4 and the rest. The package only exposes EN as a port, and states in its rules
what EN needs.

**Thermal.** A worst case of 4.95 V in, 3.3 V out and an assumed 50 mA gives 83 mW. That is
about 6 °C rise at the JEDEC RthJA of 67 °C/W, and still small at double that on the earring's
small board (`ncp730.thermal.pd`).

## Where the bolt deviates from the datasheet

- **COUT is at the bottom of the stable range, or below it** (`ncp730.cout.range`, contested).
  The datasheet's 1 uF is *effective* capacitance (p.4 note 4, p.14), and p.14 warns that 0402
  parts lose capacitance quickly under DC bias. C19 is 1 uF nominal, ±10 %, 0402, at 3.3 V
  bias, so on its own it is below 1 uF effective. The package keeps the bolt's value.

  On the bolt, the rail's other capacitance makes up the difference: C11's 4.7 uF plus the
  100 nF parts, several uF in total. That capacitance is not at the LDO's pins, though. A board
  that composes this package *without* such a rail should fit a larger COUT, such as 2.2-4.7 uF.
- **CIN has the same issue** (`ncp730.cin.min`), at the higher VSYS bias. VSYS has the 10 uF C29
  nearby, which helps.
- **Iq.** The bolt's parts.yaml says "~1uA Iq". The fitted version B is 1.8 uA typical and
  3.0 uA maximum (p.4, `ncp730.iq.version_b`). This is a documentation slip, not a circuit
  problem.

## What is verified

- `python3 tools/circuit_check.py packages/ncp730/schematic/circuit.NCP730BMT330TBG.json` prints
  `ok: true` with `--kicad 9` and with `--kicad 10`, with no warnings. The symbol pins match the
  footprint pads 1-7.
- `kicad-cli sym export svg` loads the package's symbol.
- `python3 tools/library_quotes.py --claims packages/ncp730/rules.json` finds all 18 quotes.
  Quotes that contain µ units elide the unit with "...", because the text layer renders µ as "m".
- `ccomp library check` prints `ok: true` with 18 ncp730 claims and no ncp730 warnings.
  `ccomp interfaces packages/ncp730` lists `en` and `pg`.
- A one-instance composition composes: VSYS, 3V3, GND, and PWR_EN with a 100 k pull-down, with PG
  in `no_connect`. `ccomp pins` also solves an RP2350 GPIO onto `ldo.en`.
- Both LCSC parts were in stock with lifecycle normal on 2026-10-30: U1 is extended (8841), and
  the capacitor is basic.

## What is not

- **No layout.** `ncp730.layout.caps_close` (p.26) has not been generated or measured. Whether
  the bolt has C18 and C19 on U3's layer was not checked.
- **The capacitors' DC-bias derating was not quantified.** Samsung's bias curve for
  CL05A105KA5NQNC was not read.
- **The low-current dropout figures are typical curves read off a plot** (Figures 15-16), not
  limits.
- **The 3V3 load current is assumed**, at 17 mA typical and 50 mA for the thermal case. The
  bolt's real 3V3 budget was not measured.
- **VOUT reverse bias at power-down was not checked.** VOUT's maximum is min(VIN + 0.3 V,
  6.0 V) (`ncp730.vout.abs_max`).
- **The datasheet source is onsemi's China site.** www.onsemi.com refused scripted downloads.
  LCSC's copy is the older Rev. 1 (March 2021), which lacks Figures 15-16 and the fixed
  version's 6.0 V VOUT limit, and it was not used.
