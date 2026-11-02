# ina240 0.1.0

TI INA240: a current-sense amplifier with a voltage output. Its inputs work from −4 V to 80 V
common mode, independent of its 2.7–5.5 V supply, and it has "enhanced PWM rejection" for the
large common-mode dV/dt of a switching node. That lets it read a motor phase current **in line**,
on a shunt between the half-bridge and the motor, rather than in the low-side source. Two
reference pins set where zero current sits on the output.

The variant is **INA240A1D** (LCSC C1346458): gain 20 V/V, SOIC-8. The package models it **as the
rp2350-motor-controller rev A senses phase A**: U5 across R20 ‖ R23, two 8 mΩ 2512 shunts (4 mΩ),
REF1 on +3V3 and REF2 on GND (a mid-supply reference), with the output A_SENSE going straight to
the RP2350B's ADC0. Phases B and C are wired the same way: U9 across R39 ‖ R40 with C29, and U10
across R55 ‖ R56 with C40. Their nets were compared with this package's ports one by one, and they
match. One phase is modelled; a board instantiates the package once per phase.

Built from TI's datasheet SBOS662C (Rev. C, revised December 2021). Its hash is in
`sources/sources.json`. Page numbers below are the datasheet's printed pages, which are the same
as the PDF pages for pp.1-26.

## Files

| File | What |
|---|---|
| `manifest.toml` | Variant, datasheet revision, LCSC parts with stock on 2026-11-02 |
| `ports.json` | The eight SOIC pins (p.3): function, class, limits. One interface: `out` (analog, to an ADC) |
| `schematic/circuit.INA240A1D.json` | The board's U5, R20, R23 and C16, by role |
| `rules.json` | 15 cited claims: gain, supply, common mode, PWM rejection, reference (two), input polarity, input filter, output swing, capacitive load, bypass, Kelvin, SOIC layout, shunt sizing, NC pin |
| `sources/sources.json` | The datasheet: URL, SHA-256, size, date retrieved, text-layer notes |

There is no `kicad/` directory, because KiCad ships `Amplifier_Current:INA240A1D` (see Pins).

## The circuit

| Role | Board ref | Value | LCSC | Why |
|---|---|---|---|---|
| U1 | U5 | INA240A1D, SOIC-8 | C1346458 | The amplifier. Gain 20 V/V (Table 5-1, p.3) |
| RSHUNT1 | R20 | 8 mΩ 1 % 2512 (Milliohm HoJLR2512-3W-8mR-1%) | C2904240 | Half of the 4 mΩ shunt. Pin 1 is the bridge side, pin 2 the motor side |
| RSHUNT2 | R23 | 8 mΩ 1 % 2512 (same part) | C2904240 | The other half. On the board, both inputs tap this one |
| CBYPASS | C16 | 100 nF 50 V X7R 0603 (YAGEO CC0603KRX7R9BB104) | C14663 | TI recommends 0.1 µF as close as possible to the supply and ground pins (§10.1 p.23). Figure 11-2 (p.25) draws it between pins 6 and 2 |

**Why the shunts are in the package.** The amplifier means nothing without its shunt:
- the gain and the shunt together set the full scale, 80 mV/A here;
- the datasheet sizes the shunt against the gain and what follows the output (§8.3.2 p.11);
- the Kelvin rule (p.24) is a rule about the copper between these two parts;
- TI's own application and layout (Figure 9-3 p.20, Figure 11-2 p.25) draw the shunt as part of
  the circuit.

The ports are therefore the shunt's two ends, not the amplifier's inputs. The board's two shunts
in parallel are two roles, because the board fits two parts. A board that wants one 4 mΩ part
omits RSHUNT2 and changes RSHUNT1's value.

**Which capacitor is U5's.** This was found along the copper, not guessed from the schematic:
`ccomp measure` with a `decoupling` entry for U5 on +3V3 names C16 (100 nF 0603, on B.Cu with U5).
It is 2.22 mm from VS (pin 6) and 3.49 mm from REF1 (pin 7). For U9 the same measurement gives
C29 (4.83 / 3.56 mm), and for U10 it gives C40 (4.23 / 4.35 mm). Each is the same 100 nF part.

**No filters.** Nothing is on the inputs: U5's pins 1 and 8 sit directly on the shunt nets.
The datasheet says that input filters are "not required … and … not recommended" (§9.1.1 p.18).
Nothing is on the output either: A_SENSE has two pins only, U5.5 and the RP2350B's
GPIO40_ADC0 (pin 49). The package therefore has no filter roles. A board that adds an output RC
must keep its capacitor under the 1 nF capacitive-load limit (p.5).

**What stays with the board.** The bridge-side net also carries the half-bridge's MOSFETs, the
EG3113's VS pin, its bootstrap capacitor and the phase-voltage divider (R62). The motor-side net
carries J2 and the phase TVS D9. Those parts belong to the eg3113 package, the board's phase
divider and its connectors, not to this package.

Ports:

| Port | Kind | Pins | Board net (phase A) | What |
|---|---|---|---|---|
| `BRIDGE` | supply | U1.1, RSHUNT1.1, RSHUNT2.1 | /A_P | The shunt's half-bridge end, which is the phase node. IN− is on this side. It is kind `supply` only so that it must be connected: it carries the phase current, and it is not a rail |
| `MOTOR` | supply | U1.8, RSHUNT1.2, RSHUNT2.2 | /C_N | The shunt's motor end. IN+ is on this side |
| `VS` | supply | U1.6, U1.7, CBYPASS.1 | +3V3 | 2.7–5.5 V, 2.4 mA max. REF1 is tied here |
| `GND` | ground | U1.2, U1.3, U1.4, CBYPASS.2 | GND | GND, plus REF2 and NC |
| `OUT` | signal | U1.5 | A_SENSE | 1.65 V at zero current, 80 mV/A |

The board's net names cross over: U5, on bridge A, measures into `/C_N`. The board's README
records this. The package's port names say which end of the shunt is which, whatever the board
calls the nets.

**Pins.** KiCad's `Amplifier_Current:INA240A1D` was checked against Figure 6-2 and Table 6-1
(p.3), one pin at a time:

| Pin | Datasheet | KiCad | Here |
|---|---|---|---|
| 1 | IN− | `-` | BRIDGE |
| 2 | GND | GND | GND |
| 3 | REF2 | REF2 | GND |
| 4 | NC, "Reserved. Connect to ground or leave floating" | GND (passive) | GND, as on the board |
| 5 | OUT | `~` | OUT |
| 6 | VS | V+ | VS |
| 7 | REF1 | REF1 | VS |
| 8 | IN+ | `+` | MOTOR |

The numbers all match. KiCad names pin 4 GND, which is where the board and this package put it,
and the datasheet allows that. The TSSOP (PW) variant numbers its pins differently and is not
modelled.

**Footprint.** The package uses KiCad's `Package_SO:SOIC-8_3.9x4.9mm_P1.27mm`, as the board does
(TI's D0008A, p.33). The 2512 shunts use `Resistor_SMD:R_2512_6332Metric` and the capacitor uses
`Capacitor_SMD:C_0603_1608Metric`, both also the board's.

## Interface and levels

- **`out`** is an `analog` interface with direction out: one signal, OUT, which an ADC input
  serves. Its range is 0.01 V to VS − 0.2 V worst case (p.5). There are no logic levels.
  `ccomp pins` puts three instances' outputs on an RP2350A's ADC0–2 with no warnings.
- **The reference** is the board's mid-supply: one REF pin to VS and the other to GND. That sets
  the output to VS / 2 with no input, ratiometric to the supply (§8.4.3.2 p.14). The RP2350's
  ADC_AVDD is the same +3V3, so the offset tracks the ADC's own reference. REF1 and REF2 are
  interchangeable (p.12). The inline motor example (p.20) swaps them relative to the board, and
  the SOIC layout (Figure 11-2, p.25) wires them exactly as the board does.

## Full scale on the board

G × R = 20 × 4 mΩ = 80 mV/A, centred on 1.65 V. The output swings to within 0.2 V of VS and
10 mV of GND worst case (p.5). That gives about +18.1 A in the direction that raises the output
and −20.5 A in the direction that lowers it. The ideal ±1.65 V would give ±20.6 A, which is the
figure in the board's README (`ina240.output.swing`). At 20 A the pair of shunts dissipates 1.6 W,
which is 0.8 W each if they share the current equally (`ina240.shunt.sizing`).

## Where the board departs from the datasheet

- **Input polarity** (`ina240.input.polarity`, contested). Table 6-1 (p.3) puts IN+ on the
  supply side of the shunt. The inline motor circuit (Figure 9-3, p.20) puts it on the bridge
  side. The board has IN+ on the motor side, on all three phases. Phase current flows both ways,
  so this inverts only the sign of the reading: the output falls when current flows from the
  bridge into the motor. Firmware must use the board's sign. The package keeps the board's
  wiring.

Everything else matches the datasheet's application:
- the mid-supply reference;
- no input filter;
- 100 nF on VS within 2.2–4.8 mm;
- VS at 3.3 V;
- the phase node inside the common-mode range: rev A is limited to about 48 V by its phase TVS,
  and the range goes to 80 V.

## Measured reference layout

The board's sense layout was measured with `ccomp check sense`. The figures are in
`boards/rp2350-motor-controller/README.md`, section "Half-bridges and current sensing", and the
files are in `boards/rp2350-motor-controller/mined/sense/` (sense_a/b/c.check.json and .svg).
Each INA240 sits on B.Cu directly under its shunts:
- the sense loops are 2.66, 7.14 and 3.28 mm²;
- each IN+ lands on its shunt's pad;
- each IN− joins the phase pour 0.24–0.51 mm before its pad;
- both inputs tap one shunt of each pair (R23, R40, R56).

`ina240.layout.kelvin` and `ina240.layout.soic` carry those numbers as evidence, not as a verdict,
because the datasheet gives no tolerance. The decoupling distances above came from
`ccomp measure`.

## Claims

| Claim | What it asks | Library claim it links to |
|---|---|---|
| `part.gain` | 20 V/V (A1), 0.2 % gain error | |
| `vs.range` | VS 2.7–5.5 V, 6 V abs max | |
| `vcm.range` | Inputs −4 to 80 V common mode, independent of VS | |
| `pwm.rejection` | Built for PWM common-mode dV/dt; 50–100 kHz switching | |
| `ref.midsupply` | One REF to VS, one to GND → VS / 2; REF1 and REF2 interchangeable | |
| `ref.within_rails` | REF only between GND and VS, driven directly or from a low impedance | |
| `input.polarity` | IN+ on the supply (bridge) side. **Contested**: the board is reversed | |
| `input.no_filter` | No input filter; if fitted, series R ≤ 10 Ω | refines `half_bridge.sense.filter_at_amp` |
| `output.swing` | Output within VS − 0.2 V and GND + 10 mV | |
| `output.cload` | At most 1 nF on OUT | |
| `vs.bypass` | 0.1 µF as close as possible to VS and GND (value checked as a minimum) | |
| `layout.kelvin` | Kelvin (4-wire) connection to the shunt | supports `half_bridge.sense.kelvin_pair`, `half_bridge.sense.four_pad_below_1mohm` |
| `layout.soic` | TI's SOIC layout (Figure 11-2) | |
| `shunt.sizing` | Shunt sized by full scale, ADC range, gain and dissipation | |
| `nc.pin4` | NC may be grounded or left floating | |

`input.no_filter` refines the library's sense-filter claim and does not contradict it. The
library claim says where RC filters go when a design has them. The INA240's datasheet says that
this amplifier needs none at its inputs.

## What is verified

- `python3 tools/circuit_check.py packages/ina240/schematic/circuit.INA240A1D.json` prints
  `ok: true`, with no warnings, under both `--kicad 9` and `--kicad 10`.
- `python3 tools/library_quotes.py --claims packages/ina240/rules.json` finds all 21 quotes.
  The table rows (output swing, capacitive load) and the figures (9-3, 9-7, 11-2) are cited as
  `figure` descriptions, which were read off renderings.
- `ccomp library check` prints `ok: true` with 15 ina240 claims and no ina240 warnings.
  `ccomp interfaces packages/ina240` lists `out`.
- A three-instance composition composes. Phase connector, motor connector and a header for +3V3,
  GND and the outputs give `ok: true`, with no unconnected ports. The loose version with an
  RP2350A solves the three outputs onto ADC0–2.
- The board's netlist was exported from rev A with kicad-cli. Bound as U5/R20/R23/C16,
  U9/R39/R40/C29 and U10/R55/R56/C40, each phase puts the package's five ports on five distinct
  board nets, with every pin of a port on the same net.
- LCSC, on 2026-11-02, all lifecycle normal:
  - C1346458: extended, 1334 in stock;
  - C2904240: extended, 4355 in stock;
  - C14663: basic.

## What is not

- **`ccomp check package` was not run.** It needs an `intent.json` in the package, which this
  package does not have (neither does ncp730). The wiring was compared with a script instead,
  as described above.
- **The shunt's datasheet was not read.** Its 3 W rating, its TCR and its pad geometry come from
  the LCSC listing (HoJLR2512-3W-8mR-1%), and no claim cites them.
- **How current splits between the two shunts was not measured.** The amplifier reads only the
  tapped shunt's share.
- **The 1 nF capacitive-load limit is in the typical column only** (p.5).
- **The swing-to-GND limit is stated with both REF pins at 0 V.** Applying it at mid-supply is an
  assumption.
- **The board's PWM frequency was not checked** against the 50–100 kHz case the datasheet
  designs for.
- **No generated layout.** The board's measured layout is the reference.
