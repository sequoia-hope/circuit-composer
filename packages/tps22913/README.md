# tps22913 0.1.0

Texas Instruments TPS22913B: a 2 A P-MOSFET load switch in a 0.9 mm DSBGA-4 (YZV). It turns
on at a controlled slew rate, discharges its output through 150 ohm when off, and blocks
reverse current whether on or off. The variant is **TPS22913BYZVR** (LCSC C544698), as the
bolt earring fits it: U4, U9 and U10, one switch per LED zone, VSYS in, VLED_L/R/C out, ON from
an RP2350 GPIO.

**B or C?** Both the B and the C have quick output discharge. They differ in rise time: 66 us
(B) against 660 us (C) typical at 3.3 V (Device Comparison Table, p.3). The TPS22912C is the
family member without discharge. The bolt's part is the B.

Built from the datasheet SLVSB49F (revised January 2015). Its hash is in
`sources/sources.json`. Page numbers below are printed pages, which match the PDF up to p.31.

## Files

| File | What |
|---|---|
| `manifest.toml` | Variant, datasheet revision, LCSC parts with stock on 2026-10-30 |
| `ports.json` | The four balls (p.3) and their functions; ON's logic levels; one interface, `on` (logic in) |
| `schematic/circuit.TPS22913BYZVR.json` | The switch as the bolt uses it, by role |
| `rules.json` | 15 cited claims: VIN, CIN, CIN:CL ratio, COUT, rise time, inrush, ON, boot-off, output discharge, reverse current, drop and thermal, layout, off current |
| `sources/sources.json` | The datasheet: URL, SHA-256, size, date retrieved |

## The circuit

| Role | Value | LCSC | Why |
|---|---|---|---|
| U1 | TPS22913BYZVR | C544698 | The switch (bolt U4/U9/U10). KiCad has no TPS22913 symbol. `Power_Management:AP22913CN4`, Diodes' pin-compatible part, has the same balls: A1 OUT, A2 IN, B1 GND, B2 ON. Checked ball by ball against p.3. Its footprint `Package_CSP:WLCSP-4_0.89x0.89mm_Layout2x2_P0.5mm` puts the pads where the bolt's own YZV0004ADAD does |
| COUT | 10 uF 10 V X5R 0603 | C19702 | The bolt's per-zone bulk (C20/C78/C79), placed by VOUT. The datasheet makes CL optional (§11.1.4 p.20) |
| R_ON | 10 kOhm 0402 | C25744 | The bolt's R10/R33/R34. ON must not float (p.3, §11.1.2 p.20). This holds it low, so the zone is off until firmware drives it |

The 100 nF capacitors along each VLED rail belong to the LED chain (`led_chain` recipe, one per
LED or per `cap_every` LEDs), not to this package. VSYS's bulk (C29 10 uF, C18 1 uF) is the
board's.

Ports:

| Port | Kind | Pins | What |
|---|---|---|---|
| `VIN` | supply | U1.A2 | The switched supply, 1.4-5.5 V (bolt: VSYS, 3.0-4.95 V). The board must provide ≥ 1 uF ceramic close by |
| `VOUT` | supply | U1.A1, COUT.1 | The zone rail. Follows VIN less I x rON. Discharged to GND when off |
| `GND` | ground | U1.B1, COUT.2, R_ON.2 | |
| `ON` | signal | U1.B2, R_ON.1 | Enable, active high, pulled down inside the package |

**Interface.** `on` is `logic`, `dir: in`. The ON function's levels are `vih` 1.1, `vil`
`0.4 + 0.2 * (supply >= 3.61)` (supply is VIN; the datasheet gives 0.4 V up to VIN 3.6 V and
0.6 V from 3.61 V) and `vin_max` 5.5. VOUT is `power_out` with no `nominal_v`, because it is not
a regulated rail. A board names the zone rail's voltage in `rails` if it needs one.

**Inrush.** The B rises in 82 us (10-90 %, VIN 5 V) to 97 us (3.3 V) in the switching table
(p.6). A bolt zone is about 11 uF nominal: COUT plus the chain's 100 nF capacitors (ten on VLED_L,
seven on R, six on C). At VSYS 4.95 V that is 48 mV/us and **about 0.53 A** peak, or 0.45 A from
a 4.2 V cell. Both are under the 2 A continuous and 2.5 A pulsed ratings. The real figure is
lower, because the X5R capacitor loses much of its capacitance at 4-5 V bias
(`inrush.current`). `ccomp check package` caps COUT at 33 uF. The datasheet gives rise time
only as typical values.

**Boot.** The pull-down keeps each zone off, with its rail discharged to GND, while the RP2350
is unpowered or booting (`on.boot_off`). On the bolt, VSYS feeds the switches whenever a cell or
USB is present. The MCU's 3V3 comes from an LDO, so the switches can be powered while the MCU
is not. An RP2350 pad has no pull while IOVDD is off, and it comes out of power-up with its
isolation latched (RP2350 datasheet §9.3). Without R_ON, ON would float.

## Where the bolt departs from the datasheet

- **No CIN per switch** (`vin.cin`, contested). The datasheet asks for ≥ 1 uF of ceramic close
  to VIN. The bolt shares VSYS's C29 (10 uF) and C18 (1 uF, the LDO's input capacitor) across
  all three switches. The package has no CIN role. Whether those capacitors sit close enough to
  each switch is a question about the bolt's layout.
- **CL ≥ CIN** (`cap.ratio`, contested). The datasheet strongly recommends CIN > CL, ideally
  10:1. The bolt runs about 1:1 for one zone and about 1:3 with all three zones on. Back-flow when
  VSYS drops is stopped by the always-on reverse-current protection (`vout.reverse_current`).
  The cost is a larger VSYS dip at turn-on. Enabling the zones a few hundred microseconds apart
  limits each step to one zone's inrush, and the C variant would cut the inrush tenfold.
- **10 uF COUT** is above the 0.1-1 uF of the datasheet's design example. That is not a limit,
  and the inrush stays well inside ratings.

## What is verified

- `python3 tools/circuit_check.py packages/tps22913/schematic/circuit.TPS22913BYZVR.json`
  prints ok with `--kicad 9` and with `--kicad 10`. The symbol and footprint exist in both, and
  every pin is a pad.
- `python3 tools/library_quotes.py --claims packages/tps22913/rules.json` finds all 14 quotes,
  one of them in the RP2350 datasheet.
- `ccomp library check` passes with 15 claims and no warning naming this package. `ccomp
  interfaces packages/tps22913` lists `on`.
- `ccomp compose` of one instance works. `ccomp pins` with three instances joined to an RP2350
  (`on` to `mcu`) put them on SIO GPIOs, provided the VSYS rail is given as 3.7-4.95 V.
- Stock was checked on 2026-10-30 (see `manifest.toml`).

## What is not

- **The pin solver refuses RP2350 → ON when VIN can be below 3.61 V or is not given.** The
  RP2350's levels give VOL 0.5 V max, which is at its full drive current (2-12 mA, Table 1683).
  This datasheet's VIL is 0.4 V below VIN 3.61 V. ON draws at most 1 uA, so in practice a GPIO
  driving low sits near 0 V. The worst-case comparison still fails at the bolt's lowest VSYS
  (the cell's 3.0 V). The fix belongs in the RP2350 levels or in the solver (a VOL at light
  load), not in this datasheet's VIL, which is kept as printed (`on.levels`).
- **U1 stock is low.** JLC has 298 of C544698, which is enough for 99 bolts at three per board.
  Alternatives are listed in `manifest.toml`.
- **No layout.** TI's example (§13.2 p.30) is described in `layout.*` but has not been
  generated. TI's land-pattern drawing is not in this PDF (only the package outline, p.36). The
  footprint's 0.235 mm pads come from KiCad's generator for the 0.89 mm WLCSP-4.
- **Rise and turn-on times are typical only.** The datasheet has no minimum, so the fastest
  inrush is not bounded.
- **The LED zones' peak current** is not stated here. It sets the switch's drop and heat
  (`thermal.drop`: 0.11 W at 1 A worst case).
