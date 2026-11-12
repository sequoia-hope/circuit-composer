# rp2350-flight-controllers, mined

Seven open-source RP2350 / RP2354 flight-controller PCBs across six repositories, plus one legacy
Altium `.PcbDoc` predecessor (`Kolibri v0.4`) used to test headless Altium-to-KiCad import. None
of the external board, schematic or netlist files are copied into this repository (five of the six
upstream repositories are CERN-OHL-S-2.0 or GPL-3.0); every spec reads from
`~/pcb/flight-controllers/`.

| Board | Upstream & commit | License | MCU | Stackup & size | Parts / nets | Board `sha256` |
|---|---|---|---|---|---|---|
| **OpenFC-Lite-Mini** | `OpenDrone-hw/OpenFC-Lite-Mini` @ `d2df223` (2026-10-25, rev 3.3 fab) | CERN-OHL-S-2.0 | RP2354A (QFN-60) | 6L, 20×20 mm | 131 / 75 (KiCad 10) | `5d8768a47bf8…` |
| **OpenFC-Lite** | `OpenDrone-hw/OpenFC-Lite` @ `def4ab2` (2026-10-25) | CERN-OHL-S-2.0 | RP2354B (QFN-80) | 6L, 30.5×30.5 mm | 164 / 82 (KiCad 10) | `4bdfd53e990f…` |
| **OpenAIO** | `OpenDrone-hw/OpenAIO` @ `986ce1e` (2026-10-16) | CERN-OHL-S-2.0 | RP2354A + 4× AT32F421 + ESP32-C3 | 8L HDI, 25.5×25.5 mm | 343 / 235 (KiCad 10) | `2dff516ec276…` |
| **OpenAIO-Whoop** | `OpenDrone-hw/OpenAIO-Whoop` @ `2d83462` (2026-10-19) | CERN-OHL-S-2.0 | RP2354A + 4× EFM8BB51 + ESP32-PICO-V3 | 8L HDI, 25.5×25.5 mm | 249 / 176 (KiCad 10) | `57fd021d56a7…` |
| **Kolibri v0.5** | `bastian2001/Kolibri-FC-Hardware` @ `3ea2894` (2025-11-10, tested flying) | GPL-3.0 | RP2350A + ESP8685 (Gemini ELRS) | 8L, 30×30 mm | 221 / 145 (KiCad 9) | `9509c2ded692…` |
| **Kolibri v0.6** | `bastian2001/Kolibri-FC-Hardware` @ `3ea2894` (2025-11-10) | GPL-3.0 | RP2350A + ESP8685 (single ELRS) | 8L, 20×20 mm | 201 / 121 (KiCad 9) | `abcbcd32f391…` |
| **Simple-FC** | `someonelse20/Simple-FC` @ `13f0f0b` (2026-11-08) | MIT | RP2350A (QFN-60) | 4L | 73 / 59 (KiCad 10) | `f4a656b13bdd…` |
| *Kolibri v0.4 (Altium)* | `bastian2001/Kolibri-FC-Hardware` @ `3ea2894` (`deprecated/PCB Version 0.4/PCB v0.4.PcbDoc`) | GPL-3.0 | RP2040 + ESP8285 | 8L, Altium `.PcbDoc` | 176 / 122 | `135cd3b22039…` |

Rebuild:

```sh
python3 tools/compare_buck.py boards/rp2350-flight-controllers/core_bucks.spec.json boards/rp2350-flight-controllers/mined/core_bucks
python3 tools/compare_buck.py boards/rp2350-flight-controllers/bec_and_rf_bucks.spec.json boards/rp2350-flight-controllers/mined/bec_and_rf_bucks
python3 tools/compare_buck.py boards/rp2350-flight-controllers/bridges.spec.json boards/rp2350-flight-controllers/mined/bridges
python3 tools/compare_buck.py boards/rp2350-flight-controllers/sense.spec.json boards/rp2350-flight-controllers/mined/sense
python3 tools/compare_buck.py boards/rp2350-flight-controllers/protection.spec.json boards/rp2350-flight-controllers/mined/protection

for b in openfc-lite-mini:OpenFC-Lite-Mini/hardware/OpenFC.kicad_pcb \
         openfc-lite:OpenFC-Lite/hardware/OpenFC.kicad_pcb \
         openaio:OpenAIO/hardware/OpenAIO.kicad_pcb \
         openaio-whoop:OpenAIO-Whoop/hardware/OpenAIO-Whoop.kicad_pcb \
         "kolibri-v05:Kolibri-FC-Hardware/PCB Version 0.5/Kolibri v0.5.kicad_pcb" \
         "kolibri-v06:Kolibri-FC-Hardware/PCB Version 0.6/Kolibri v0.6.kicad_pcb" \
         simple-fc:Simple-FC/hardware/Simple-FC.kicad_pcb; do
  id="${b%%:*}"; rel="${b#*:}"
  ccomp check package "~/pcb/flight-controllers/$rel" --package packages/rp2350 \
    --binding "boards/rp2350-flight-controllers/bindings/$id.rp2350.binding.json" \
    -o "boards/rp2350-flight-controllers/mined/package/$id.rp2350.check.json" || true
  ccomp measure "~/pcb/flight-controllers/$rel" \
    --spec "boards/rp2350-flight-controllers/signals/$id.measure.json" \
    -o "boards/rp2350-flight-controllers/mined/signals/$id.json"
  ccomp recognise board "~/pcb/flight-controllers/$rel" \
    -o "boards/rp2350-flight-controllers/mined/recognised/$id.json" >/dev/null
done
```

## RP2350 / RP2354 core regulators

Measured with `ccomp check buck` (`mined/core_bucks/compare.json` and per-board SVGs):

| Board & parts | Hot loop mm² (placed) | Switch node mm² | Output loop mm² | FB clearance mm | Copper under SW mm² |
|---|---|---|---|---|---|
| **OpenAIO-Whoop** (`U10, L2, C58, C56`) | **0.565** (0.565) | **1.635** | **1.656** | 0.358 | **1.268** |
| **OpenFC-Lite** (`U2, L1, C19, C20`, QFN-80) | 0.739 (0.739) | 2.260 | 2.485 | 0.380 | **2.232** |
| **OpenAIO** (`U2, L1, C15, C16`) | 0.834 (0.834) | 2.332 | 2.360 | 0.385 | **2.340** |
| **OpenFC-Lite-Mini** (`U10, L4, C45, C46`) | 0.845 (0.845) | 2.894 | 2.292 | 0.380 | **2.900** |
| **Kolibri v0.5** (`U1, L1, C13, C12`) | 0.891 (0.891) | 2.583 | 2.465 | 0.380 | **2.578** |
| **Kolibri v0.6** (`U1, L1, C13, C12`) | 0.892 (0.891) | 2.253 | 2.338 | 0.389 | **2.258** |
| **Simple-FC** (`U1, L2, C29, C28`) | 1.031 (0.562) | 2.008 | **7.671** | 0.700 | 0.000 |
| *ref: `packages/rp2350` generated* | *1.025* | *1.904* | *2.343* | *0.415* | *0* |
| *ref: `rp2350-motor-controller` (QFN-80)* | *1.054* | *2.207* | *2.464* | *0.415* | *2.223* |
| *ref: `easy2350` (QFN-60)* | *1.064* | *2.301* | *2.452* | *0.386* | *0* |

### Findings

1. **Six of the seven boards beat `easy2350`'s hot-loop area; `OpenAIO-Whoop` halves it.** Tight
   0402/0201 passives and stacked microvias into the exposed ground pad bring `OpenAIO-Whoop` down
   to 0.565 mm² hot loop and 1.656 mm² output loop, with 0.358 mm feedback clearance (passing
   `>= 0.3 mm`). The other five `OpenDrone-hw` and `Kolibri` layouts cluster tightly at
   0.739–0.892 mm² hot loop and 2.292–2.485 mm² output loop.
2. **Every multi-layer FC with an `In1.Cu` ground plane leaves it solid under `VREG_LX`.** Six of
   seven boards (`OpenDrone-hw` ×4 and `Kolibri` ×2) have 1.27–2.90 mm² of `In1.Cu` ground copper
   directly under the switch node, matching `rp2350-motor-controller` (2.223 mm²) on the open
   dispute between `rp2350.vreg.lx_cutout` (RP2350 datasheet §6.3.8 p.412) and
   `buck.sw.keep_ground_under`.
3. **`Simple-FC` passes `switch_node_underlap_mm2 <= 0` for the wrong reason.** Its `In1.Cu` has no
   continuous ground fill under `U1` (`Simple-FC` routes signals across `In1.Cu`/`In2.Cu` on a
   4-layer stackup and pours `GND` on `B.Cu`), so `C28`'s ground return detours around the package
   and inflates `output_loop_area_mm2` to **7.671 mm²** — more than 3× Raspberry Pi's reference.

## RP2350 package checks and decoupling

Checked against `packages/rp2350@0.2.0` (`mined/package/*.rp2350.check.json`) and `ccomp measure`
decoupling paths (`mined/signals/*.json`). The four `RP2354A`/`RP2354B` boards stack 2 MB of in-package
QSPI flash (`FLASH`, `C_FLASH`, `R_SS_LINK`, `R_SS_PULLUP` unbound by design):

| Board | Variant | Pass / Fail / Unbound | Supply decoupling paths (`IOVDD`, `DVDD`, `ADC`, `VREG`, `OTP/QSPI`) | Component-value & topology findings |
|---|---|---|---|---|
| **OpenAIO-Whoop** | RP2354A | 40 / 6 / 20 | 0.66–3.16 mm | `RFILT = 27R` (spec 33 Ω); `R_USB = 27R` ✓; 12 MHz active CMOS oscillator `X1` on `XIN` (`XOUT` floating); dual-Schottky `D8` (`RB706F-40`) BOOTSEL |
| **Kolibri v0.5** | RP2350A + `W25Q16JVUXIQ` | 51 / 9 / 2 | 0.83–2.91 mm | All passive values match spec (`33R`, `15p`, `27R`, `1k`); `R33–U10` BOOTSEL distance 7.87 mm (`<= 4.1 mm` soft); `RUN` floating |
| **Kolibri v0.6** | RP2350A + `W25Q16JVUXIQ` | 49 / 11 / 2 | 0.72–1.99 mm | Same BOM as v0.5, plus `U1 → R1/R2` USB series resistors moved **30.1–30.6 mm** away (`<= 4.2 mm`) and `/USB.D±` routed **15.6–16.2 mm** over `+5V`/`VCC` |
| **OpenAIO** | RP2354A | 42 / 11 / 10 | 0.67–2.91 mm | `OpenDrone-hw` BOM consolidation: `RFILT = 30R`, `R_USB = 30R`, `CL = 20p`, `R_BOOTSEL = 10k` |
| **OpenFC-Lite-Mini** | RP2354A | 40 / 13 / 10 | 0.38–1.99 mm | Same `OpenDrone-hw` values (`30R`, `30R`, `20p`, `10k`); 1 via on `XIN`/`XOUT` |
| **OpenFC-Lite** | RP2354B (QFN-80) | 42 / 13 / 10 | 0.72–3.86 mm | Same `OpenDrone-hw` values (`30R`, `30R`, `20p`, `10k`); QFN-80 pin translation verified cleanly |
| **Simple-FC** | RP2350A + `W25Q128JVS` | 43 / 13 / 10 | 1.09–2.24 mm (`RUN` 5.92 mm) | **No 27 Ω USB series resistors**; **`CL = 33pF`** on 10 pF crystal; **fitted `R4 = 10K`** on `QSPI_SS` (`= false` DNF rule); `XOUT` 7.75 mm (`<= 3 mm`) |

### Findings

1. **`Simple-FC` has three hard schematic violations of Hardware Design with RP2350.**
   - *Missing USB series termination:* `U1.51`/`U1.52` (`USB_DM`/`USB_DP`) connect directly to USB-C
     receptacle `J1` with no 27 Ω series resistors (`rp2350.usb.series_r`).
   - *Crystal load capacitors >2× rated $C_L$:* `Y1` is an Abracon `ABM8-272-T3` ($C_L = 10\text{ pF}$,
     the exact part Raspberry Pi specifies with 15 pF caps), yet `C1` and `C2` are **33 pF**
     (effective $C_L \approx 16.5\text{ pF} + C_{\text{stray}}$), pulling oscillation frequency low
     and reducing negative-resistance margin, compounded by a 7.75 mm `XOUT → R5` track.
   - *Populated `QSPI_SS` pull-up:* `R4 = 10K` is fitted on `/QSPI_SS` alongside `W25Q128JVS`
     (`U6`), violating `rp2350.qspi.ss_pullup_dnf` (HDG §2.2 specifies DNF unless required).
2. **Three `OpenDrone-hw` boards (`OpenFC-Lite-Mini`, `OpenFC-Lite`, `OpenAIO`) use `10k` for
   `R_BOOTSEL`, which is marginal against RP2350's internal pull-up.** Raspberry Pi specifies
   `1 kΩ` (`rp2350.qspi.bootsel_r`). During boot sampling, RP2350 enables its ~50 kΩ internal
   pull-up on `QSPI_SS`; a `10 kΩ` series resistor to ground forms a divider at
   $3.3\text{ V} \times 10 / (50 + 10) \approx 0.55\text{ V}$ (worse at the 30 kΩ minimum pull-up:
   $0.825\text{ V}$, right on the edge of $V_{IL,\max} = 0.3 \times \text{IOVDD} = 0.99\text{ V}$).
   The same three boards consolidate BOM reels with `30R` for both `RFILT` (spec 33 Ω) and
   `R_USB_DP/DM` (spec 27 Ω, +11 % above nominal Full-Speed source impedance) and `20p` crystal
   load caps.
3. **`Kolibri v0.6`'s 30×30 → 20×20 mm shrink degraded USB placement and return paths.** On
   `Kolibri v0.5`, `R1`/`R2` (`27 Ω`) sit right beside `U1` (`U1 → R1/R2` = 1.9–2.1 mm, passing
   `<= 4.2 mm`) and `J1` is protected by `U8` (`WE-TVS-824015043`). On `v0.6`, `R1`/`R2` moved next
   to the edge connector **30.1 mm and 30.6 mm** from `U1`, `U8` was removed, and `/USB.D+` /
   `/USB.D-` run **15.6–16.2 mm unreferenced** across `+5V` (9.8–11.0 mm) and `VCC` (3.6–4.5 mm)
   power pours instead of ground.
4. **Supply decoupling placement is exemplary across all six 6L/8L flight controllers.** Every
   `IOVDD`, `DVDD`, `ADC_AVDD`, `VREG_VIN` and `USB_OTP_VDD` pin on `OpenFC-Lite-Mini`, `OpenAIO`,
   `OpenAIO-Whoop`, `Kolibri v0.5` and `Kolibri v0.6` reaches a local 100 nF or 4.7 µF MLCC within
   **0.38–3.16 mm** (`3.86 mm` worst case on QFN-80 `OpenFC-Lite` pin 5), comfortably inside the
   package's 3.6–5.2 mm ceilings.

## High-voltage BEC bucks and SX1280 / SX1281 RF bucks

Ten battery-fed 5 V / 10 V BEC synchronous bucks (3S–8S input) and four Semtech `SX1280` / `SX1281`
internal 15 µH DC-DC bucks (`mined/bec_and_rf_bucks/compare.json`):

| Board & rail | Converter & parts | Hot loop mm² (placed) | Switch node mm² | Output loop mm² | FB clearance mm | Copper under SW mm² |
|---|---|---|---|---|---|---|
| **Kolibri v0.5** `+5V` | `LMR38025` (`IC5, L3, C33, C39`) | **1.778** | 10.197 | 7.707 | 0.552 | 8.526 |
| **OpenAIO** `+5V` | `LMR51430` (`U4, L3, C31, C28`) | 2.872 (2.945) | 7.671 | 3.007 | **0.181** | 1.832 |
| **OpenFC-Lite** `+10V` | `LMR51635` (`U3, L2, C24, C35`) | 3.043 | 6.865 | 6.097 | 1.648 | 6.866 |
| **OpenFC-Lite-Mini** `+10V` | `LMR51430` (`U3, L3, C15, C30`) | 3.058 | 8.463 | 6.265 | 0.624 | 6.395 |
| **OpenAIO** `+10V` | `LMR51430` (`U3, L2, C20, C24`) | 3.087 | 6.402 | 4.191 | **0.235** | 0.709 |
| **Kolibri v0.6** `+5V` | `LMR38025` (`IC4, L2, C32, C39`) | 3.315 | 14.922 | 5.766 | 1.250 | 1.383 |
| **Kolibri v0.6** `+10V` | `LMR38025` (`IC5, L3, C33, C42`) | 3.315 | 14.041 | 4.135 | 1.005 | 2.003 |
| **OpenFC-Lite** `+5V` | `LMR51635` (`U4, L3, C30, C38`) | 3.337 | 7.164 | 6.077 | 1.717 | 7.166 |
| **Kolibri v0.5** `+10V` | `LMR38025` (`IC6, L4, C34, C42`) | 4.581 | 9.877 | 7.635 | 0.354 | 8.190 |
| **OpenFC-Lite-Mini** `+5V` | `LMR51430` (`U4, L5, C19, C31`) | **7.239** | 6.383 | 5.885 | 1.225 | 5.086 |
| **Kolibri v0.5** `SX1280 #2` | `SX1280` (`U5, L5, C45, C48`) | **0.745** (0.645) | 1.994 | 1.989 | 0.577 | 1.997 |
| **Kolibri v0.6** `SX1280` | `SX1280` (`U3, L4, C27, C28`) | 0.883 (0.821) | 2.412 | 1.949 | 1.141 | 0.069 |
| **Kolibri v0.5** `SX1280 #1` | `SX1280` (`U3, L2, C27, C28`) | 0.905 (0.804) | 2.405 | 2.458 | 1.241 | 0.274 |
| **OpenAIO** `SX1281` | `SX1281` (`U21, L4, C118, C121`) | **3.618** (0.717) | 1.070 | 3.203 | 1.359 | 1.078 |

### Findings

1. **SOT-23-6 (`LMR51430`/`LMR51635`) and HSOIC-8 (`LMR38025`) BEC hot loops cluster around
   1.8–4.6 mm²**, except `OpenFC-Lite-Mini`'s +5 V rail (`7.239 mm²`), where `C19` sits across a
   board-edge keepout from `U4`.
2. **`OpenAIO` splits its BEC ICs (`B.Cu`) from their inductors (`F.Cu`), pinching feedback
   clearance to 0.181 mm and 0.235 mm.** Putting `U3`/`U4` on `B.Cu` directly underneath `L2`/`L3`
   shrinks the output loops (`3.01` and `4.19 mm²`), but the switch vias pass within 0.18–0.24 mm
   of the feedback networks without an intervening shield plane.
3. **`OpenAIO`'s `SX1281` DC-DC hot loop is 5× its placed geometry (`3.618` vs `0.717 mm²`).**
   `C118` (`VDD_IN` decoupling) is placed 0.8 mm from `U21`, but its ground return detours around a
   dense HDI breakout before rejoining `U21`'s exposed pad. On `OpenAIO-Whoop`, `SX1281` pin 14
   (`DCC_SW`) is left unconnected so the radio runs on its internal LDO, saving the 15 µH inductor
   on a cramped 1S board.

## ESC half-bridges and battery-bus current sensing

`OpenAIO` integrates four 3-phase AM32 ESCs (`AT32F421K8U7` ×4 + Novosense `NSG2065Q` half-bridge
drivers ×12 + 24× `DOY180N03T` MOSFETs), placing each high-side FET on `F.Cu` directly opposite its
low-side FET on `B.Cu` (`mined/bridges/compare.json`). Both `OpenAIO` and `OpenAIO-Whoop` sense
total battery current with a TI `INA186A3` (`mined/sense/compare.json`):

| Bridge (`OpenAIO`) | FETs (`F.Cu` / `B.Cu`), driver, cap | Hot loop mm² (placed) | Phase path mm | Gate loop HS / LS mm² | Gate path HS / LS mm | Bootstrap loop mm² | Driver VCC path mm |
|---|---|---|---|---|---|---|---|
| ESC1 A | `Q3 / Q4, U14, C90` | 1.463 (0.952) | 1.36 | 5.23 / 16.36 | 9.47 / 11.54 | 1.86 | 3.14 |
| ESC1 B | `Q5 / Q6, U14, C91` | 2.228 (1.354) | 1.29 | 6.85 / 11.54 | 7.75 / 9.87 | 0.82 | 3.14 |
| ESC1 C | `Q7 / Q8, U14, C92` | 3.363 (1.496) | 1.41 | 7.68 / 9.97 | 5.99 / 9.49 | 1.29 | 3.14 |
| ESC2 A | `Q9 / Q10, U16, C93` | 1.709 (1.019) | 1.12 | 5.93 / 5.47 | 12.06 / 8.04 | 1.86 | 2.66 |
| ESC2 B | `Q11 / Q12, U16, C94` | 2.013 (1.163) | 1.15 | 5.77 / 2.10 | 8.98 / 5.79 | 0.82 | 2.66 |
| ESC2 C | `Q13 / Q14, U16, C95` | 2.969 (1.298) | 1.10 | 9.42 / 2.47 | 9.58 / 3.47 | 1.30 | 2.66 |
| ESC3 A | `Q15 / Q16, U18, C96` | **1.169** (1.071) | 1.11 | **24.61** / 2.49 | 13.32 / 8.44 | 1.90 | **0.92** |
| ESC3 B | `Q17 / Q18, U18, C97` | 2.089 (1.205) | 1.09 | 9.01 / 7.83 | 8.60 / 10.27 | 0.83 | 0.92 |
| ESC3 C | `Q19 / Q20, U18, C98` | 2.729 (1.371) | 1.11 | 5.11 / 3.70 | 9.42 / 4.37 | 1.31 | 0.92 |
| ESC4 A | `Q21 / Q22, U20, C99` | 3.111 (1.160) | 1.30 | 3.28 / 5.05 | 9.07 / **15.81** | 1.86 | 3.31 |
| ESC4 B | `Q23 / Q24, U20, C100` | 3.066 (1.309) | 0.94 | 2.06 / 5.67 | 7.93 / 9.60 | 0.81 | 3.31 |
| ESC4 C | `Q25 / Q26, U20, C101` | **3.400** (1.448) | 1.28 | 7.49 / 5.30 | 6.18 / 10.13 | 1.30 | 3.31 |

| Current sense | Shunt & package | Sense loop mm² | `+` path mm, Kelvin offset mm | `−` path mm, Kelvin offset mm |
|---|---|---|---|---|
| **OpenAIO:** `U25` (`INA186A3`) | `Rsense2` (`0.2mR`, **2-pad** 2512) | **8.083** | 2.552, **0.861** | 7.569, **2.311** |
| **OpenAIO-Whoop:** `U12` (`INA186A3`) | `R2` (`0.5mR`, **4-pad Kelvin** `HCS1206`) | **1.103** | 0.967, **0.000** | 2.905, **0.000** |

### Findings

1. **Double-sided FET stacking cuts commutation hot loops ~10× below single-sided SO-8 bridges.**
   Every `OpenAIO` half-bridge has its own local ceramic bus capacitor (`C90..C101`) and closes its
   commutation loop vertically through the 8-layer stack in **1.17–3.40 mm²** (`0.95–1.50 mm²`
   placed), versus 25.4–25.5 mm² on `rp2350-motor-controller`'s coplanar PowerPAK SO-8 bridges.
   All twelve `NSG2065Q` drivers also satisfy TI's `<= 5 mm` VCC decoupling rule (`0.92–3.31 mm`).
   The trade-off is gate routing around the dense 25.5 mm perimeter: `ESC3 A`'s high-side gate loop
   stretches to **24.6 mm²** (`13.3 mm` path) and `ESC4 A`'s low-side gate path reaches **15.8 mm**.
2. **`OpenAIO` pairs 30 V MOSFETs (`DOY180N03T`) with a 6S (25.2 V) rating and zero TVS clamping.**
   At full 6S charge (25.2 V), a 30 V $V_{DS,\max}$ rating leaves only **4.8 V** (19 %) of margin
   for commutation overshoot and active braking — despite the tight 1.2–3.4 mm² loops, a 40 V FET
   or bus TVS is standard practice for 6S ESCs. In addition, ceramic bus/decoupling capacitors
   `C90..C106` carry literal placeholder `"C"` values on the PCB instead of capacitance/voltage specs.
3. **`OpenAIO` vs `OpenAIO-Whoop` is a textbook demonstration of the `< 1 mΩ` 4-terminal shunt
   rule.** `OpenAIO` uses a **2-pad** `0.2 mΩ` (`200 µΩ`) 2512 shunt (`Rsense2`), and `U25`'s sense
   lines enter the load-carrying `+BATT` and `ESC_BATT` pours **0.861 mm** (`+`) and **2.311 mm**
   (`−`) before the shunt pads (`8.083 mm²` sense loop). Two millimetres of 2 oz copper pour adds a
   significant, temperature-dependent fraction of 200 µΩ in series with the measurement.
   `OpenAIO-Whoop` switches to a **4-pad Kelvin** `0.5 mΩ` `HCS1206` shunt (`R2`) with net-tie
   sense pads, achieving **0.000 mm Kelvin offset on both legs** and shrinking the sense loop 7.3×
   to **1.103 mm²**.

## Connector and bus protection

**None of the seven flight controllers fits a battery-bus TVS diode**, and **five of seven omit USB
data ESD protection** (`OpenFC-Lite-Mini`, `OpenFC-Lite`, `OpenAIO`, `Kolibri v0.6`, `Simple-FC`).
The two boards with USB ESD arrays were measured with `ccomp check protection`
(`mined/protection/compare.json`):

| Entry | TVS | To TVS mm | Vias before | TVS lead mm | Stub mm | Clamp loop mm² | Ground via mm, count within 1 mm |
|---|---|---|---|---|---|---|---|
| **OpenAIO-Whoop** `J31` `D+` | `D7` (`TPD2EUSB30`) | **0.792** | **1, fails** | 11.81 (`R43`) | **0.792, fails** | **0.684** | 0.0, **25** |
| **OpenAIO-Whoop** `J31` `D−` | `D7` (`TPD2EUSB30`) | **0.476** | **1, fails** | 13.00 (`R42`) | **0.476, fails** | **0.171** | 0.0, **25** |
| **Kolibri v0.5** `J1` `D+` | `U8` (`WE-TVS-824015043`) | 4.931 | 0 ✓ | 10.30 (`R1`) | **3.934, fails** | 5.820 | 0.0, 1 |
| **Kolibri v0.5** `J1` `D−` | `U8` (`WE-TVS-824015043`) | 4.404 | 0 ✓ | 10.17 (`R2`) | **0.000 ✓** | 9.305 | 0.0, 1 |

### Findings

1. **`OpenAIO-Whoop` achieves sub-mm² clamp loops (`0.684` and `0.171 mm²`), but branches up
   through a stacked HDI via.** `J31` is on `B.Cu` and `D7` (`TPD2EUSB30`) sits almost directly
   above it on `F.Cu` (`0.48–0.79 mm` away, with 25 ground vias within 1 mm). Because the USB tracks
   leave `J31` on `B.Cu` toward `U10` while the stacked via branches up to `D7`, the surge sees one
   via (`interface.esd.no_via_before_tvs`) and a `0.48–0.79 mm` stub before `D7`.
2. **`Kolibri v0.5` routes `D−` straight through `U8` (`stub = 0`) but taps `D+` on a 3.93 mm T-branch.**
   Both lines stay on `F.Cu` (`vias_before_tvs = 0`) and reach `U8` more than 10 mm ahead of `R1`/`R2`,
   but `D+` detours off the main trunk on a 3.93 mm stub instead of passing through the pad.

## Signal return paths

Measured across USB, QSPI, IMU/OSD/radio SPI and video nets (`mined/signals/*.json`):

- **`OpenDrone-hw` (`OpenFC-Lite-Mini`, `OpenFC-Lite`, `OpenAIO`, `OpenAIO-Whoop`):** Dedicated
  adjacent `GND` planes keep unreferenced segments confined to brief via/pad antipad crossings
  (`worst_gap_mm <= 0.49 mm` on USB and SPI across all four boards; only `+5V_USB` on `OpenAIO` and
  `/RX/RF_WIFI` coplanar feed on `OpenAIO-Whoop` exceed 1 mm).
- **`Kolibri v0.5` and `v0.6`:** Split internal power pours (`+5V`, `+4V5`, `+10V`, `VCC`, `+3V3`)
  lie directly beneath high-speed signal layers without stitching caps at transitions, leaving
  `4.8–8.6 mm` of gyro/SD SPI over `+4V5`/`+5V`/`+3V3` on `v0.5` and `15.6–16.2 mm` of USB `D±`
  over `+5V`/`VCC` on `v0.6`.
- **`Simple-FC`:** With no continuous inner ground plane on its 4-layer stackup, every measured bus
  runs largely unreferenced: SPI `SCLK`/`MOSI`/`MISO` run **27.3–35.1 mm unreferenced** (`8.4–12.2 mm`
  over `+3V3`), SD SPI runs **18.1–19.4 mm unreferenced** (`worst_gap = 10.1 mm`), and USB `D±` run
  **15.8 mm unreferenced**.

## Altium `.PcbDoc` import (`tools/import_altium.py`)

`tools/import_altium.py` wraps headless `pcbnew` (`PCB_IO_MGR.ALTIUM_DESIGNER` + optional
`ZONE_FILLER`) to convert `.PcbDoc` boards into `.kicad_pcb`. Tested on
`Kolibri-FC-Hardware/deprecated/PCB Version 0.4/PCB v0.4.PcbDoc` (8 copper layers, 176 parts, 122
nets, 2,521 track segments, 16 zones):

- **Survives cleanly for `ccomp`:** Full 8-layer copper stackup (including Altium layer labels
  `GND Top`, `Signal 1/3.3V`, `Power 5/VBat`), `Edge.Cuts` board outline, footprints with reference
  designator and `Value`/comment (`RP2040`, `ESP8285`, `SX1280IMLTRT`, `TPS62160DSGR`), pad numbers
  and pad nets, copper tracks/arcs (including exposed-mask busbars), vias, and copper pours (`--refill`
  reconstructs zone polygons identically enough for `ccomp check buck` and `ccomp recognise board`).
- **Lost without the Altium schematic/libraries:** Schematic-only supplier/MPN parameters (when an
  Altium passive's PCB comment holds `"100nF"` or `"C"`, manufacturer/voltage ratings on the `.SchDoc`
  do not cross over), pin electrical types and pin function names (same limitation as any standalone
  `.kicad_pcb`), Altium query-scoped DRC rules/rooms, and auto-generated net syntax (`NetC4_1` /
  `NetIC1_2` instead of KiCad's `Net-(C4-Pad1)`, which causes `ccomp recognise` to count all 122/122
  nets as user-named).

## What mining these boards changed in the tools

- **KiCad 10 (`20260206`) zone fills (`crates/pcb/src/lib.rs`):** KiCad 10 omits `(net_name "...")`
  on `(zone (net "GND") ...)`, which `kicad_pcb` numbers into `z.net` while leaving `z.net_name`
  empty. `Board::from_pcb` now falls back to `name(z.net)`, restoring plane fills across all five
  KiCad 10 boards.
- **Copperless stencil/mask pads (`crates/pcb/src/lib.rs`):** KiCad 10 `0201` footprints include two
  unnamed `(layers "F.Paste")` stencil apertures; `Board::from_pcb` now skips pads whose expanded
  copper layer set is empty so 2-terminal `0201` passives keep `pads.len() == 2`.
- **Exposed-mask copper tracks (`crates/pcb/src/lib.rs`):** `normalise_track_layers` rewrites
  `(segment ... (layers "B.Cu" "B.Mask"))` and `(arc ... (layers ...))` on exposed-solder power
  busbars (`Kolibri v0.5`/`v0.6` and Altium imports) to their copper `(layer ...)`.
- **Stacked HDI vias (`crates/pcb/src/graph.rs`):** `copper_path` now joins overlapping vias whose
  barrel spans differ (`F.Cu..In1.Cu` microvias landing on `In1.Cu..In6.Cu` buried core vias on
  `OpenAIO` and `OpenAIO-Whoop`) without altering same-span through-hole via clusters.
- **Multi-channel TVS ground selection (`crates/verify/src/protect.rs`):** When `ret` is omitted on
  a multi-channel ESD array (`TPD2EUSB30`, `WE-TVS-824015043`), `protection` now chooses the shared
  connector/TVS net preferring ground names and highest pad fanout instead of taking the first
  shared pad in pin order (which previously picked the sibling USB data line).
- **180°-symmetric 4-pin crystals (`crates/ccomp/src/package_check.rs`):** `swapped_pin` now treats
  4-pad `Y*`/`X*` crystals whose shield pads `2` and `4` share a ground net as 180°-reversible
  (`1 <-> 3`, `2 <-> 4`), handling `Simple-FC`'s rotated `ABM8-272-T3` while excluding 4-pin active
  oscillators (`pad(2) = GND != pad(4) = VDD`).
- **Recogniser families & feedback dividers (`crates/recognise/src/{parts,lib}.rs`):** Narrowed
  `"TLV70"` in `LDO_FAMILIES` to `"TLV700"`, `"TLV702"`, `"TLV707"` so TI's `TLV7031` video sync
  comparator is not misclassified as an LDO; added `RP2354`, `AT32`, `EFM8` and `ESP8685` to
  `MCU_FAMILIES`; and restricted converter `fb` divider attachment to dividers whose top resistor
  sits on that converter's output rail (so `+BATT` ADC dividers into RP2350/RP2354 do not overwrite
  the 1.1 V core buck's `fb` role).

## Recognized, not yet measured

Circuit blocks present across these seven boards that are good candidates for new `ccomp` verifiers
and library claims:

- **Synchronous boost & 4-switch buck-boost converters:** `TPS61022` 1S → +5 V high-current boost on
  `OpenAIO-Whoop` (where the hot switching loop is on the *output* capacitor `Cout + FETs`, inverted
  relative to a buck!) and `TPS631000` 4-switch buck-boost on `Simple-FC` (both input and output
  discontinuous loops).
- **Complementary P+N discrete ESC bridges:** `OpenAIO-Whoop` drives four 1S brushless motors from
  `EFM8BB51` MCUs + dual complementary `AGM210MAP` MOSFET packages without dedicated gate-driver ICs.
- **Analog video OSD & 5.8 GHz VTX:** `AT7456E` SPI OSD (`OpenFC-Lite-Mini`, `OpenFC-Lite`,
  `OpenAIO`), discrete `TLV7031` comparator + `TS5A3159` analog-switch OSD (`Kolibri v0.5`/`v0.6`,
  `OpenAIO-Whoop`), and `RTC6705` + `SE5004L` 5.8 GHz PA + LC harmonic filter (`OpenAIO-Whoop`).
- **2.4 GHz ELRS RF front-ends:** `SX1280`/`SX1281` matching/balun networks, coplanar 50 Ω antenna
  feeds, and `ESP32-C3` / `ESP32-PICO-V3` / `ESP8685` backpack Wi-Fi antennas (`OpenAIO`,
  `OpenAIO-Whoop`, `Kolibri v0.5` dual-radio Gemini, `Kolibri v0.6`).
- **Altium auto-net names in `recognise::names::is_auto`:** Matching `^Net[A-Za-z]+[0-9]+_[0-9]+$`
  alongside KiCad's `Net-(...)` so `ccomp recognise` distinguishes unnamed Altium nets from
  designer-named signals.
