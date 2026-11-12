# PCBGolf: smaller-silicon research (items 1–7)

Compiled 2026-10-27. Every JLC/LCSC number and stock figure comes from the JLCPCB parts API on that date, and every recommended part is **Extended** (none is Basic). Each section lists the datasheet it cites, with page and table references. Anything marked "unverified" was not confirmed from a primary document. Areas are body areas unless a section says "over leads".

## Summary of recommended substitutions

| # | Original | Recommended | Area saved (approx.) | Main caveat |
|---|---|---|---|---|
| 1 | STM32H725ZGT6, LQFP-144 (22×22 over leads) | **STM32H723VGT6, LQFP-100** (16×16 over leads), C730142, stock 192 | **≈228 mm²**, plus the SMPS 2.2 µH inductor and 3 caps (the H723 is LDO-only) | FDCAN3 moves to PD12/PD13, the PF11 ADC input moves to another ADC pin, and the firmware needs the LDO supply config and an H723 build. VFQFPN68 is ruled out because it has only 2 FDCAN. A G474 in LQFP64 is possible (another ≈112 mm²) but loses SDMMC and needs a full firmware port. |
| 2 | USB2517, QFN-64 9×9 + 24 MHz crystal | **2× WCH CH334P**, QFN-16 3×3, C5373042, cascaded, each with a 12 MHz 2520 crystal | **≈56 mm²**. A CH338F single chip would save ≈63 mm² but has JLC stock 0. | The SMBus configuration is lost (the CH334P has none). |
| 3 | 4× MCP2542FD, TDFN-8 2×3 | **Keep** (no smaller CAN-FD part with VIO and STBY at JLC). A TCAN1046V dual is optional. | ≈0 | — |
| 4 | 4× OPA197, SOIC-8 (bus-activity comparators, 12 V) | **1× OPA4991IRUCR**, X2QFN-14 2×2, 40 V, RRIO, C2878946 | **≈72–114 mm²**, plus 3 caps | 0.4 mm pitch |
| 5 | 4× TPS25942A, WQFN-20 3×4 (12 V, ILIM 28 kΩ = 3.18 A) | **4× TPS259470A**, QFN-10 2×2, 23 V, 5.5 A, C3662799 | **≈32 mm²**, plus 4 resistors | The IMON is shared with ILIM (RILM ≈1.05 kΩ), so the ADC gets ≈0.19 V/A and firmware must rescale. Grounding OVLO is unverified. |
| 6 | 2× AP62300 TSOT26, 3.3 µH 1008, SiS427, SMAJ16CA | AP62300Z6 SOT-563 (C5250745) or TPS62933 SOT-583, 1.5 µH 2016, SiA483ADJ (C5377599), SMF4L16CA SOD-123FL | ≈11 + 3.6 + 6.7 + 6.9 ≈ **28 mm²** | The P-FET's RDS(on) roughly doubles, and the TVS has low stock (3–10 pcs). |
| 7 | 8× QS5K2 TSMT5, 8× bicolour SOT-23 LEDs | DMN3055LFDB U-DFN2020-6 (C5245313); Kingbright APHB1608CGKSURKC 1.6×0.8 (C6506245) | ≈33 + 46 = **79 mm²** | — |

---

## 1. MCU (original: STM32H725ZGT6, LQFP-144)

### Sources

- **[DS13311]** STM32H725xE/G datasheet, DS13311 **Rev 3 (Dec 2021)**, 282 pp. This is the copy LCSC hosts for C6303469: https://datasheet.lcsc.com/datasheet/pdf/35b05cf598d83ac434ea3abee1552505.pdf?productCode=C6303469. ST's own copy is https://www.st.com/resource/en/datasheet/stm32h725ag.pdf, and a web search says it is now at Rev 5 (Nov 2023). st.com blocked the download from here, so Rev 5 was not checked for changes.
- **[DS13313]** STM32H723xE/G datasheet, DS13313 **Rev 1 (Jul 2020)**, from LCSC (C730142): https://datasheet.lcsc.com/datasheet/pdf/2745b8cc1d9d9b0de7348c600b344a0b.pdf?productCode=C730142
- **[DS12288]** STM32G474xB/C/E datasheet, DS12288 **Rev 3 (Apr 2020)**, from LCSC (C529413): https://datasheet.lcsc.com/datasheet/pdf/8312d6f520f4b15a8e387c06044e4d6e.pdf?productCode=C529413
- **[OPD]** ST's machine-readable pin and alternate-function database, STM32_open_pin_data: https://github.com/STMicroelectronics/STM32_open_pin_data/tree/master/mcu. Files used: `STM32H725RGVx.xml` (VFQFPN68), `STM32H725VGTx.xml` (LQFP100), `STM32H725VGHx.xml` (TFBGA100), `STM32H725ZGTx.xml` (LQFP144), `STM32H723VGTx.xml`, `STM32G474R(B-C-E)Tx.xml`, `STM32G474C(B-C-E)Ux.xml`, plus every G473/G474/G483/G484/H523/H533/H562/H563/H7R3/H7S3 file. The pin-to-signal mappings below come from these files.
- **[JLC]** JLCPCB parts API (`selectSmtComponentList`), queried 2026-10-27. Every STM32 listed is an **Extended** part. Stock is the JLC stock on that date.

### 1a. Every STM32H725 package (DS13311 Table 2, pp. 18–21; dimensions from section 7)

| Order code (H725…) | Package | Body (mm) | Pitch | GPIO | FDCAN / TT-FDCAN | SDMMC | USB OTG_HS w/ int. FS PHY | Separate VDD33USB pad | LDO | SMPS | Single-layer escape? |
|---|---|---|---|---|---|---|---|---|---|---|---|
| RE V / RG V | VFQFPN68 | 8×8 (D=E=8.00, Table 125 p.251) | 0.40 (Table 125) | **46** (p.18) | **1/1 = 2 total** (p.19) | 2 | 1 [0/1]: FS PHY only (p.19) | **no** (p.20) | **no**: "VFQFPN68 variant is DCDC only" (p.1) | yes | yes (perimeter + EP) |
| VE T / VG T | LQFP100 | 14×14, 16×16 over leads (Table 126 p.254) | 0.50 | 67 (p.18) | 2/1 = 3 (p.19) | 2 | 1 [1/1] | yes | yes | yes | yes |
| VE H / VG H | TFBGA100 | 8×8 (Table 127 p.258) | 0.80 | 74 | 2/1 = 3 | 2 | 1 [1/1] | yes | yes | yes | **no** (inner balls) |
| ZE T / ZG T (**original**) | LQFP144 | 20×20, 22×22 over leads (Table 131 p.264) | 0.50 | 97 | 2/1 = 3 | 2 | 1 [1/1] | yes | yes (VDDLDO pins in [OPD]) | yes | yes |
| VG Y | WLCSP115 | 3.73×4.15 (Table 129 p.261) | 0.35 | 67 | 2/1 = 3 | 2 | 1 [0/1] | yes | (see note) | yes | **no** |
| AE I / AG I | UFBGA169 | 7×7 (p.1) | 0.50 (p.1) | 121 | 2/1 = 3 | 2 | 1 [1/1] | yes | yes | yes | **no** |
| IE K / IG K | UFBGA176+25 | 10×10 (p.1) | — | 128 | 2/1 = 3 | 2 | 1 [1/1] | yes | — | yes | **no** |
| IE T / IG T | LQFP176 | 24×24 (p.1) | 0.50 | 119 | 2/1 = 3 | 2 | 1 [1/1] | yes | — | yes | yes |

There is **no LQFP-64 and no LQFP-48** for any part in the H723/H725/H730/H733/H735 line. DS13311 Table 2 lists only the eight packages above. DS13313 Table 1 (pp. 18–20) lists only TFBGA100, LQFP100, LQFP144 and UFBGA144 for the H723, and [OPD] has no other package files for the H723/730/733/735.

The LDO row of DS13311 Table 2 (p.20) is ambiguous after text extraction. [OPD] is unambiguous: LQFP100, TFBGA100 and LQFP144 all have VDDLDO pins, and VFQFPN68 has none.

### 1b. Does a smaller perimeter package carry the needed functions? (pin mappings from [OPD])

| Function needed | Original pins (LQFP144) | H725 VFQFPN68 (8×8) | H725 / H723 LQFP100 (14×14) |
|---|---|---|---|
| USB OTG FS (internal PHY) | PA11/PA12 | PA11/PA12 ✓ | PA11/PA12 ✓ |
| SDMMC1 4-bit | PC8–PC12, PD2 | D0 only on **PB13**, because PC8 is not bonded out. CK=PC12, CMD=PD2, D1–D3=PC9–PC11. ✓, but D0 then takes the FDCAN2-alt pin | PC8–PC12, PD2 ✓ (same pins) |
| I2C (I2C2) | PB10/PB11 | **No I2C2.** PB11 is not bonded out. I2C1/3/4/5 exist, e.g. I2C1 on PB6/PB7 or PB8/PB9 | PB10/PB11 ✓ (same pins) |
| FDCAN1 | PB8/PB9 | PB8/PB9 ✓ | PB8/PB9 ✓ |
| FDCAN2 | PB5/PB6, with alternate PB12/PB13 | PB5/PB6 and PB12/PB13 ✓ | PB5/PB6 and PB12/PB13 ✓ |
| FDCAN3 | PG10 (RX) / PG9 (TX) | **Peripheral absent** (only FDCAN1/2, DS13311 p.19; [OPD] IP list) ✗ | **PD12 (RX) / PD13 (TX)**, the only FDCAN3 pins in LQFP100 (no port G) |
| 4 ADC inputs | PA6, PC4, PB1, PF11 | PA6, PC4, PB1 ✓. PF11 is absent, so use any of PA0–PA7, PB0, PC0, PC1, PC5 (14 ADC pins) | PA6, PC4, PB1 ✓. PF11 is absent, so use one of the 16 ADC pins (PA0–7, PB0–1, PC0–5) |
| HSE | PH0/PH1 | ✓ | ✓ |
| Button PE15 | PE15 | absent, move it | H723 LQFP100: PE15 present ✓. H725 LQFP100: **PE15 absent** (the SMPS pins take its place), so move it |
| PDR_ON | pin | no pin | no pin (neither H725 nor H723 LQFP100 lists one in [OPD]). That the PDR is always enabled without the pin is **unverified**; DS13311 p.28 only says "The PDR supervisor can be enabled/disabled through PDR_ON pin". |
| GPIO budget (48 signal pins + SWD) | 97 GPIO | 46 GPIO ✗ (too few even before the FDCAN3 problem) | 67 (H725) / **80 (H723)** GPIO ✓ |

**Shared FDCAN (4 transceivers on 3 FDCANs):** confirmed. [OPD] maps FDCAN2_RX to both PB5 and PB12, and FDCAN2_TX to both PB6 and PB13, on every H72x package. Only one RX pin can feed the peripheral at a time, so the original already runs 3 simultaneous CAN buses and muxes the 4th port onto FDCAN2 by switching alternate functions. LQFP100 keeps this exactly.

**Conclusion:** VFQFPN68 is ruled out. It has only 2 FDCAN controllers and 46 GPIO, has no I2C2, and is DCDC-only. LQFP100 is the smallest H72x package that is perimeter-only and keeps every function. The firmware remaps it needs are FDCAN3 to PD12/PD13, the PF11 ADC channel to another ADCx_INP pin, and on the H725 the button off PE15.

### 1c. LQFP100 candidates at JLC [JLC]

| Part | LCSC | Type | Stock 2026-10-27 | Flash / RAM | Regulator | Notes |
|---|---|---|---|---|---|---|
| **STM32H723VGT6** | **C730142** | Extended | **192** | 1 MB / 564 KB (DS13313 p.1) | **LDO only, no SMPS** (DS13313 Table 1 p.19: "SMPS step-down converter –") | 80 GPIO (Table 1 p.18), FDCAN 2/1 (p.19), 2× SDMMC (p.1). No separate VDD33USB pad on LQFP100 (p.19 row "USB separate supply pad": VGT "–"), so USB runs from VDD and VDD must be 3.3 V. |
| STM32H723VET6 | C730141 | Extended | 529 | 512 KB / 564 KB | LDO only | same pins as above |
| STM32H725VGT6 | C730154 | Extended | **0** | 1 MB / 564 KB | LDO + SMPS | 67 GPIO |
| STM32H725VGT3 | C5271069 | Extended | 1 | same | LDO + SMPS | |
| STM32H730VBT6 | C730157 | Extended | 860 | **128 KB** flash (value line) | LDO + SMPS (assumed same die as H725; unverified) | only usable if the firmware fits in 128 KB plus external flash; unverified |
| STM32H733VGT6 | C730160 | Extended | 1 | 1 MB | LDO only (assumed H723 + crypto; unverified) | |
| STM32H735VGT6 | C730166 | Extended | 2 | 1 MB | LDO + SMPS (assumed) | |
| (orig.) STM32H725ZGT6 | C730156 | Extended | 125 | 1 MB | LDO + SMPS | LQFP-144 |
| (rejected) STM32H725RGV6 | C5271073 | Extended | 11 | | DCDC only | VFQFPN-68, only 2 FDCAN |
| (rejected) STM32H735RGV6 | C5271173 | Extended | 22 | | DCDC only | VFQFPN-68, [OPD] shows FDCAN 2 |

### 1d. Supply configuration and decoupling

| Item | H725 (original, SMPS) | H723 LQFP100 (LDO only) | Source |
|---|---|---|---|
| VCAP | 2× 2.2 µF, ESR < 100 mΩ. A 3rd VCAP pin, if present, is tied to the others with no extra cap. | 2× 2.2 µF (2 VCAP pins in [OPD]) | DS13311 Table 15 p.120; DS13313 VCAP table p.92 |
| SMPS external parts | Cin 4.7 µF on VDDSMPS, Cfilt 220 pF on VLXSMPS, COUT 10 µF on VFBSMPS, **L = 2.2 µH**, Isat ≥ 1.7 A, Irms ≥ 1.4 A | **none** | DS13311 Table 16 p.121 |
| VDDA / VREF+ | 1 µF // 100 nF (×2 if VREF+ is separate) | same | DS13311 Fig. 48/49 p.204 |
| Supply pins (count from [OPD]) | H725ZGT (LQFP144, original): VDD ×13, VDDLDO ×3, VCAP ×3, VDDSMPS, VFBSMPS, VDD33USB, VDD50USB, VDDA, VREF+, VBAT. H725VGT (LQFP100): VDD ×8, VDDLDO ×3, VCAP ×3, VDDSMPS, VFBSMPS, VDD33USB, VDDA, VREF+, VBAT | H723VGT: VDD ×5, VCAP ×2, VDDA, VREF+, VBAT | [OPD] |
| Per-VDD-pin 100 nF plus one bulk 4.7 µF | ST's usual guideline, given in AN5419. **Unverified here**: st.com timed out on AN5419. | | AN5419 (not retrieved) |

The H723 drops the SMPS inductor and its 3 caps. It has fewer supply pins (18 non-I/O pins, against 31 on the H725 LQFP100 and 45 on the H725 LQFP144, all from [OPD]), so it needs roughly 5 fewer 100 nF caps. The trade-off is LDO dissipation instead of SMPS at 550 MHz. DS13311/DS13313 Table "Supply voltage and maximum temperature configuration" (DS13311 Table 14 p.119) sets the junction-temperature limits per VOS level and was **not checked** for the LDO case.

### 1e. Non-H7 options

| MCU | FDCAN | SDMMC | USB | Smallest perimeter pkg with enough I/O | GPIO | Verdict | Source |
|---|---|---|---|---|---|---|---|
| STM32G474/G473/G483/G484 | **3** | **none** (SD card only over SPI) | FS device, crystal-less via HSI48 + CRS | **LQFP64, 10×10 body, 12×12 over leads** (DS12288 Table 118 p.217). UFQFPN48 7×7 has only 42 GPIO. | 52 (LQFP64), 42 (UFQFPN48), 38 (LQFP48) | Possible but tight. With SD over SPI (4 pins instead of 6) the budget is 46 pins + 2 SWD = 48 of 52. RAM drops from 564 KB to 128 KB and the core from 550 to 170 MHz (DS12288 p.1, Table 2 pp.15–16). USB_DM/DP (PA11/PA12) collide with FDCAN1's default pins, so FDCAN1 moves to PB8-BOOT0/PB9 and FDCAN3 goes to PA8/PA15 or PB3/PB4 ([OPD]). Whether the firmware fits is **unverified**. 4-bit SD is lost. | DS12288 Table 2 pp.15–16; [OPD] |
| STM32G474 at JLC | — | — | — | STM32G474RBT6 **C529413** (Extended, stock 760, 128 KB flash). STM32G473RCT6 **C529361** (Extended, stock 1991, 256 KB). STM32G474RET6 C521608 has stock 0. | | | [JLC] |
| STM32H523/H533 | 2 | 1 (LQFP64+) | FS | LQFP64 / UFQFPN48 | 49 / 35 | ✗ only 2 FDCAN | [OPD] |
| STM32H562 | 1 | 1 | FS | VFQFPN68 | 53 | ✗ | [OPD] |
| STM32H563 | 2 | 1–2 | FS | VFQFPN68 8×8 | 53 | ✗ only 2 FDCAN | [OPD] |
| STM32H7R3/H7S3 | 2 | 0 in VFQFPN68, 1 in LQFP100 | FS + HS | VFQFPN68 / LQFP100 | 45 / 67 | ✗ only 2 FDCAN | [OPD] |

### 1f. Recommendation

**STM32H723VGT6 (LQFP-100, C730142, Extended, in stock)** keeps the same core, flash, RAM, 3 FDCANs, SDMMC1 4-bit, USB OTG FS and I2C2, all on the same pins apart from FDCAN3 (PD12/PD13) and the PF11 ADC input. It drops the SMPS inductor and 3 SMPS caps.

- Footprint over leads goes from 22×22 = **484 mm² to 16×16 = 256 mm²**, a saving of **≈228 mm²**, before counting the SMPS parts.
- The firmware changes are the pin remap, LDO supply configuration (`PWR_LDO_SUPPLY` instead of SMPS) and the H723 device define. That the panda firmware builds unchanged for H723 is **unverified**.
- The G474 in LQFP64 (144 mm² over leads, about another 112 mm² saved) works only if SD over SPI and 128 KB RAM are acceptable. It needs a full firmware port, so it is high-risk.

---

## 2. USB hub (original: Microchip USB2517, QFN-64 9x9, 7 ports, 5 used, SMBus-configured)

Every JLC/LCSC number, package and stock figure below comes from the JLCPCB parts search API (`jlcpcb.com/api/overseas-pcb-order/v1/shoppingCart/smtGood/selectSmtComponentList`), queried 2026-10-27. Each part also has a page at `https://www.lcsc.com/product-detail/<LCSC#>.html`. All hub parts listed are **Extended** parts at JLC; none are Basic. Page numbers are PDF page numbers.

### Datasheets used
- [CH334] WCH CH334/CH335 datasheet V2.4 (Chinese), LCSC mirror: https://atta.szlcsc.com/upload/public/pdf/source/20240313/2136C33A6EB724334E520DEFC33CFBA6.pdf
- [CH338] WCH CH338 manual V1.2: https://atta.szlcsc.com/upload/public/pdf/source/20250604/33B7EB63DA58DDDA601A5A5911A2B220.pdf
- [CH339] WCH CH339 manual V1.2: https://atta.szlcsc.com/upload/public/pdf/source/20250604/8965FF1FBFF5B77999A4EFBE83813D66.pdf
- [USB2517] Microchip USB2517/USB2517I datasheet (LCSC copy for C478081): https://wmsc.lcsc.com/wmsc/upload/file/pdf/v2/lcsc/2103091533_Microchip-Tech-USB2517I-JZX_C478081.pdf
- [USB2514B] Microchip USB251xB/xBi datasheet (LCSC copy for C633339): https://wmsc.lcsc.com/wmsc/upload/file/pdf/v2/lcsc/2111161830_Microchip-Tech-USB2514BI-AEZG_C633339.pdf
- [FE1.1s] Terminus FE1.1s datasheet (LCSC copy for C6776948): https://wmsc.lcsc.com/wmsc/upload/file/pdf/v2/lcsc/2306141021_Terminus-Tech-FE1-1S-BQFN24B_C6776948.pdf

### Candidates

| Part | Ports (DS) | Package / body / pitch | Crystal | LCSC # (stock 2026-10-27) | Notes |
|---|---|---|---|---|---|
| **USB2517 (baseline)** | 7, MTT [USB2517 p1] | QFN-64, 9x9 mm [USB2517 p1]; 0.5 mm pitch per the standard 64-QFN 9x9 outline (pitch not re-read from the drawing, so unverified) | 24 MHz crystal or external clock required [USB2517 p1, pin table "XTAL1/XTAL2"] | C1521556 USB2517-JZX (3353); C478081 USB2517I-JZX (1552) | SMBus slave port [USB2517 p1]; internal 1.8 V regulator [p1] |
| **CH338F** | 7 downstream, STT/MTT [CH338 p1-2 §1-2] | QFN-32, 4x4 mm, 0.4 mm pitch [CH338 p3, Table 3-1] | XI/XO crystal pins [CH338 p4, Table 4-1]. The F-variant reference circuit has a crystal (X1) [CH338 p21, Fig 8-1]. "Some applications" can run crystal-free [p2]. **Crystal frequency is not stated in the manual (unverified; CH334 uses 12 MHz).** | **C42387425 (stock 0)**. CH338L LQFP-48 7x7 C42380708 (1763). CH338X QFN-64 9x9 C42387265 (180) | Single 3.3 V supply on the F variant [CH338 p2, Table 1-1]. SMBus config at address 0x2C [p12 §5.6]. Ganged power/overcurrent only, no LEDs on the F variant [Table 1-1]. Decoupling: VDD33 x2 (1 µF), AVDDK, DVDDK, so 4 caps [p4, Table 4-1] |
| **CH334P** | 4, MTT [CH334 p1, p3 Table 1-1] | QFN-16, 3x3 mm, 0.5 mm pitch [CH334 p3, Table 1-2] | 12 MHz crystal [CH334 p1]. Crystal-free is "optional" for CH334P and must be confirmed when ordering. To run without one, leave XO open and tie XI to GND [CH334 p3 Table 1-1, p25 §6.2]. **The JLC stock is presumably the default variant, which needs a crystal (unverified).** | **C5373042 (43087)** | No PWREN/OVCUR, no EEPROM/SMBus config [Table 1-1]. That is fine here because the MCU drives the eFuse enables. Has an internal 5 V to 3.3 V LDO [p1]. With an on-board 3.3 V rail, tie V5 and VDD33 together and use one 1 µF cap [p26 §6.4] |
| CH335J | 4 | QFN-12, 2x2 mm, 0.4 mm pitch [CH334 p3, Table 1-2] | **Crystal-free by default** [CH334 p25 §6.2] | Not listed at JLC. A "CH335P QFN-16C(2x2)" C50083477 is listed with stock 0, and that suffix is not in the V2.4 datasheet (unverified) | Would be the smallest option (2 x 4 mm² with no crystal) if it were stocked |
| CH334F | 4 | QFN-24, 4x4 mm, 0.5 mm pitch [CH334 p3] | 12 MHz, crystal-free optional [p3, p25] | C5187527 (4083) | Larger than CH334P with no benefit here |
| CH335F | 4 | QFN-28, 4x4 mm, 0.4 mm pitch [CH334 p3] | as above | C5137013 (5799) | |
| CH339F | 4 (the CH339W variant has 7) [CH339 p2 Table 1-1] | QFN-32, 4x4 mm, 0.4 mm pitch [CH339 p3 Table 3-1] | crystal; crystal-free possible in non-Ethernet use [CH339 p2] | C48686443 (946) | Multi-function hub. CH339W is QFN-68 8x8, C25503971 (73) |
| FE1.1s | 4, **single TT only** [FE1.1s p1] | WQFN-24, 4x4 mm, 0.5 mm pitch [FE1.1s p6, p20] | 12 MHz crystal [FE1.1s p1, pin table] | C6776948 (17721) | Larger than CH334P and STT only |
| USB2514B | 4, MTT [USB2514B p1] | SQFN-36, 6x6 mm [USB2514B p1] | 24 MHz [p1, p44 §7.1] | C16251 (10639) and others | Too large |
| GL850G | 4 | SSOP-28 / QFN-28 5x5 (from the JLC listing only, datasheet not checked) | 12 MHz (unverified) | C136617 SSOP-28 (11062). The QFN parts all show stock 0 | Too large |

Crystal options at JLC: a 12 MHz 2520 crystal, C521567 XYDBPCNANF-12MHZ (8445 in stock, 2.5x2.0 mm, 12 pF). No 12 MHz crystal in 2016 size was found in stock. A 24 MHz 2016 crystal, C2682776 X201624MKB4SI (8082 in stock, 2.0x1.6 mm).

### Area comparison (IC body plus crystal body; load and decoupling caps not counted)

| Option | Ports | Area mm² | Saved vs baseline | JLC availability |
|---|---|---|---|---|
| USB2517 + 24 MHz 2016 xtal (baseline) | 7 | 81 + 3.2 = **84** | – | in stock |
| **CH338F + 2520 xtal** | 7 | 16 + 5 = **21** | **~63** | **stock 0** (needs global sourcing or consignment) |
| **2x CH334P cascaded + 2x 2520 xtal** | 4+4-1 = 7 | 18 + 10 = **28** | **~56** | in stock (43k) |
| 2x CH334P, crystal-free variant ordered | 7 | **18** | ~66 | variant must be confirmed with WCH; not verified at JLC |
| 2x CH335J (crystal-free default) | 7 | **8** | ~76 | not at JLC |
| 2x FE1.1s + 2 xtal | 7 | 32 + 10 = 42 | ~42 | in stock |

Whether two CH334Ps can share one crystal (by feeding hub 2's XI from an oscillator) is unverified. The datasheet only says the device accepts an external 12 MHz input [CH334 p1].

**Recommendation:** use 2x CH334P (QFN-16 3x3, C5373042) in cascade. Hub 1 port 4 feeds hub 2, which gives 7 ports for the 5 needed. Each hub uses a 12 MHz 2520 crystal. This saves about 56 mm² and the parts are in stock. If CH338F (QFN-32 4x4, 7 ports, SMBus at 0x2C) comes back into stock, it is the better single-chip choice, saving about 63 mm², with fewer parts and an SMBus interface like the original. The 0.5 mm pitch and 16 pins of CH334P are also easier to escape on a 1-layer board than the 0.4 mm pitch QFN-32 of CH338F. The original firmware's SMBus configuration of USB2517 has to be dropped with CH334P, which has no SMBus [Table 1-1]. The MCU only needs default hub behaviour.

---

## 3. CAN-FD transceivers (original: 4x Microchip MCP2542FD, TDFN-8 2x3, VDD 5 V, VIO 3.3 V, STBY)

The JLC data comes from the same API query on 2026-10-27. I listed every in-stock part in JLC's "CAN Transceivers" category (about 1200 parts) and filtered out SOIC/SO/DIP packages. All parts below are **Extended**.

### Datasheets used
- [MCP2542] Microchip MCP2542FD/4FD, MCP2542WFD/4WFD, DS20005514C: https://ww1.microchip.com/downloads/en/DeviceDoc/MCP2542FD-MCP2542WFD-4WFD-Data-Sheet-DS20005514C.pdf
- [TCAN1046V] TI TCAN1046V-Q1, SLLSF70C: https://www.ti.com/lit/ds/symlink/tcan1046v-q1.pdf
- [TCAN1044A] TI TCAN1044A-Q1, SLLSFJ3D: https://www.ti.com/lit/ds/symlink/tcan1044a-q1.pdf

### Candidates

| Part | Ch | Package / body | VIO pin? | Standby pin? | FD rate | LCSC # (stock) | Area/ch mm² |
|---|---|---|---|---|---|---|---|
| **MCP2542FD (baseline)** | 1 | TDFN-8, 2x3x0.8 mm [MCP2542 p28, package marking; p1 package types] | yes, pin 5 [p1]. Only the 2542 variants have VIO [p2 note 2] | STBY, pin 8 [p1] | 8 Mbps [p1] | C114028 MCP2542FDT-E/MNY (266); C623192 -H/MNY (92) | **6.0** |
| MCP2542WFD | 1 | TDFN-8 2x3 [MCP2542 p28] | yes | yes | 8 Mbps | C623194 (32) | 6.0 |
| MCP2544FD | 1 | TDFN-8 2x3 [MCP2542 p28] | **no**, pin 5 is NC [p1] | yes | 8 Mbps | C623202 (228) | 6.0, but **no VIO**, so it is not usable with 3.3 V logic and a 5 V VDD |
| MCP2557FD / MCP2558FD | 1 | TDFN-8 2x3 (JLC listing; datasheet not checked) | unverified | unverified (the 3 mA standby current in the listing suggests silent mode, not standby) | 8 Mbps (listing) | C623212 (20) / C623215 (50) | 6.0 |
| TCAN1046V-Q1 (dual) | 2 | VSON-14, 4.50x3.00 mm [TCAN1046V p1 Device Information] | yes, pin 11 [p3] | STB1/STB2, independent [p3] | 8 Mbps [p1] | C1518670 TCAN1046DMTRQ1 (5176; note this is the non-V part per its MPN); C1852059 TCAN1046VDMTRQ1 (44) | **6.75** |
| TJA1463ATK | 1 | HVSON-14 3x4.5 (JLC) | (datasheet not checked) | | 8 Mbps (listing) | C3216139 (158) | 13.5 |
| TJA1048TK (dual) | 2 | HVSON-14 3x4.5 (JLC) | unverified | unverified | 5 Mbps (listing) | C181726 (9) | 6.75 |
| TCAN1044AV-Q1 | 1 | SOT-23-THIN (DDF) **2.9x2.8 mm** incl. leads; VSON-8 (DRB) 3x3 [TCAN1044A p1 Device Information] | yes, AV variant 1.7-5.5 V [p1] | yes [p1 title] | 5 Mbps rated, 8 possible [p1] | C3236207 DDF (2474); C3234119 DRB (33915) | 8.1 / 9 |
| TCAN1462V | 1 | VSON-8 3x3 (JLC) | (not checked) | | 8 Mbps (listing) | C5215833 (11533) | 9 |
| TJA1462ATK | 1 | HVSON-8 3x3 (JLC) | (not checked) | | 8 Mbps (listing) | C3229259 (2613) | 9 |
| TCAN3413/3414 | 1 | VSON-8 3x3 (JLC), **3.3 V-only supply** | – | | 8 Mbps (listing) | C22445415 (12168) / C31542495 (5211) | 9. Would remove the need for a 5 V CAN rail (from the listing; not checked in the datasheet) |
| TCAN334G | 1 | SOT-23-8 (JLC), 3.3 V-only supply | – | yes (listing says low-power mode) | 5 Mbps (listing) | C191005 (861) | ~8 (datasheet not checked) |
| HMT232TK | 1 | DFN-8 **2x2** (JLC) | ? | ? | **probably classic CAN only** (the name suggests an SN65HVD232 clone; no datasheet found, so unverified) | C56302348 (2372) | 4. **Not usable** until FD capability is verified |

No CAN-FD transceiver in a package smaller than 2x3 mm (DFN 2x2, WLCSP) was found in stock at JLC. No quad CAN transceiver exists in the JLC category; the largest are duals.

### 4-channel totals (IC bodies; caps are 0402 at about 0.5 mm² body each)

| Option | IC area | Supply caps needed | Total (approx.) |
|---|---|---|---|
| 4x MCP2542FD (baseline) | 24 | VDD + VIO per chip = 8 | ~28 |
| 2x TCAN1046V dual | 27 | VCC + VIO per chip = 4 (decoupling per the TI typical application figure on p1; the exact cap count was not read from the layout section) | ~29 |
| 4x TCAN1044AV DDF | 32.4 | 8 | ~36 |

**Recommendation:** keep the MCP2542FD in TDFN-8 2x3 (C114028). It is already the smallest CAN-FD transceiver with VIO and STBY that JLC stocks, at 6 mm² per channel. The TCAN1046V dual (6.75 mm² per channel) gives no area gain. It only halves the part and cap count (2 fewer placements per pair), which may still be worth it on a 1-layer board if routing two channels from one package is easier. Expected area saving for this item: about 0 mm².

---

Method: manufacturer datasheets downloaded and text-extracted (`pdftotext`). Page numbers (`p.N`) are PDF page numbers. LCSC/JLCPCB availability comes from the JLCPCB parts API (`selectSmtComponentList`), queried 2026-10-27: stock counts are a snapshot. "ext" = JLCPCB Extended library. **None of the recommended parts is in the JLCPCB Basic library.** "Footprint" below means the body (or lead-span × body) area from the cited datasheet. It is not the land pattern.

Datasheets used (all fetched 2026-10-27):

| Key | Document | URL |
|---|---|---|
| [OPA197] | TI SBOS737C (Mar 2018), OPAx197 | https://www.ti.com/lit/ds/symlink/opa197.pdf |
| [OPA991] | TI SBOS969F (Jan 2022), OPAx991 | https://www.ti.com/lit/ds/symlink/opa991.pdf |
| [OPA990] | TI SBOS933I (Aug 2021), OPAx990 | https://www.ti.com/lit/ds/symlink/opa990.pdf |
| [TLV910x] | TI SBOS943E (Aug 2021), TLV910x | https://www.ti.com/lit/ds/symlink/tlv9104.pdf |
| [TPS25942] | TI SLVSCE9D (Oct 2017), TPS25942x/44x | https://www.ti.com/lit/ds/symlink/tps25942a.pdf |
| [TPS25947] | TI SLVSFC9C (rev. May 2026) | https://www.ti.com/lit/ds/symlink/tps25947.pdf |
| [TPS25946] | TI SLVSGA8B (Apr 2022) | https://www.ti.com/lit/ds/symlink/tps25946.pdf |
| [TPS2595] | TI SLVSE57C (Apr 2018), TPS2595xx | https://www.ti.com/lit/ds/symlink/tps2595.pdf |
| [TPS2596] | TI SLVSET8A (Aug 2019) | https://www.ti.com/lit/ds/symlink/tps2596.pdf |
| [TPS25961] | TI SLVSGT8 (Dec 2022) | https://www.ti.com/lit/ds/symlink/tps25961.pdf |
| [AP62300] | Diodes DS41958 Rev. 4-2 (2023), AP62300/AP62301/AP62300T | https://www.diodes.com/datasheet/download/AP62300.pdf |
| [TPS6293x] | TI SLUSEA4D (Aug 2022) | https://www.ti.com/lit/ds/symlink/tps62933.pdf |
| [TPS563300] | TI SLUSES9 (Jul 2022) | https://www.ti.com/lit/ds/symlink/tps563300.pdf |
| [TPS62903] | TI SLVSES3A (Mar 2021) | https://www.ti.com/lit/ds/symlink/tps62903.pdf |
| [SiS427EDN] | Vishay doc 62856, S13-1164 Rev. A | https://www.vishay.com/docs/62856/sis427edn.pdf |
| [SiA483ADJ] | Vishay doc 77080, S19-0391 Rev. A | https://www.vishay.com/docs/77080/sia483adj.pdf |
| [SMAJ] | Vishay doc 88390 (rev. 09-Jan-2024), SMAJ series | https://www.vishay.com/docs/88390/smaj50a.pdf |
| [SMF4L] | Littelfuse SMF4L series, rev. JC.05/26/21 (third-party mirror; the littelfuse.com copy returned 403) | https://static.chipdip.ru/lib/251/DOC011251708.pdf |
| [QS5K2] | ROHM QS5K2 Rev. A | https://fscdn.rohm.com/en/products/databook/datasheet/discrete/transistor/mosfet/qs5k2.pdf |
| [DMN3055] | Diodes DS38172 Rev. 3-2, DMN3055LFDB | https://www.diodes.com/datasheet/download/DMN3055LFDB.pdf |
| [APHB-CGK] | Kingbright DSAK6423 V.14A (2025), APHB1608CGKSURKC | https://www.kingbrightusa.com/images/catalog/SPEC/APHB1608CGKSURKC.pdf |
| [APHB-ZGK] | Kingbright DSAM9277 V.7A (2025), APHB1608ZGKSURKC | https://www.kingbrightusa.com/images/catalog/SPEC/APHB1608ZGKSURKC.pdf |
| [JLC] | JLCPCB parts API, keyword search, 2026-10-27 | https://jlcpcb.com/parts (the `C…` number links to https://jlcpcb.com/partdetail/C…) |

---

## 4. CAN bus-activity comparators (4 channels, +12 V supply)

**Corrected function (netlist, via parent agent):** each OPA197 runs from +12 V. +IN ← CANx_H through 10 kΩ. −IN ← CANx_L through 10 kΩ, with a pull to +5 V. The output drives a CAN-activity LED through 2.7 kΩ. It is a comparator, not a current-sense amplifier. Requirements: inputs must accept 0–5 V bus levels, the supply is 12 V, and the output must drive an LED through 2.7 kΩ. That is about (12 V − ~2 V)/2.7 kΩ ≈ 3.7 mA (my arithmetic). An open-drain comparator also works if the LED is moved to sink.

| Part | Ch | Package (body) | Supply | Input range | Output | LCSC / JLC | Source |
|---|---|---|---|---|---|---|---|
| **Baseline** OPA197IDR | 1 | SOIC-8, 4.90 × 3.90 mm | 4.5–36 V | RRIO | ±65 mA | C79274, ext, stk 1727 | [OPA197] p.1 Features + Device Information. Orderables p.34 |
| OPA197IDBVR | 1 | SOT-23-5 (DBV), 2.90 × 1.60 mm | same | same | same | C221351, ext, stk 86 595 | [OPA197] p.1 (DBV listed as "SOT (5)"). p.34 orderable OPA197IDBVR |
| OPA4197IPWR | 4 | TSSOP-14, 5.00 × 4.40 mm | same | same | same | C2057327, ext, stk 1860 | [OPA197] p.1. **There is no QFN for OPA4197**: only SOIC-14 and TSSOP-14 are listed |
| OPA2197IDGKR | 2 | VSSOP-8, 3.00 × 3.00 mm | same | same | same | C2867268, ext, stk 10 679 | [OPA197] p.1 |
| **OPA4991IRUCR (recommended)** | 4 | X2QFN-14 (RUC), 2.00 × 2.00 mm (1.9–2.1), 0.4 mm pitch, 0.4 mm max height | 2.7–40 V (42 V abs) | (V−)−0.1 to (V+)+0.1 V | ISC ±75 mA | **C2878946, ext, stk 5524** | [OPA991] p.1 Device Information. p.9 abs max / recommended VS. p.11 VCM. p.12 ISC. p.69 RUC0014A drawing (2.1/1.9, 8×0.4 pitch). p.39 orderable OPA4991IRUCR |
| OPA4990IRUCR | 4 | X2QFN-14, 2.00 × 2.00 mm | 2.7–40 V | RRIO | — | C4368228, ext, stk 162 | [OPA990] p.1 Device Information |
| TLV9104SIRTER | 4 | WQFN-16, 3.00 × 3.00 mm | 2.7–16 V | RRIO | — | C4370130, ext, stk 3023 | [TLV910x] p.1. The 16 V max leaves little margin on +12 V |
| LM339QT (ST) | 4 | TQFN-16, 3 × 3 mm, open collector | 2–36 V | 0 to VCC−1.5 V (typical LM339; not verified in an ST document) | sink only | C2926946, ext, stk 10 598 | [JLC] only. 0–70 °C grade per the listing |

Notes:
- OPA991 is explicitly suitable as a comparator. Its input protection "does not limit differential input capability" up to 40 V, it is "suitable for use as a comparator" ([OPA991] p.22), and it has phase-reversal protection ([OPA991] p.26).
- **Area for 4 channels:** 4 × SOIC-8 body = 4 × 19.1 = 76.4 mm² of body area alone (with leads, about 4 × 4.9 × 6.0 ≈ 118 mm²; the 6.0 mm lead span is the JEDEC typical and was not extracted). One OPA4991 RUC body = **4.0 mm²**. Decoupling also drops from 4 caps to 1. Estimated saving ≈ 70–110 mm² plus 3 caps.
- Pin pitch on RUC is 0.4 mm. JLCPCB assembles 0.4 mm QFN; that is not verified in a JLC document in this pass.

## 5. eFuses (4 ports, VIN = +12 V, VOUT = OBD-C VBUS)

**Corrected use (netlist, via parent):** VIN = +12 V. ILIM = 28 kΩ. IMON = 33 kΩ to GND into an MCU ADC. EN is driven by the MCU. OVP = GND, PGTH = GND. dVdT/PG/FLT/DMODE are not connected.

**Original current limit:** [TPS25942] Equation 4 (p.23): I(LIM) = 89 / R(ILIM)[kΩ] → 89 / 28 = **3.18 A** (my arithmetic). Characterized points are on p.8 (24.9 kΩ → 3.25/3.53/3.81 A min/typ/max).
**Original IMON scaling:** GAIN(IMON) = 47.78 / 52.3 / 57.23 µA/A ([TPS25942] p.8). V(IMON) = I × 52.3 µA/A × 33 kΩ ≈ 1.73 V/A (my arithmetic, using the equation 14 form on p.31). At 3.18 A that would be 5.5 V, so the 33 kΩ IMON saturates a 3.3 V ADC at about 1.9 A. This is a pre-existing design point and is worth knowing for firmware.

| Part | VIN operating (abs max) | Continuous I | Current monitor | Package (body), pitch | LCSC / JLC | Source |
|---|---|---|---|---|---|---|
| **Baseline** TPS25942A | 2.7–18 V (20 V max) | ILIM 0.6–5.3 A | IMON pin, 52.3 µA/A | WQFN-20 (RVC), 3.00 × 4.00 mm | C181295, ext, stk 2856 | [TPS25942] p.1 Features + Device Information. p.8 GAIN, ILIM. p.50 orderable |
| **TPS259470ARPWR (recommended)** | 2.7–23 V (28 V abs) | 5.5 A (TJ ≤ 125 °C) | Analog ILM pin (shared with ILIM resistor), GIMON 165/182/200 µA/A | QFN-10 HotRod (RPW), 2 × 2 mm, 0.45 mm pitch | **C3662799, ext, stk 3035** | [TPS25947] p.1 Features. p.2 Package Information. p.4 Device Comparison (470A = adjustable OVLO, active current limit, auto-retry). p.7 abs max 28 V. p.8 VIN, IMAX. p.9 ILIM table. p.10 GIMON. p.30 Eq. 5 RILM = 3334/ILIM. p.34 Eq. 9 IOUT = VILM/(RILM × GIMON) |
| TPS259474ARPWR | same | same | same | same, circuit-breaker variant | C3662807, ext, stk 246 | [TPS25947] p.4 |
| TPS259460ARPWR | 2.7–23 V | 5.5 A | ILM ±6 % (≥1 A) | QFN-10, 2 × 2 mm, 0.45 mm | C3294606, ext, stk 3333 | [TPS25946] p.1. p.6 IMAX. p.7 ILIM/GIMON. **Bidirectional when ON, so there is no reverse blocking while enabled.** The TPS25942 blocked reverse current ([TPS25942] p.1), so the TPS25947 is the closer functional match |
| TPS259571DSGR | 2.7–18 V (20 V abs) | 4 A (5 A at TJ ≤ 105 °C) | ILM 276 µA/A | WSON-8, 2.00 × 2.00 mm | C471038, ext, stk 11 367 | [TPS2595] p.1. p.3 Device Comparison (x70/x71 = no OV clamp). p.5 VIN, IMAX. p.7 ILM gain, ILIM. Avoid the x2x/x3x/x4x variants: their fixed OV clamp is 3.8/5.7/13.7 V |
| TPS2596x | 2.7–19 V | 0.125–2 A | ILM | SOIC-8 per p.1 table | — | [TPS2596] p.1. **Fails: 2 A < 3.18 A** |
| TPS25961 | 2.7–19 V | 0.1–2 A | none listed | SON-6, 2 × 2 mm | — | [TPS25961] p.1. **Fails: 2 A, no monitor** |

Substitution notes (TPS259470A):
- RILM for the same 3.18 A limit is 3334/3.18 ≈ 1.05 kΩ (Eq. 5, p.30; my arithmetic). The ILM voltage is then about 0.19 V/A, about 0.61 V full scale at ILIM (Eq. 9, p.34). Firmware rescales the ADC reading. The separate IMON resistor goes away, so each channel drops one resistor (ILIM and IMON are the same pin).
- OVLO pin: the original OVP pin was tied to GND. That TPS259470 OVLO can simply be grounded to disable OVLO is **not verified**; read §7.3.4 / pin table p.5 before layout. ITIMER and dVdt are left open in the TI EC test conditions (p.8). Whether open is acceptable in the application is not verified.
- **Area:** 12.0 mm² → 4.0 mm² body per channel. That is 32 mm² saved over 4 channels, plus 4 resistors.

## 6. Power front end: bucks, inductors, reverse-protection P-FET, TVS

### 6a. Buck converters (2×; 12 V in, outputs presumed 5 V and 3.3 V)

| Part | VIN | IOUT | fSW | Package (body) | LCSC / JLC | Source |
|---|---|---|---|---|---|---|
| **Baseline** AP62300TWU-7 | 4.2–18 V. Abs max VIN −0.3 to +22 V for 400 ms | 3 A | 750 kHz per [JLC] listing (not checked in the datasheet) | TSOT26, 2.90 × 2.80 mm (lead span) | C1883519, ext, stk 18 712 | [AP62300] p.1. p.4 VIN, abs max. p.21 TSOT26 D 2.90, E 2.80 |
| **AP62300Z6-7 (recommended, same die)** | 4.2–18 V | 3 A | same | **SOT563, 1.60 × 1.60 mm**, 0.5 mm pitch | **C5250745, ext, stk 1224** | [AP62300] p.1 ("available in SOT563 and TSOT26"). p.21 SOT563 D = E = 1.60, e = 0.50. **Different pinout**: p.1 pin assignments |
| AP62301Z6-7 | 4.2–18 V | 3 A | — | SOT563 | C1880992, ext, stk 1754 | [AP62300] p.1. AP62301 differs in light-load mode (not detailed here) |
| TPS62933DRLR | **3.8–30 V** | 3 A | 200 kHz–2.2 MHz | SOT583, 1.60 × 2.10 mm | C3200405, ext, stk 14 892 | [TPS6293x] p.1 |
| TPS563300DRLR | 3.8–28 V | 3 A | 500 kHz per [JLC] | SOT583, 1.60 × 2.10 mm, 0.5 mm pitch | C5219249, ext, stk 3066 | [TPS563300] p.1 |
| TPS62903RPJR | 3–17 V | 3 A | 1.0 / 2.5 MHz | VQFN-9 HR, 1.5 × 2.0 mm, 0.5 mm pitch | C2866502, ext, stk 1165 | [TPS62903] p.1 |

Thermal caveat: the AP62300 θJA is 110 °C/W in SOT563 versus 70 °C/W in TSOT26 ([AP62300] p.4, p.16). Figure 40 (p.16) gives SOT563 output-current derating versus ambient at VIN = 12 V. Check it against the real load.
Robustness note: the existing SMAJ16CA clamps at 26 V ([SMAJ] p.2), which is above the AP62300 22 V (400 ms) abs max ([AP62300] p.4). TPS62933 (30 V) or TPS563300 (28 V) would close that gap at a similar footprint (1.6 × 2.1 mm). The TPS62903 has a 17 V max VIN and is the least robust option.

### 6b. Inductors

The baseline is 3.3 µH in 1008 = 2.5 × 2.0 mm. This value is what [AP62300] p.17 (Table, VOUT 3.3 V / 5 V → L = 3.3 µH) recommends at 750 kHz.
- With the AP62300 (any package), keep **3.3 µH**: the frequency is fixed, so the inductor cannot shrink on value alone.
- With the TPS62933 at 2.2 MHz: [TPS6293x] Eq. 16 (p.30) L ≥ (VIN−VOUT)·VOUT / (VIN·fSW·K·IOUT). With K = 0.3 and 3 A (my arithmetic), that gives 1.47 µH for 12→5 V and 1.21 µH for 12→3.3 V. That fits a **Murata DFE201612E-1R5M (2.0 × 1.6 × 1.2 mm), C882597, ext, stk 1288**. The LCSC listing gives 2.3 A / 3.2 A, which is less than 3 A Irms, and the Murata datasheet was not fetched. For 1 µH, the DFE201612E-1R0M is C337892 (LCSC 2.9 A / 4 A, stk 17 389).
- With the TPS62903 at 2.5 MHz, TI lists 1.0 µH TDK TFM252010ALM-1R0MTAA (2.5 × 2.0 × 1.0 mm, 4.7 A) and Murata DFE252012F-1R0M ([TPS62903] p.19, Table 8-4).
- Savings: 5.0 mm² → 3.2 mm² per inductor (2016 versus 2520). The real gain is in height (1.2 mm or lower) only if the inductor sets the board height.
- The original load currents are unknown. A 3.3 µH 1008 part typically saturates around 2 A or less (not verified), so the design probably never draws 3 A, and smaller/lower-current parts may be acceptable.

### 6c. Reverse-protection P-FET

The P-FET carries all port current: 4 eFuses × up to 3.18 A at 12 V plus the bucks. Keep RDS(on) low.

| Part | VDS | RDS(on) max @ −10 V / −4.5 V | ID | VGS max | Package (body) | LCSC / JLC | Source |
|---|---|---|---|---|---|---|---|
| **Baseline** SiS427EDN | −30 V | 10.6 / 21.3 mΩ | −50 A (package-limited figure) | ±25 V | PowerPAK 1212-8, 3.30 × 3.30 mm | C727478, ext, stk 7032 | [SiS427EDN] p.1 Product Summary. p.2. p.7 dims D = E = 3.30 |
| **SiA483ADJ (smaller)** | −30 V | 20 / 33 mΩ | −12 A | −20 / +16 V | PowerPAK SC-70-6L, 2.05 × 2.05 mm | **C5377599, ext, stk 5308** | [SiA483ADJ] p.1. p.7 dims D = E = 2.05 |
| DMP3028LFDE-7 | −30 V | 25 mΩ @ 10 V (LCSC) | 6.8 A | — | U-DFN2020-6 | C461077, ext, stk 113 | [JLC] listing only, datasheet not fetched |

Area 10.9 → 4.2 mm² (−6.7 mm²). Conduction loss doubles: at, for example, 10 A it is 2.0 W versus 1.06 W (I²R, my arithmetic). That is probably too much for a 2 × 2 package at full load. Only substitute it if the realistic total input current is about 5 A or less. VGS: with gate-to-GND drive at a 12–16 V input, VGS stays within −20 V. Above about 20 V, a gate zener/clamp is needed. Check what the original circuit does.

### 6d. TVS (input, 16 V bidirectional)

The task said "SMAJ16CA (DO-214AA)". SMAJ is the **SMA = DO-214AC** package ([SMAJ] p.1). DO-214AA is SMB. Confirm which footprint the board actually uses.

| Part | VWM | VBR min/max | Ipp (10/1000 µs) | VC | PPPM | Package | LCSC / JLC | Source |
|---|---|---|---|---|---|---|---|---|
| **Baseline** SMAJ16CA | 16 V | 17.8 / 19.7 V | 15.4 A | 26.0 V | 400 W | SMA (DO-214AC): body 3.99–4.50 × 2.54–2.79 mm, overall length ≤ 5.28 mm | e.g. C134960 (Diodes), ext, stk 4907 | [SMAJ] p.1. p.2 table row SMAJ16A (CA marking XP). p.5 outline |
| **SMF4L16CA (recommended, same 400 W)** | 16 V | 17.80 / 19.70 V | 15.4 A | 26.0 V | 400 W | SOD-123FL: body 2.90–3.10 × 1.70–2.00 mm, overall 3.50–3.90 mm | Littelfuse C1849666 (**stk 3**). Diodes SMF4L16CAQ-7 C7363871 (stk 10). Comchip ASMF4L16CA-HF C5151525 (stk 5). All ext | [SMF4L] p.1 (400 W). p.2 row SMF4L16CA. p.5 dims. **JLC stock is very low; verify before ordering** |
| SMF16CA (200 W) | 16 V | 17.8 / 19.7 V | 7.7 A | 26 V | 200 W | SOD-123FL | Littelfuse C1851354 (stk 1148). hongjiacheng C19077511 (**ext, "preferred", stk 415 679**) | [JLC] listing values. Littelfuse SMF datasheet not retrieved (403). **Half the surge rating** |

Area ≈ 5.28 × 2.79 = 14.7 mm² → 3.9 × 2.0 = 7.8 mm² (−6.9 mm²). Height also drops (SOD-123FL is a flat, low-profile package per [SMF4L] p.1; the exact height was not extracted).

## 7. Dual N-FETs (8×) and bicolour LEDs (8×)

### 7a. Dual N-FET

| Part | VDS | RDS(on) max | ID | VGS max | VGS(th) | Config | Package (body) | LCSC / JLC | Source |
|---|---|---|---|---|---|---|---|---|---|
| **Baseline** QS5K2 | 30 V | 100 mΩ @ 4.5 V, 107 @ 4.0 V, 154 @ 2.5 V | ±2.0 A (8 A pulsed) | ±12 V | 0.5–1.5 V | Dual N, **common source** | TSMT5, 2.9 × 1.6 mm body, 2.8 mm lead span | C2889124, ext, stk 491 | [QS5K2] p.1 abs max + outline. p.2 EC table |
| **DMN3055LFDB-7 (recommended)** | 30 V | 40 mΩ @ 4.5 V, 75 mΩ @ 2.5 V | 5.0 A (TA 25 °C) | ±12 V | 0.5–1.5 V | Dual N, independent | U-DFN2020-6 (Type B), 2.00 × 2.00 mm, 0.65 mm pitch | **C5245313, ext, stk 1455** | [DMN3055] p.1 summary. p.3 abs max + EC. p.7 outline |
| DMT3020LFDB-7 | 30 V | 32 mΩ @ 4.5 V (LCSC) | 7.7 A | ±20 V | VGS(th) up to 3 V per listing (risky at 3.3 V drive) | Dual N | U-DFN2020-6 | C154736, ext, stk 742 | [JLC]. Datasheet DS38407 fetched; only VDS/VGS were extracted |
| PMDPB95XNE2X (Nexperia) | 30 V | 77 mΩ @ 4.5 V (LCSC) | 3 A | — | — | Dual N | DFN2020-6 | C552758, ext, stk 5441 | [JLC] only. Nexperia datasheet blocked (403), **unverified** |
| PMDXB550UNEZ (Nexperia) | 30 V | 670 mΩ @ 4.5 V | 0.59 A | — | — | Dual N | DFN1010-6 (1.1 × 1.0 mm) | C552763, ext, **stk 19** | [JLC] only, **unverified**. Only valid if each output is ≤ about 0.3 A |

Area: TSMT5 2.9 × 2.8 = 8.1 mm² → DFN2020 4.0 mm². That is −4.1 mm² each, −33 mm² for 8. Pinout differs: QS5K2 is common-source, while the DMN3055 has two independent sources that you tie together on the board. A DFN1010 dual (1.1 mm²) is only possible if the relay/ignition loads are small. The load current was not given, so this is not verified.

### 7b. Bicolour LEDs (SOT-23-3 → 0603-size 4-pad)

| Part | Colours (λdom) | VF typ/max @ 20 mA | Package | LCSC / JLC | Source |
|---|---|---|---|---|---|
| **Kingbright APHB1608CGKSURKC** | Green 570 nm + Hyper Red 630 nm | Green 2.1/2.5 V | **1.6 × 0.8 × 0.5 mm**, 4-pad (two independent dice) | **C6506245, ext, stk 943** | [APHB-CGK] p.1 |
| Kingbright APHB1608ZGKSURKC | Green 525 nm (InGaN) + Hyper Red 630 nm | Green 3.3/4.1 V | 1.6 × 0.8 × 0.5 mm | C5879051, ext, stk 1265 | [APHB-ZGK] p.1 |
| Kingbright APHB1608LZGKSURKC | low-current (2 mA) green/red | — | 1.6 × 0.8 mm | C6862094, ext, stk 1615 | [JLC] |
| Kingbright APHBM2012 family | various | — | 2.0 × 1.2–1.3 mm | e.g. C5553822 (red/yellow), C5879044 (red/green) | [JLC] |

Area: SOT-23-3 is about 2.9 × 2.4 ≈ 7 mm² (typical SOT-23 lead span, not extracted from a datasheet). APHB1608 is 1.28 mm², so about −5.7 mm² each and about −46 mm² for 8. The four pads have independent anodes and cathodes, so either common-anode or common-cathode wiring of the original can be reproduced. If the drive is from +5 V/+12 V through fixed resistors, the InGaN green (VF 3.3 V) needs different resistors from the AlGaInP green.

---

## Items 4–7 summary (body-area deltas; my arithmetic from the cited dimensions)

| Item | From → To | Area before → after | Saved |
|---|---|---|---|
| 4 comparators | 4× OPA197 SOIC-8 → 1× OPA4991IRUCR X2QFN-14 2 × 2 (C2878946) | ~76 mm² body (~118 with leads) → 4 mm² | ~72–114 mm² + 3 decoupling caps |
| 4 eFuses | TPS25942A WQFN-20 3 × 4 → TPS259470A QFN-10 2 × 2 (C3662799) | 48 → 16 mm² | 32 mm² + 4 resistors (ILIM = IMON pin). Firmware rescales ILM |
| 2 bucks | AP62300 TSOT26 → AP62300Z6 SOT563 (C5250745), or TPS62933 SOT583 for 30 V VIN | 2 × 8.1 → 2 × 2.6 (or 3.4) mm² | ~11 mm² |
| Inductors | 3.3 µH 1008 → 1.5 µH 2016 (needs a 2.2 MHz buck, e.g. TPS62933) | 2 × 5.0 → 2 × 3.2 mm² | ~3.6 mm² (current rating to verify) |
| P-FET | SiS427EDN 3.3 × 3.3 → SiA483ADJ 2.05 × 2.05 (C5377599) | 10.9 → 4.2 mm² | 6.7 mm². RDS(on) doubles; only OK at modest current |
| TVS | SMAJ16CA SMA → SMF4L16CA SOD-123FL (400 W) | 14.7 → 7.8 mm² | 6.9 mm². JLC stock 3–10 pcs |
| 8 dual FETs | QS5K2 TSMT5 → DMN3055LFDB U-DFN2020-6 (C5245313) | 8 × 8.1 → 8 × 4.0 | ~33 mm² |
| 8 bicolour LEDs | SOT-23-3 → Kingbright APHB1608 1.6 × 0.8 (C6506245) | 8 × ~7 → 8 × 1.28 | ~46 mm² |

**Unverified or open items:** whether the TPS259470 OVLO pin can be grounded and ITIMER left open in the application; the Nexperia PMDPB95XNE2 and PMDXB550UNE parameters (datasheets 403); the Murata DFE201612E ratings (LCSC listing only); SOIC-8 and SOT-23 lead spans (JEDEC typicals, not extracted); real buck and relay load currents (not in the brief); the JLCPCB 0.4 mm-pitch QFN capability (not checked in a JLC document).
