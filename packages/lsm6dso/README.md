# lsm6dso 0.1.0: ST LSM6DSO 6-axis IMU, I2C mode

The LSM6DSO (3D accelerometer + 3D gyroscope, LGA-14L 2.5 x 3.0 x 0.86 mm) connected the way
the datasheet's Mode 1 figure shows (DS12140 Rev 2, §7.1 Figure 21, p.35). It talks I2C to a
host, and its auxiliary interface is unused. Variant LSM6DSOTR, LCSC C2655100 (extended part,
21241 in JLC stock on 2026-10-27).

| File | What |
|---|---|
| `manifest.toml` | variant, sources, LCSC sourcing with stock |
| `sources/sources.json` | datasheet, ST TN1383 (MEMS PCB guidelines), TI SLVA689A (I2C pull-ups), with hashes |
| `ports.json` | the 14 pins by number, with functions, supplies and net classes; its interfaces: an I2C target at 0x6A or 0x6B, an SPI target (one or the other), INT1, INT2 and the sensor hub's I2C controller |
| `rules.json` | 18 cited claims: supplies, decoupling, I2C, unused pins, land pattern, layout, placement |
| `schematic/circuit.LSM6DSOTR.json` | the circuit: parts by role, ports VDD, GND, SDA, SCL, INT1, INT2 |

## Circuit and the values chosen

| Role | Value | Why |
|---|---|---|
| U1 | LSM6DSOTR | |
| C_VDD (C1) | 100 nF X7R 0402, `near` U1.8 | Figure 21; §7.1 p.35: "as near as possible to the supply pin" |
| C_VDDIO (C2) | 100 nF X7R 0402, `near` U1.5 | the same, on VDDIO |
| R_SDA_PU, R_SCL_PU | 4.7 kohm, optional | see below |

- **One 3.3 V rail for VDD and VDDIO.** VDD may be 1.71-3.6 V and VDDIO 1.62-3.6 V (Table 3
  p.12). The host's IOVDD is 3.3 V, so a single rail needs no level shifting. Each pin still
  has its own capacitor.
- **CS (pin 12) to VDDIO.** This selects I2C (§5.1 p.17).
- **SA0 (pin 1) to GND, address 0x6A** (1101010b, §5.1.1 p.18). That leaves 0x6B free for a
  second LSM6DSx on the bus. The pin has no default pull-up (Table 19 p.38), so it must be tied.
- **SDx and SCx (pins 2, 3) to GND.** These are the unused I2C-master/aux pins. Mode 1 says
  "Connect to VDDIO or GND" (Table 1 p.7).
- **OCS_Aux and SDO_Aux (pins 10, 11) left unconnected but soldered** (Table 1 notes, p.8).
  They are listed in `chip_pins_unconnected`. Both have internal pull-ups in Mode 1 (Table 19).
- **No pull-up on INT1.** INT1 must be low or floating at power-on, or the part starts in
  I3C-only mode (Table 19 note 2 p.38, §5.3 p.24). INT1 and INT2 are push-pull, active high by
  default (CTRL3_C p.51).
- **Pull-ups are 4.7 kohm, not the 10 kohm in ST's figure.** At 400 kHz the rise time must be
  at most 300 ns, so Rp(max) = 300 ns / (0.8473 Cb) (SLVA689A eq. 6, Table 2-1 p.3). That
  allows up to 75 pF of bus capacitance with 4.7 kohm, but only 35 pF with 10 kohm. The
  minimum is (3.3 - 0.4) V / 3 mA = 967 ohm. A board whose bus already has pull-ups omits both
  resistors (instance `omit`). The limits apply to the parallel value of all the pull-ups on
  the bus.

## Symbol and footprint

- **Symbol: `Sensor_Motion:LSM6DSM`.** KiCad 10 has no LSM6DSO symbol. The LSM6DSM symbol
  matches Table 1 pin by pin, in both numbers and functions. The only differences are the
  names SDX and SCX, which the datasheet writes SDx and SCx. LSM6DS3 and LSM6DSL were rejected:
  their pins 10 and 11 are NC.
- **Footprint: `Package_LGA:LGA-14_3x2.5mm_P0.5mm_LayoutBorder3x4y`, checked against Figure 25
  (p.151).**
  - The package pads are 0.475 x 0.25 mm at 0.5 mm pitch, 0.1 mm in from the package edge.
    That puts their centres at x = ±1.1625 and y = ±0.9125, which are exactly the KiCad pad
    centres. Numbering and the pin-1 corner match too.
  - ST's land rule (TN1383 §3.1, taken from TN0018) is pad + 0.1 mm, which gives 0.575 x 0.35
    mm. KiCad's lands are 0.625 x 0.35 mm, so they are 0.05 mm longer than the rule. I
    accepted this and recorded it in `lsm6dso.land.pattern`.
  - KiCad's paste layer equals the land. ST asks for stencil apertures of 70-90 % of the land
    (`lsm6dso.land.stencil`), so a board should set a paste margin of about -0.02 mm.

## Layout and placement (TN1383)

- **Top side:** no tracks, vias or pour under the package. Inner and bottom layers may run
  under it.
- **Traces:** all the same width, leaving each land outward along its long edge,
  symmetrically.
- **Ground:** reached through a trace, not a pour flooded onto the pad.
- **Soldering:** every land is soldered, including both GND pins and the two unconnected pins.
- **Heat and force:** keep away from heat sources, which on this board means the battery,
  charger and buck. Keep at least 10 mm from force points such as the USB-C connector and the
  buttons, between fasteners, and away from unsupported board sections.

## Verified

- `python3 tools/circuit_check.py packages/lsm6dso/schematic/circuit.LSM6DSOTR.json`: ok
  (5 parts, no warnings).
- `ccomp library check`: ok, with this package's 18 claims loaded and no warnings for it.
- Every quote was found verbatim in its PDF's text.
- `ccomp compose` composes it, including an instance with both pull-ups omitted.

## Not verified or open

- **TN0018 was not read.** st.com timed out and Mouser returned an HTML page. The land rule
  comes from TN1383, which says it takes it from TN0018. A newer TN0018 (Rev 8, 2025) exists.
- **The datasheet is Rev 2 (January 2019), from LCSC's link.** ST has published later
  revisions, which have not been checked for changes.
- **`ccomp check package` cannot run on this package yet.** It needs an `intent.json`, which
  this package does not have (by design, the block is two capacitors). The `checks` in
  `rules.json` are therefore written but not yet exercised.
- **Most layout rules are not enforced by any tool** (`enforced_by` says "not yet enforced").
  These are the top-side keep-out, equal trace widths, no pour on the pads, and distances from
  heat and force. ST gives no distance from heat sources.
