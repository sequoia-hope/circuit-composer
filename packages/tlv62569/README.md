# tlv62569 0.1.0

A Circuit Composer package for the Texas Instruments TLV62569, a 2 A synchronous step-down
converter. It packages the TLV62569DBV (SOT-23-5) in the datasheet's typical application
(SLVSDG1C Figure 5, p.8), set for a 3.3 V rail fed from a 1S LiPo (3.0-4.2 V) or from USB 5 V
(up to 5.5 V). The milestone 5 board needs up to 600 mA from it.

| File | What |
|---|---|
| `manifest.toml` | Variant, source revisions, LCSC parts with stock on 2026-10-27 |
| `ports.json` | The five DBV pins by number, and each function's class, supply and limits (§5 p.3) |
| `rules.json` | 18 claims: 17 from the datasheet, 1 from the inductor's catalogue. 7 of them carry part-value `checks`, and 11 link to 12 claims in `library/claims/buck.json` |
| `schematic/circuit.TLV62569DBV.json` | The circuit: 8 parts by role, nets SW and FB, ports VIN, VOUT, GND (must be connected) and EN |
| `intent.json` | One `buck` block, `reg`, in the shape of rp2350's `core_reg`: roles, nets, objectives, a constraint, ports |
| `sources/sources.json` | Datasheet and inductor catalogue, with SHA-256 hashes |

The DBV variant has **no power-good pin**. PG is only on the TLV62569PDDC (SOT-23-6) and the
TLV62569PDRL (SOT563) (Device Comparison p.1, Pin Functions p.3).

## The circuit and why

| Role | Figure 5 | Value | Part (LCSC) | Why |
|---|---|---|---|---|
| U1 | — | TLV62569DBV | TLV62569DBVR (C141836) | |
| CIN | C1 | 10 uF 25 V X5R 0805 | CL21A106KAYNNNE (C15850) | The datasheet needs at least 4.7 uF and says more reduces ripple (§8.2.2.5 p.10). 10 uF leaves margin for DC-bias loss up to 5.5 V |
| L1 | L1 | 2.2 uH | SXN SMNR4020-2.2UH (C135262) | 2.2 uH is Table 4's standard value (p.9). The part is shielded, DCR 40 mOhm, rated 3.40 A |
| COUT | C2 | 22 uF 25 V X5R 0805 | CL21A226MAQNNNE (C45783) | Table 4 has 2.2 uH + 22 uF as a checked combination. It leaves margin over the 10 uF standard for DC bias at 3.3 V, and lowers the Power Save Mode overshoot (§7.3.1). The datasheet's range is 10-47 uF (§8.2.2.5) |
| RFB_TOP | R1 | 453 k 1 % | C27009 | VOUT = 0.6 × (1 + R1/R2) (Equation 2, p.9). This gives 3.318 V nominal, and 3.25-3.39 V over VFB's 0.588-0.612 V (p.4) |
| RFB_BOT | R2 | 100 k 1 % | C25741 | The datasheet limits R2 to 200 k at most (§8.2.2.2). The CFF value is given for 100 k. The divider draws 6 uA |
| CFF | C3 | 6.8 pF C0G | C1576 | "6.8-pF capacitance is recommended for R2 of 100-kΩ" (§8.2.2.2), for transient response (Figure 19) |
| R_EN | (none) | 1 M | C26083 | Pulls EN up to VIN (see below) |

**Inductor current.** Equation 3 (p.10) at VIN = 5.5 V gives a ripple dIL of
3.3 × (1 − 3.3/5.5) / (2.2 uH × 1.5 MHz) = 0.40 A. At 600 mA the peak IL,MAX is then 0.80 A,
and the datasheet's 20-30 % margin asks for Isat ≥ 1.04 A. At the IC's full 2 A the same sums
give 2.2 A and a 2.86 A requirement. The SMNR4020-2.2UH's 3.40 A rating covers both, and also
the high-side current limit (3 A typical, p.5). The catalogue does not say whether its "Rated DC
Current" is saturation or heating; LCSC lists it as Isat. The Sunlord SWPA4020S2R2MT
(C83423) would be the better-known choice, but LCSC returns no data for it (lifecycle unknown).

**EN: a port, pulled up.** Figure 5 ties EN straight to VIN. The datasheet requires that EN
"must be terminated and should not be left floating" (§7.4.1 p.7). The package therefore adds
R_EN (1 M to VIN) and exposes EN as a signal port:
- A board can leave EN unconnected and the rail is always on.
- It can pull EN below 0.4 V (a slide switch or an open drain) to shut the rail down. That draws
  4.2 uA through R_EN from a full cell, plus the IC's 2 uA maximum shutdown current (p.4).

The datasheet does not specify EN's input current. The 1 M value assumes it is far below 1 uA,
because VIH maximum is only 1.2 V. A board that wants Figure 5 exactly omits R_EN and ties EN
to VIN.

**Low battery: 100 % duty cycle.** As the cell falls toward VOUT, the converter holds its
high-side switch on (§7.3.2 p.6). It keeps regulating down to
VIN(MIN) = VOUT + IOUT × (RDS(on) + DCR) = 3.318 + 0.6 × (0.100 + 0.040) ≈ 3.40 V at 600 mA
(3.41 V with the DCR at its +30 % limit). Below that, VOUT follows the cell about 84 mV under
it. This continues down to the under-voltage lockout, 2.3 V typical and 2.45 V maximum, falling
(p.4). So at a 3.0 V cell the rail is about 2.9 V.

The rp2350 package records VREG_VIN/VREG_AVDD as 3.135-3.63 V. The rail leaves that range
when the cell drops below about 3.22 V at 600 mA, or a little lower at a lighter load. The board
should therefore stop or warn on a battery reading near 3.3 V, not rely on the UVLO.

RDS(on) is a typical value at VIN = 5 V and 25 °C, and the datasheet gives no curve against
VIN. These dropout figures are estimates.

## Rules and library links

The datasheet's layout guidelines (§10.1 p.13) map onto library claims as follows:
- Parts close, power traces short and wide: supports `buck.hot_loop.cin_first_closest`,
  `buck.out_loop.cout_at_inductor` and `buck.sw.min_area`.
- Capacitor grounds to power GND: supports `buck.ground.solid_plane`.
- FB traces away from SW: supports `buck.fb.away_from_sw_and_inductor`.
- GND layers for shielding: supports `buck.fb.shielded_other_layer`.
- Figure 20's DBV layout supports `buck.fb.sense_at_cout` and `buck.fb.divider_at_ic`.

The CIN and COUT rules support `buck.cap.ceramic_low_esr`, and the inductor part supports
`buck.inductor.shielded`.

Three claims refine library claims:
- the Isat margin refines `buck.inductor.isat_above_peak`;
- the -50 % capacitance allowance built into Table 4 refines `buck.cap.dc_bias_effective`;
- the CFF rule refines `buck.fb.divider_at_ic`.

No claim contradicts the library. Figure 20 puts C1's and C2's grounds a few millimetres apart
on one pour, which sits against `buck.ground.cin_cout_separation` (10-20 mm). The datasheet
states no distance, so this is recorded as a note, not a `contradicts` link.

The datasheet gives no feedback clearance in millimetres. The intent's 0.3 mm is the rp2350
floor, borrowed and marked `soft`.

## What is verified, what is not

Verified:
- `python3 tools/circuit_check.py` passes for `schematic/circuit.TLV62569DBV.json`. The
  symbol `Regulator_Switching:TLV62569DBV` (extends TLV62568DBV) has pins EN 1, GND 2, SW 3,
  VIN 4, FB 5, the same as the datasheet p.3.
- `ccomp library check` passes (ok: true).
- All 15 quotes are found on their pages by `tools/library_quotes.py`.
- `ccomp resolve intent.json` parses.

Not verified:
- **No layout yet.** `ccomp gen buck` cannot place this block today. It takes the FB pin's net
  as the output net, and it has no roles for a divider (see below).
- The inductor footprint is KiCad's `L_Sunlord_SWPA4020S`, not one drawn from the SXN catalogue.
  Its 1.1 × 3.7 mm pads at ±1.5 mm cover SXN's recommended 0.95 × 3.3 mm pads 2.1 mm apart (p.4).
- The capacitors' DC-bias curves were not read, so their effective capacitance at 3.3-5.5 V is
  not quantified.
- Thermal behaviour is unchecked: RθJA is 188.2 °C/W (p.4), and the datasheet has no loss data
  for 3.3 V out from a 3.0-4.2 V input.

## For the buck generator and checker

`ccomp check buck --intent packages/tlv62569/intent.json --block reg` should work on a placed
board as it stands. `crates/verify/src/buck.rs` already takes the output net from the
inductor's other pad. When FB is not on that net, it measures the feedback path from the FB pin
to the nearest pad on the FB net, which is the divider. The run from COUT to RFB_TOP is not
measured.

`ccomp gen buck` (`crates/blockgen/src/buck.rs`) would need these changes:
1. Take `n_out` from the inductor's non-SW pad, not from `fb.net` (line 414). Today the check
   "L1 has no pad on FB" stops it.
2. Add optional roles to `BuckRoles` (`rfb_top`, `rfb_bot`, `cff`), and a step that places them
   beside the FB pin. That step would route FB to the divider (short, clearance-checked), the
   VOUT sense from COUT's pad to RFB_TOP, and RFB_BOT to GND.
3. Handle a two-sided SOT-23 pinout. VIN (4) and FB (5) are on one side, and EN, GND and SW
   (1-3) on the other, so CIN spans the package (VIN and GND are on opposite rows). The search
   band "outside the switch pin's row" and the escape-then-straight-run copper assume one row of
   adjacent pins, as on a QFN.
4. Default to no switch-node keepout (`keepout_layer: null`). This datasheet does not ask for
   one, and the milestone 5 board may be 2-layer.
