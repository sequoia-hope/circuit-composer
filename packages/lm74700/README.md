# lm74700 0.1.0

Texas Instruments LM74700-Q1: an ideal diode controller that drives an external N-channel
MOSFET. It regulates the MOSFET's forward drop to 20 mV, and turns it off within 0.75 us when
current tries to flow backwards, leaving the body diode to block. It runs from 3.2-65 V and
withstands -65 V. The variant is **LM74700QDBVRQ1** (LCSC C2941042), the 6-pin SOT-23 (DBV) on
a 3000 reel. The package models it **as the RP2350 motor controller rev A uses it**: U29 with
Q5, C79 and C78 in `~/pcb/rp2350-motor-controller/hardware/rev-A` (tag `rev-A-fab`). There,
USB-C VBUS feeds ANODE, and the cathode, Net-(JP1-A), reaches the motor bus VMOT through the
bridged solder jumper JP1. EN is tied to ANODE, so the diode is always on.

Built from the datasheet SNOSD17G (revised December 2020), and the MOSFET's from the Tritech-MOS
TMG08N10SI datasheet. Their hashes are in `sources/sources.json`. Page numbers below are the
LM74700 datasheet's printed pages, which are the same as the PDF pages for pp.1-25.

## Files

| File | What |
|---|---|
| `manifest.toml` | Variant, MOSFET, datasheet revisions, LCSC parts with stock on 2026-11-02 |
| `ports.json` | The six pins (p.3), each with its function, class and limits. EN's levels. No interfaces: EN is tied inside the package |
| `schematic/circuit.LM74700QDBVRQ1.json` | The board's U29, Q5, C79 and C78, by role |
| `rules.json` | 23 cited claims: VCAP, CIN and COUT, EN, ANODE and CATHODE ratings, regulation and reverse blocking, MOSFET selection, layout, TVS, quiescent current, the MOSFET's ratings |
| `sources/sources.json` | Both datasheets: URL, SHA-256, size, date retrieved |

There is no `kicad/` directory. KiCad's `Power_Management:LM74700` and `Transistor_FET:Q_NMOS_GDS`
match the datasheets pin by pin.

## The circuit

| Role | Board ref | Value | LCSC | Why |
|---|---|---|---|---|
| U1 | U29 | LM74700QDBVRQ1 | C2941042 | The controller. The datasheet's typical application (Figure 10-1, p.16) |
| Q1 | Q5 | TMG08N10SI, 100 V 8 A, SOT-89 | C7422896 | The external N-MOSFET: source on ANODE, gate on GATE, drain on CATHODE (p.3). LCSC C7422896 is Tritech-MOS's TMG08N10SI. The board's schematic carries only the LCSC number, and its rev B review names the part |
| CVCAP | C79 | 100 nF 50 V X7R 0402 (CC0402KRX7R9BB104) | C131394 | The charge-pump capacitor, across VCAP and ANODE. At least 0.1 uF, and at least 10 x the MOSFET's Ciss: 2.1 nF here (p.17) |
| COUT | C78 | 1 uF 100 V X7T 0603 (GRM188D72A105KE01D) | C23829972 | At least 100 nF on the cathode (p.17). Rated 100 V because the cathode is on the motor bus. The board's value field reads `1u 100v` |

**Which parts are the package's.**

- **C105 and C106 (2 x 4.7 uF 50 V, VBUS) are the USB connector's, not this block's.** They sit
  beside the USB-C receptacle J3: about 6 mm from J3 and 20 mm from U29. They serve the whole
  VBUS node, which also feeds the PD controller U101 and the VBUS pin of the ESD clamp U11.
  The datasheet's input requirement is only 22 nF minimum (`lm74700.cin.min`), and the board
  meets it with these capacitors. So the package has no CIN role. A board that composes it must
  put at least 22 nF on the `VIN` net, as `tps22913` asks of its VIN.
- **JP1 is the board's.** It is the link from this block's cathode to VMOT, not part of the
  ideal diode. With JP1 open, C78 is the only capacitance on the cathode.
- **No TVS.** The datasheet says a TVS is not needed for the controller to operate
  (`lm74700.tvs.selection`). Rev A has none on VBUS. Rev B adds an SMF24A at J3, which would
  belong to the connector.

**Pins.** All of these were checked against Figure 5-1 and Table 5-1 (p.3), and against the
board's netlist, one pin at a time:

- 1 is VCAP (Net-(U29-VCAP), C79 pin 1).
- 2 is GND.
- 3 is EN. It is tied to ANODE (VBUS), which Table 5-1 allows: "Can be connected to ANODE for
  always ON operation". EN withstands ±65 V.
- 4 is CATHODE (Net-(JP1-A), Q5's drain).
- 5 is GATE (Net-(Q5-G)).
- 6 is ANODE (VBUS, Q5's source).

The 8-pin DDF package numbers its pins differently (p.4) and is not this variant.

**The MOSFET's pins.** The TMG08N10SI datasheet's SOT-89-3L drawing (p.1) has G, D, S along the
leads, with the tab on D. KiCad's `Q_NMOS_GDS` has 1 G, 2 D, 3 S, and pad 2 of `SOT-89-3` is
both the middle lead and the tab. The board's netlist agrees: 1 gate, 2 cathode, 3 VBUS.

**Footprint.** KiCad's `SOT-23-6` has 1.325 x 0.6 mm pads at ±1.1375 mm, on a 0.95 mm pitch. TI's
DBV0006A land pattern (PDF p.34) has 1.1 x 0.6 mm pads on a 2.6 mm span, on the same pitch. The
outer edges agree to within 0.05 mm. KiCad's pads reach 0.27 mm further in.

Ports:

| Port | Kind | Pins | Board net | What |
|---|---|---|---|---|
| `VIN` | supply | U1.6, U1.3, Q1.3, CVCAP.2 | VBUS | The anode: ANODE, EN, the MOSFET's source, and CVCAP's lower plate. 4-60 V operating, ±65 V absolute maximum |
| `VOUT` | supply | U1.4, Q1.2, COUT.1 | Net-(JP1-A) | The cathode: CATHODE, the MOSFET's drain, COUT. It follows VIN less 20 mV, or less I x RDS(on) at high current. 60 V recommended maximum to GND |
| `GND` | ground | U1.2, COUT.2 | GND | The controller's bias return (80 uA). No load current flows through it |

The internal nets are `GATE` (U1.5, Q1.1) and `VCAP` (U1.1, CVCAP.1).

**EN is not brought out.** On the board it is tied to VBUS. A floating EN disables the device
(an internal 3 uA sink pulls it low), so an open optional port would be a trap. A variant that
wants a switched enable can add an `EN` port. Its levels are already in `ports.json`: VIH 2.6 V,
VIL 0.5 V, 65 V maximum.

## The board's reference layout

The rev A layout was measured with `ccomp check ideal-diode`. The results are in
`boards/rp2350-motor-controller/README.md`, section "Bus protection and the LM74700", and in
`boards/rp2350-motor-controller/mined/ideal_diode/` (`lm74700_vbus.check.json`, `.svg`):

- GATE to Q5's gate: 12.4 mm.
- ANODE to Q5's source: 18.6 mm, Kelvin offset 0.
- CATHODE to Q5's drain: 11.6 mm, Kelvin offset 0.
- VCAP → C79 → ANODE: 3.2 mm.

The datasheet asks for the three MOSFET connections to be "close" (`lm74700.layout.sense_at_mosfet`)
and for GATE to be "short" (`lm74700.layout.gate_short`), but it gives no number. The
measurements are therefore recorded, not judged.

## Where the board departs from the datasheet

- **The cathode sits on the motor bus** (`lm74700.cathode.range`, contested). CATHODE is
  recommended up to 60 V to GND, and its absolute maximum is 75 V above ANODE (p.5). Through
  JP1 the cathode is VMOT. The board's README gives VMOT as 60 V maximum continuous, and about
  48 V on rev A. So at a 60 V bus the cathode runs at the top of its range, and surges go past
  it. The board's own review records this as F-27, and rev B replaces U29 and Q5 with an
  LTC4359 and a 4 mohm 100 V MOSFET. The package models rev A as fabricated.
- **The MOSFET's threshold is above the recommended maximum** (`lm74700.mosfet.vth`, contested).
  The datasheet recommends a Vth of 2-2.5 V maximum (p.17). The TMG08N10SI's VGS(th) is 1.0-3.0 V
  (2.0 V typ).
- **The MOSFET is rated 100 V, against "up to 60-V"** (in `lm74700.mosfet.vds`'s note, asserted).
  The datasheet suggests MOSFETs rated up to 60 V, because the controller itself is a 65 V part.
  The real requirement is that VDS withstand the largest differential, which here is the bus
  with USB absent. 100 V meets it.

The MOSFET meets the VGS rating (±20 V against 15 V, `lm74700.mosfet.vgs`) and the body-diode
current requirement (8 A, `lm74700.mosfet.body_diode`). Its RDS(on) (100-130 mohm at 10 V) keeps
the 20 mV regulation for nominal loads of about 0.15-0.5 A (`lm74700.mosfet.rdson`). The
package does not know the board's USB load, so this is not judged. C79 meets the VCAP rule
with a wide margin (`lm74700.vcap.value`), and C78 meets the COUT rule (`lm74700.cout.min`).

**C78 is fitted reversed on the board.** Its pin 1 is on GND and its pin 2 on the cathode. That
makes no difference for a ceramic capacitor. A recomposition of the board states it with
`reversed: ["COUT"]`.

## What is verified

- `python3 tools/circuit_check.py packages/lm74700/schematic/circuit.LM74700QDBVRQ1.json` prints
  `ok: true` with `--kicad 9` and with `--kicad 10`, with no warnings. The symbols and footprints
  exist, and every pin is a pad.
- `python3 tools/library_quotes.py --claims packages/lm74700/rules.json` finds all 19 quotes. The
  other 4 claims cite table values as `figure`: EN levels, the CATHODE range, quiescent current,
  and the MOSFET's ratings.
- `ccomp library check` prints `ok: true` with 23 lm74700 claims and no lm74700 warnings.
- A one-instance composition composes: VBUS with a 4.7 uF input capacitor, the cathode through a
  bridged jumper to a bus connector, and GND. With the board's designators and
  `reversed: ["COUT"]`, all 13 pins of U29, Q5, C79 and C78 land on the same nets as in the
  rev A netlist (`kicad-cli sch export netlist`).
- Stock was checked on 2026-11-02 (see `manifest.toml`). All four parts are extended and in
  stock, with lifecycle normal.

## What is not

- **No layout generated.** Only the board's layout was measured, as described above.
- **The board's USB load current is not known here**, so the RDS(on) window and the MOSFET's
  dissipation are not judged.
- **The start-up inrush** through the body diode into the bus was not quantified. It is set by
  the USB source's current limit.
- **The MOSFET datasheet is undated** and has no revision letter. It is LCSC's copy. Its SOT-89
  drawing (p.5) is an image, so the footprint was checked against the pin letters on p.1 and
  KiCad's SOT-89-3, not against dimensions.
- **The thermal side was not modelled**: neither the controller's θJA of 189.8 C/W, nor the
  MOSFET's 125 C/W.
