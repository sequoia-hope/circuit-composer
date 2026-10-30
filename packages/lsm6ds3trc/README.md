# lsm6ds3trc 0.1.0: ST LSM6DS3TR-C 6-axis IMU, I2C mode, as the bolt earring uses it

The LSM6DS3TR-C (3D accelerometer + 3D gyroscope, LGA-14L 2.5 x 3.0 x 0.86 mm) connected the
way the bolt earring (simple_earring rev 2, U6) has it. That is the datasheet's Mode 1 figure
(DocID030071 Rev 3, §7.1 Figure 15, p.45): I2C to a host, I2C master unused. Variant
LSM6DS3TR-C, LCSC C967633 (extended part, 20813 in JLC stock on 2026-10-30, lifecycle normal).

The LSM6DSO (`packages/lsm6dso/`) is a close sibling with the same package and nearly the same
pins, but every number here is from the LSM6DS3TR-C's own datasheet. Three differences matter:

- **Pins 10 and 11 are NC.** They are not OCS_Aux/SDO_Aux.
- **VDDIO is limited to VDD + 0.1 V.** On the LSM6DSO it may go to 3.6 V independently of VDD.
- **INT1 has no power-on strap.** There is no I3C, and INT1 is simply driven low by default.

| File | What |
|---|---|
| `manifest.toml` | variant, sources, LCSC sourcing with stock |
| `sources/sources.json` | the datasheet, with its hash. TN1383 is cited from `lsm6dso/st_tn1383`, and SLVA689A from the library's `ti_slva689` |
| `ports.json` | the 14 pins by number, with functions, supplies, net classes and logic levels; its interfaces: an I2C target at 0x6A or 0x6B, an SPI target (one or the other), INT1, INT2 and the I2C master |
| `rules.json` | 18 cited claims: supplies, levels, decoupling, I2C, interrupts, unused pins, land pattern, layout, placement |
| `schematic/circuit.LSM6DS3TR-C.json` | the circuit: parts by role, ports VDD, GND, SDA, SCL, INT1, INT2 |

## Circuit and the values chosen

| Role (bolt ref) | Value, LCSC | Why |
|---|---|---|
| U1 (U6) | LSM6DS3TR-C, C967633 | |
| C_VDD (C23) | 100 nF X7R 0402, C1525, `near` U1.8 | Figure 15 C1; §7.1 p.45: "as near as possible to the supply pin" |
| C_VDDIO (C24) | 100 nF X7R 0402, C1525, `near` U1.5 | Figure 15 C2, on VDDIO |
| R_SDA_PU (R14), R_SCL_PU (R15) | 4.7 kohm 1 % 0402, C105871, optional | see below |

The bolt's other 100 nF capacitors on 3V3 belong to other parts (C25 is the flash's, C77 is
U8's, and C1-C4 and C7-C9 come from the MCU cluster). `simple_earring/scripts/circuit.py` labels only C23 and C24 as the
IMU's.

- **One 3.3 V rail for VDD and VDDIO.** VDD may be 1.71-3.6 V. VDDIO may be from 1.62 V up to
  VDD + 0.1 V (Table 4 p.24). One rail always meets that limit, and the host's IOVDD is 3.3 V
  too. Each pin still has its own capacitor.
- **CS (pin 12) to VDDIO.** This selects I2C (§6.1 p.38). With CS tied high, the `spi`
  interface is not available in this circuit.
- **SA0 (pin 1) to GND, address 0x6A** (1101010b, §6.3.1 p.39). The pin has no default
  pull-up (Table 18 p.47), so it must be tied.
- **SDx and SCx (pins 2, 3) to VDDIO, as on the bolt.** Mode 1 says "Connect to VDDIO or GND"
  (Table 2 p.20). They have no pull-up by default, so neither choice draws current. The
  LSM6DSO package ties them to GND instead. Both are allowed.
- **Pins 10 and 11 (NC) left unconnected but soldered** (Table 2 note 2). They have 30-50
  kohm internal pull-ups by default (Table 18 pp.47-48).
- **INT1 and INT2 need no resistor.** Both are driven low by default, and are push-pull, active
  high (CTRL3_C p.63). The bolt uses INT1 (IMU_INT1, wake on motion) and leaves INT2 open.
  INT2 is still a port and an interface. A board that doesn't use it lists it in
  `no_connect`.
- **Pull-ups are 4.7 kohm (the bolt's), not the 10 kohm in ST's figure.** At 400 kHz the rise
  time must be at most 300 ns, so Rp(max) = 300 ns / (0.8473 Cb) (SLVA689A eq. 6). That
  allows up to 75 pF of bus capacitance with 4.7 kohm, but only 35 pF with 10 kohm. The
  minimum is (3.3 - 0.4) V / 3 mA = 967 ohm. A board whose bus already has pull-ups omits both
  resistors (instance `omit`).

## Ports, interfaces and levels

- **Ports:** VDD (supply; VDD, VDDIO, CS, SDx, SCx and the pull-ups' top), GND (ground; pins
  6, 7 and SA0), SDA, SCL, INT1, INT2 (signal).
- **Interfaces:**
  - `i2c`: target, `max_khz` 400, addresses 0x6A (SA0 to GND) or 0x6B (SA0 to VDDIO).
    Requires CS tied to VDDIO, and excludes `spi`.
  - `spi`: target, SCK=SCL, SDI=SDA, SDO=SDO/SA0, CS=CS. Excludes `i2c`.
  - `int1`, `int2`: logic out, push-pull.
  - `aux`: the Mode 2 I2C master on SDx/SCx.
- **Levels:** every I/O function (SDA, SCL, SA0, CS, INT, SDx/SCx) has these levels, from
  Table 4 p.24 and Table 9 p.29. Its supply is VDDIO.
  - VIH = 0.7 x supply, VIL = 0.3 x supply.
  - VOH = supply - 0.2, VOL = 0.2, both at 4 mA.
  - vin_max = supply + 0.3.

## Symbol and footprint

- **Symbol: `Sensor_Motion:LSM6DS3`** (in KiCad 9 and 10). KiCad has no LSM6DS3TR-C symbol.
  LSM6DS3 matches Table 2 pin by pin, including no-connect pins on 10 and 11. Only the case of
  SDX and SCX differs from the datasheet. LSM6DSM was rejected: its pins 10 and 11 are
  OCS_Aux and SDO_Aux.
- **Footprint: `Package_LGA:LGA-14_3x2.5mm_P0.5mm_LayoutBorder3x4y`, checked against Figure 17
  (p.111).**
  - The package pads are 0.475 x 0.25 mm at 0.5 mm pitch, 0.1 mm in from the edge. That puts
    their centres at x = ±1.1625 and y = ±0.9125, which are exactly the KiCad pad centres.
  - KiCad's lands are 0.625 x 0.35 mm. TN1383's rule is pad + 0.1 mm, which gives 0.575 x
    0.35 mm, so KiCad's are 0.05 mm longer. I accepted this (`lsm6ds3trc.land.pattern`).
  - ST asks for stencil apertures of 70-90 % of the land, so a board should set a paste margin
    of about -0.02 mm.
- **The bolt uses a different footprint.** It has EasyEDA's `LGA-14_L3.0-W2.5-P0.50-TL`, with
  0.675 x 0.28 mm lands at x ±1.26, y ±1.01 and the same numbering. Its lands are narrower
  than the rule and extend 0.1 mm past the package. This package does not adopt it.

## Layout and placement (TN1383)

- **Under the part:** nothing on the IMU's own side (the bolt mounts it on the back).
- **Traces:** all the same width, leaving each land outward along its long edge.
- **Ground:** reached through a trace, not a pour flooded onto the pad.
- **Soldering:** every land is soldered, including both GND pins and the NC pins.
- **Heat and force:** keep away from heat sources. Keep at least 10 mm from force points
  (buttons, connectors) where the board allows it.

## Verified

- `python3 tools/circuit_check.py packages/lsm6ds3trc/schematic/circuit.LSM6DS3TR-C.json`
  with `--kicad 9` and `--kicad 10`: ok (5 parts, no warnings).
- `python3 tools/library_quotes.py --claims packages/lsm6ds3trc/rules.json`: all 26 quotes
  found, each on its locator's page.
- `ccomp library check`: ok, with this package's 18 claims loaded and no errors or warnings
  for it. `ccomp interfaces packages/lsm6ds3trc` lists its five interfaces.
- `ccomp compose` composes it, including an instance with both pull-ups omitted. U1's nets
  match the bolt's U6 pin for pin:
  - 1, 6, 7 on GND.
  - 2, 3, 5, 8, 12 on 3V3.
  - 4 on IMU_INT1, 13 on IMU_SCL, 14 on IMU_SDA.
  - 9, 10, 11 open.
- LCSC parts C967633, C1525 and C105871 were checked with `jlc_parts.py show` on 2026-10-30.
  All three have lifecycle normal and are in stock.

## Not verified or open

- **The datasheet is Rev 3 (May 2017), from LCSC's link.** LCSC's PDF lacks the final page
  (115, the legal notice). ST's current revision has not been compared.
- **Table 9 prints the control-pin input range as "0.3 to Vdd_IO +0.3 V".** I read the lower
  limit as -0.3 V.
- **TN0018 was not read.** The land rule comes from TN1383, which takes it from TN0018. The
  same gap is recorded in the lsm6dso package.
- **`ccomp check package` cannot run without an `intent.json`.** The `checks` in `rules.json`
  are therefore written but not yet exercised.
- **Most layout rules are not enforced by any tool.** These are the keep-out under the part,
  equal trace widths, no pour on the pads, and distances from heat and force.
