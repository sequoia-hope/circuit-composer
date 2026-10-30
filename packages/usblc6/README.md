# usblc6 0.1.0

The USBLC6-2P6 is a two-line ESD protector for USB 2.0 in a SOT-666. Each data line has a pair
of steering diodes, one to a VBUS rail and one to GND, and a TVS from VBUS to GND clamps that
rail. This "rail-to-rail" topology protects D+, D- and VBUS. Each line passes straight over a pin
pair (flow-through): I/O1 on pins 1 and 6, I/O2 on pins 3 and 4. Pin 2 is GND and pin 5 is VBUS.

This package models the part **as the bolt earring uses it** (U5, with C28 on its VBUS).

**The fitted part is not ST's.** LCSC C2827693 is **TECH PUBLIC's** USBLC6-2P6, a second source of
ST's part with the same pinout and topology. ST's own part is LCSC C15999. The package therefore
cites two datasheets:

| Source | What it gives |
|---|---|
| `usblc6_tp_ds`: TECH PUBLIC USBLC6-2P6, 4 pages, PDF dated 2021-08-04 | The fitted part's ratings, characteristics, pin diagram, derating and SOT-666 outline. The PDF is page images with no text layer, so its values were read from a rendering, and claims cite it with `figure` transcriptions, not quotes. It has no application circuit and no layout guidance |
| `usblc6_st_ds`: ST USBLC6-2, Doc ID 11265 Rev 5 (Oct 2011), LCSC's copy | The original part. It is read for the application (CBUS) and the layout rules (§2.2–2.3), which apply here because the pinout and topology are the same |

Page numbers below are printed pages; in both documents they equal the PDF page.

## Files

| File | What |
|---|---|
| `manifest.toml` | Variant, both datasheet revisions, LCSC parts with stock on 2026-10-30 |
| `ports.json` | The six SOT-666 pins and their four functions. No logic levels and no interfaces (see below) |
| `schematic/circuit.USBLC6-2P6.json` | U1 and CBUS by role, the bolt's values |
| `rules.json` | 9 cited claims: second source, flow-through, CBUS, layout (three), clamping, capacitance, VBUS rating |
| `sources/sources.json` | Both datasheets: URL, SHA-256, size, date retrieved |

## The circuit

| Role | Bolt | Value | LCSC | Why |
|---|---|---|---|---|
| U1 | U5 | USBLC6-2P6 (TECH PUBLIC), SOT-666 | C2827693 | The protector. KiCad `Power_Protection:USBLC6-2P6` numbers its pins 1/6 I/O1, 2 GND, 3/4 I/O2, 5 VBUS. That matches the TECH PUBLIC pin diagram (p.1) and ST Figure 1 (p.1), checked pin by pin. KiCad's `Package_TO_SOT_SMD:SOT-666` is identical to the bolt's own footprint apart from UUIDs |
| CBUS | C28 | 100 nF 16 V X7R 0402 | C1525 | ST Figure 18 (p.9) draws "CBUS = 100nF" right at the VBUS pin, and Figure 15 (p.8) repeats it. It gives the steering diodes a low-impedance high rail. It appears only in figures; TECH PUBLIC's datasheet says nothing about it |

Ports:

| Port | Kind | Pins | Bolt net | What |
|---|---|---|---|---|
| `DP` | signal | U1.1, U1.6 | USB_C_DP | Protected line I/O1. The connector's D+ enters on pin 1 and leaves pin 6 toward the device (R6, 27 ohm) |
| `DM` | signal | U1.3, U1.4 | USB_C_DN | Protected line I/O2. D- enters on pin 3 and leaves pin 4 (R7) |
| `VBUS` | supply | U1.5, CBUS.1 | VBUS | The clamp's rail, from the connector's VBUS |
| `GND` | ground | U1.2, CBUS.2 | GND | The clamp's return |

**Pin order.** A port with both pins of a pair cannot say which pin faces the connector.
`usblc6.io.flow_through` records the order: connector → pin 1 (3) → across the package → pin 6
(4) → device, with no stub. The pin solver and router do not enforce it yet.

**No interfaces, no levels.** The part is a passive clamp. No type in `library/interfaces.json`
describes a pass-through protector, because a `usb` interface needs a device or host role. The
I/O functions have kind `esd_io` and class `DIFF_HS`, so that the nets they join take the USB
pair's net class.

## Clamping, capacitance, ratings

These are the TECH PUBLIC figures for the fitted part, with ST's for the original:

| | TECH PUBLIC (fitted) | ST original |
|---|---|---|
| VRWM / VRM | 5 V max | leakage specified at 5.25 V |
| VBR (VBUS–GND, 1 mA) | 6 V min | 6 V min |
| Leakage | 0.08 uA max at 5 V | 150 nA max at 5.25 V |
| Clamp, 1 A 8/20 us | 10 V max | 12 V max |
| Clamp, 5–5.5 A 8/20 us | 14 V max (11 typ) at 5.5 A | 17 V max at 5 A |
| IPP / PPK (8/20 us) | 5.5 A / 80 W | — |
| IEC 61000-4-2 | ±30 kV air and contact | 15 kV air, 8 kV contact |
| Line capacitance | Cj 0.35 typ / 0.5 max pF, I/O–I/O | 2.5 / 3.5 pF I/O–GND; 1.2 / 1.7 pF I/O–I/O |
| Tj | −55 to +125 °C, power derated to ~20 % at 125 °C | −40 to +125 °C |

**Layout matters more than these numbers.** In ST's worked example (§2.2 p.4), 10 mm tracks of
about 6 nH each on the I/O and GND paths raise the positive clamp from 31.2 V to 319.2 V at an
8 kV strike. That makes the layout claims the substance of this package:

| Claim | What it asks | Library claim it links to |
|---|---|---|
| `layout.near_connector` | The protector as close as possible to the connector (§2.3 p.6) | `interface.esd.tvs_near_connector`, `signal.usb2.esd_at_connector` |
| `layout.short_tracks` | The I/O, VBUS and GND tracks as short as possible (§2.3 p.5) | `interface.esd.tvs_ground`, `interface.esd.no_stub_to_tvs`, `interface.protection.two_vias` |
| `io.flow_through` | Each line runs straight across its pin pair, with no stub (Figures 7 and 18) | `interface.esd.no_stub_to_tvs`, `signal.usb2.no_stubs_testpoints` |

## Where the bolt departs from the datasheets

- **VBUS working voltage** (`usblc6.vbus.working_voltage`). TECH PUBLIC rates VRWM at 5 V, but
  USB VBUS may reach 5.25 V. ST's original specifies its leakage at 5.25 V "as specified in USB".
  VBR is at least 6 V, so the part should not conduct at 5.25 V, but its leakage there is
  unspecified. This is a note, not a fix: the bolt keeps C2827693, and C15999 (ST, in stock) is
  the documented alternative.
- **Capacitance** (`usblc6.io.capacitance`, status `contested`). The two makers' figures disagree:
  TECH PUBLIC's 0.5 pF I/O–I/O maximum is about seven times below ST's 1.7 pF for what it presents
  as the same part, and TECH PUBLIC gives no I/O–GND figure. Either value is negligible at the
  RP2350's 12 Mb/s full speed.
- **CBUS** follows ST. The bolt fits it.

## What is verified

- `python3 tools/circuit_check.py packages/usblc6/schematic/circuit.USBLC6-2P6.json` prints
  `ok: true` with `--kicad 9` and with `--kicad 10`.
- `python3 tools/library_quotes.py --claims packages/usblc6/rules.json` finds all 7 quotes, all in
  ST's datasheet. The TECH PUBLIC citations are `figure` transcriptions, because that PDF has no
  text to search. They were read off page renderings and checked once by eye.
- `ccomp library check` prints `ok: true` with these 9 claims loaded. `ccomp interfaces
  packages/usblc6` reports no interfaces, as intended.
- A smoke composition (J1 USB-C, this package, R6/R7 27 ohm) gives `ok: true`, and its nets
  match the bolt's: `USB_C_DP` = J1.A6/B6, U1.1, U1.6, R6.1, and so on.
- LCSC on 2026-10-30: C2827693 is extended, 45609 at JLC, lifecycle normal. C1525 is basic,
  lifecycle normal.

## What is not

- **TECH PUBLIC's values are transcribed, not machine-checked.** No script can search an image
  PDF, so they rest on one reading.
- **ST's guidance is applied to a clone.** It is assumed to carry over because the pinout and
  topology are the same. ST's cited copy is Rev 5 (2011), LCSC's link; st.com timed out, and
  later ST revisions were not read.
- **C28's role is inferred.** The bolt's circuit shows C28 (100 nF) only on VBUS/GND, and the
  README does not say what it is for. Its value matching ST's CBUS is why it is in this package.
  If it was meant as a board-level VBUS decoupler, the package's CBUS duplicates it.
- **No layout, and no topology check.** Nothing yet checks that pin 1 or 3 faces the connector
  on the routed board.
- **No residual-voltage check.** Neither datasheet gives a residual voltage under an IEC strike
  to compare with the RP2350's USB pad ratings. On the bolt, R6/R7 and the chip's own pad
  protection take the rest.
