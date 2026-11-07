//! What a part is, from its reference designator, value and footprint: the corpus carries no
//! part numbers and no pin names, so these three are all there is.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Resistor,
    Capacitor,
    Inductor,
    FerriteBead,
    Diode,
    Led,
    /// TVS, ESD arrays, varistors.
    Suppressor,
    Crystal,
    Oscillator,
    Ic,
    Transistor,
    Connector,
    Fuse,
    Switch,
    TestPoint,
    /// Vias, holes, logos, solder pads: footprints with no electrical part behind them.
    Mechanical,
    Other,
}

/// A part's kind and the fact it was decided on.
pub fn classify(reference: &str, value: &str, footprint: &str) -> (Kind, String) {
    let prefix: String = reference.chars().take_while(|c| c.is_ascii_alphabetic()).collect::<String>().to_ascii_uppercase();
    let v = value.to_ascii_uppercase();
    let fp = footprint.rsplit(':').next().unwrap_or(footprint).to_ascii_uppercase();
    let has = |s: &str, keys: &[&str]| keys.iter().any(|k| s.contains(k));
    let why = |what: &str| format!("{reference} is {what} (refdes {prefix}, value {value:?}, footprint {fp})");

    const MECH: &[&str] = &["VIA", "HOLE", "LOGO", "MOUNT", "FIDUCIAL", "SOLDER_PAD", "PROTO", "PERF", "OSHW", "STITCH"];
    // A designator of a real part wins over a footprint name: `QFN-60_ThermalVias` is an IC.
    const ELECTRICAL: &[&str] = &["R", "C", "L", "D", "U", "IC", "Q", "J", "P", "Y", "X", "F", "SW", "LED", "FB", "CON", "CN", "K", "T", "TP", "RN", "M", "S", "B", "BT"];
    let mech_name = !ELECTRICAL.contains(&prefix.as_str()) && (has(&fp, MECH) || has(&v, MECH));
    if mech_name || matches!(prefix.as_str(), "H" | "MH" | "REF" | "G" | "LOGO" | "FID" | "VIA" | "HOLE") {
        // `G` is KiCad's graphic-item prefix (logos); `REF**` an unnamed footprint.
        if !(prefix == "G" && has(&fp, &["CONN", "PIN"])) {
            return (Kind::Mechanical, why("mechanical"));
        }
    }
    if has(&v, &["TVS", "ESD", "USBLC", "PRTR", "PESD", "TPD", "SMBJ", "SMAJ", "SM712", "VARISTOR", "MOV"]) || has(&fp, &["SMBJ", "SMAJ"]) {
        return (Kind::Suppressor, why("a suppressor"));
    }
    if prefix == "FB" || has(&v, &["FERRITE", "BLM", "BEAD"]) || has(&fp, &["FERRITE", "BEAD"]) {
        return (Kind::FerriteBead, why("a ferrite bead"));
    }
    if has(&fp, &["OSCILLATOR"]) || has(&v, &["TCXO", "OSC", "XO-"]) && !has(&fp, &["CRYSTAL"]) {
        return (Kind::Oscillator, why("an oscillator"));
    }
    let crystalish = has(&fp, &["CRYSTAL", "XTAL", "HC49", "HC-49", "RESONATOR", "TSX", "ABM", "FC-135", "FA-238", "3215", "2016", "2520"]) || hertz(&v).is_some() || has(&v, &["XTAL", "CRYSTAL"]);
    if prefix == "Y" || (matches!(prefix.as_str(), "X" | "XT" | "Q" | "XTAL" | "U") && crystalish && !has(&fp, &["SOT", "SOIC", "QFN", "TSSOP", "CONN", "PIN"])) {
        return (Kind::Crystal, why("a crystal"));
    }
    if has(&v, &["LED"]) || has(&fp, &["LED"]) || matches!(prefix.as_str(), "LED" | "LD") {
        return (Kind::Led, why("an LED"));
    }
    match prefix.as_str() {
        "R" | "RN" | "RV" | "VR" | "RA" | "RP" => return (Kind::Resistor, why("a resistor")),
        "C" | "CP" | "CE" => return (Kind::Capacitor, why("a capacitor")),
        // An `L` valued in ohms (`600R@100MHz`) is a bead; one under 0.4 uH an RF part.
        "L" if v.contains("R@") || v.ends_with('R') || v.contains("OHM") => return (Kind::FerriteBead, why("a ferrite bead")),
        "L" if si_value(&v, false).is_some_and(|h| h < 0.4e-6) => return (Kind::Other, why("an RF inductor")),
        "L" => return (Kind::Inductor, why("an inductor")),
        "D" | "CR" | "ZD" => return (Kind::Diode, why("a diode")),
        "U" | "IC" | "IC_" | "M" if !has(&fp, &["CONN", "PIN_HEADER", "PINHEADER"]) => return (Kind::Ic, why("an IC")),
        "Q" | "T" if !has(&fp, &["TRANSFORMER"]) => return (Kind::Transistor, why("a transistor")),
        "J" | "P" | "CON" | "CN" | "X" | "USB" | "CONN" => return (Kind::Connector, why("a connector")),
        "F" | "PTC" | "PF" => return (Kind::Fuse, why("a fuse")),
        "SW" | "S" | "BTN" | "B" | "K" | "MX" | "KEY" => return (Kind::Switch, why("a switch")),
        "TP" => return (Kind::TestPoint, why("a test point")),
        _ => {}
    }
    // An unknown prefix: fall back on the footprint.
    if has(&fp, &["CONN", "PIN_HEADER", "PINHEADER", "HEADER", "JST", "USB", "MOLEX", "TERMINAL"]) {
        return (Kind::Connector, why("a connector"));
    }
    if fp.starts_with("R_") || fp.starts_with("RES") {
        return (Kind::Resistor, why("a resistor"));
    }
    if fp.starts_with("C_") || fp.starts_with("CAP") {
        return (Kind::Capacitor, why("a capacitor"));
    }
    if has(&fp, &["SOIC", "QFN", "QFP", "TSSOP", "DIP", "MSOP", "BGA", "SOT-23-5", "SOT-23-6"]) {
        return (Kind::Ic, why("an IC"));
    }
    (Kind::Other, why("unrecognised"))
}

/// A frequency in a value string: `12MHz`, `32.768kHz`, `16M` on a crystal.
pub fn hertz(v: &str) -> Option<f64> {
    let v = v.to_ascii_uppercase();
    for (unit, mult) in [("MHZ", 1e6), ("KHZ", 1e3), ("HZ", 1.0)] {
        if let Some(i) = v.find(unit) {
            let num: String = v[..i].chars().rev().take_while(|c| c.is_ascii_digit() || *c == '.').collect::<Vec<_>>().into_iter().rev().collect();
            if let Ok(x) = num.parse::<f64>() {
                return Some(x * mult);
            }
        }
    }
    None
}

/// A passive's value in SI units: `100n`, `0.1uF`, `4K7`, `10k`, `2R2`, `4.7µH`. `M` is mega on
/// a resistor and milli otherwise.
pub fn si_value(v: &str, resistor: bool) -> Option<f64> {
    let v = v.trim().replace('µ', "u").to_ascii_uppercase();
    let v = v.split([' ', '/', '_', ',']).next()?;
    let v = v.trim_end_matches(['F', 'H', 'Ω']).trim_end_matches("OHM");
    let mult = |c: char| match c {
        'P' => Some(1e-12),
        'N' => Some(1e-9),
        'U' => Some(1e-6),
        'M' if resistor => Some(1e6),
        'M' => Some(1e-3),
        'R' | 'E' => Some(1.0),
        'K' => Some(1e3),
        _ => None,
    };
    // `4K7`: the multiplier stands for the decimal point.
    if let Some((i, c)) = v.char_indices().find(|(_, c)| c.is_ascii_alphabetic()) {
        let m = mult(c)?;
        let (a, b) = (&v[..i], &v[i + 1..]);
        if a.is_empty() || !b.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        let x: f64 = if b.is_empty() { a.parse().ok()? } else { format!("{a}.{b}").parse().ok()? };
        return Some(x * m);
    }
    v.parse().ok()
}

/// Value strings naming a linear regulator, by family prefix.
pub const LDO_FAMILIES: &[&str] = &[
    "AMS1117", "LM1117", "LD1117", "NCP1117", "AP2112", "AP2210", "AP7361", "AP7365", "XC6206", "XC6210", "XC6220", "MCP1700", "MCP1702", "MCP1703", "MCP1754", "MIC5205",
    "MIC5219", "MIC5504", "LP2985", "LP2992", "LP5907", "LP2950", "LP38", "TLV70", "TLV75", "TPS7A", "TPS73", "LM317", "LM2937", "LM3940", "RT9013", "RT9193", "HT73", "HT75",
    "SPX3819", "ME6211", "ME6206", "L78", "LM78", "MC78", "UA78", "L79", "LM79", "7805", "7812", "7833", "7905", "LT1117", "LT1763", "LT3042", "ADP150", "ADP7118", "BD433",
];

/// Value strings naming a microcontroller or SoC, by family prefix.
pub const MCU_FAMILIES: &[&str] = &[
    "ATMEGA", "ATTINY", "ATXMEGA", "AT90", "ATSAM", "SAMD", "SAME", "STM32", "STM8", "GD32", "CH32", "CH55", "RP2040", "RP2350", "ESP32", "ESP8266", "ESP-", "NRF51", "NRF52",
    "NRF53", "PIC1", "PIC24", "PIC32", "DSPIC", "MSP430", "EFM32", "EFR32", "LPC", "MK20", "MK64", "MKL", "MIMXRT", "CY8C", "TM4C", "AVR", "ARDUINO", "TEENSY",
];

/// Whether a value names one of `families`, ignoring a leading manufacturer word.
pub fn names_family(value: &str, families: &[&str]) -> bool {
    let v = value.to_ascii_uppercase();
    families.iter().any(|f| v.split(|c: char| !c.is_ascii_alphanumeric() && c != '-').any(|w| w.starts_with(f)))
}

/// A linear regulator's rated output current, from its family: `AMS1117` 1 A, `78L05` 0.1 A.
pub fn ldo_rating(value: &str) -> Option<f64> {
    let v = value.to_ascii_uppercase();
    if v.contains("78L") || v.contains("79L") {
        return Some(0.1);
    }
    if v.contains("78M") || v.contains("79M") {
        return Some(0.5);
    }
    const RATINGS: &[(&str, f64)] = &[
        ("AMS1117", 1.0), ("LM1117", 0.8), ("LD1117", 0.8), ("NCP1117", 1.0), ("LT1117", 0.8), ("AP2112", 0.6), ("AP2210", 0.3), ("AP7361", 1.0), ("AP7365", 0.6),
        ("XC6206", 0.2), ("XC6210", 0.7), ("XC6220", 0.7), ("MCP1700", 0.25), ("MCP1702", 0.25), ("MCP1703", 0.25), ("MCP1754", 0.15), ("MIC5205", 0.15),
        ("MIC5219", 0.5), ("MIC5504", 0.3), ("LP2985", 0.15), ("LP2992", 0.25), ("LP5907", 0.25), ("LP2950", 0.1), ("LP38", 0.8), ("TLV70", 0.2), ("TLV75", 0.5),
        ("TPS7A", 0.5), ("TPS73", 0.15), ("LM317", 1.5), ("LM2937", 0.5), ("LM3940", 1.0), ("RT9013", 0.5), ("RT9193", 0.3), ("HT73", 0.25), ("HT75", 0.1),
        ("SPX3819", 0.5), ("ME6211", 0.5), ("ME6206", 0.3), ("L78", 1.0), ("LM78", 1.0), ("MC78", 1.0), ("UA78", 1.0), ("L79", 1.0), ("LM79", 1.0), ("7805", 1.0),
        ("7812", 1.0), ("7833", 1.0), ("7905", 1.0), ("LT1763", 0.5), ("LT3042", 0.2), ("ADP150", 0.15), ("ADP7118", 0.2), ("BD433", 0.5),
    ];
    RATINGS.iter().find(|(f, _)| names_family(&v, &[f])).map(|(_, a)| *a)
}

/// A current in a value string: `500mA`, `1.1A`, `PTC 2A`.
pub fn amps(value: &str) -> Option<f64> {
    let v = value.to_ascii_uppercase();
    let b = v.as_bytes();
    for i in 0..b.len() {
        if b[i] != b'A' || (i + 1 < b.len() && b[i + 1].is_ascii_alphabetic()) {
            continue;
        }
        let (end, mult) = if i > 0 && b[i - 1] == b'M' { (i - 1, 1e-3) } else { (i, 1.0) };
        let start = v[..end].rfind(|c: char| !(c.is_ascii_digit() || c == '.')).map_or(0, |j| j + 1);
        if let Ok(x) = v[start..end].parse::<f64>() {
            if x > 0.0 {
                return Some(x * mult);
            }
        }
    }
    None
}

/// A power inductor's current class from its footprint's size: the corpus gives no saturation
/// current, but a part's body size bounds it. Returns amps and the size it read.
pub fn inductor_class(footprint: &str) -> Option<(f64, f64)> {
    let fp = footprint.rsplit(':').next().unwrap_or(footprint).to_ascii_uppercase();
    let num = |s: &str| s.parse::<f64>().ok();
    let mut size: Option<f64> = None;
    // `4.5X4.5`, `5X5MM`.
    let b = fp.as_bytes();
    for (i, c) in fp.char_indices() {
        if c != 'X' || i == 0 || !b[i - 1].is_ascii_digit() {
            continue;
        }
        let start = fp[..i].rfind(|c: char| !(c.is_ascii_digit() || c == '.')).map_or(0, |j| j + 1);
        let rest: String = fp[i + 1..].chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
        if let (Some(a), Some(b)) = (num(&fp[start..i]), num(&rest)) {
            if a <= 30.0 && b <= 30.0 {
                size = Some(a.max(b));
                break;
            }
        }
    }
    // `CDRH4D22`: the number before `D` is the body in mm.
    if size.is_none() {
        if let Some(i) = fp.find("CDRH") {
            let n: String = fp[i + 4..].chars().take_while(|c| c.is_ascii_digit()).collect();
            size = num(&n);
        }
    }
    if size.is_none() {
        const CODES: &[(&str, f64)] = &[("0402", 1.0), ("0603", 1.6), ("0805", 2.0), ("1008", 2.5), ("1206", 3.2), ("1210", 3.2), ("1812", 4.5), ("2016", 2.0), ("2520", 2.5)];
        size = CODES.iter().find(|(c, _)| fp.contains(c)).map(|(_, s)| *s);
    }
    // A vendor series code `4040`, `6045`, `7045`: the body's side in tenths of a mm, first.
    if size.is_none() {
        let digits: Vec<char> = fp.chars().collect();
        for w in digits.windows(4) {
            if w.iter().all(|c| c.is_ascii_digit()) {
                let side: f64 = format!("{}{}", w[0], w[1]).parse().unwrap();
                if (20.0..=99.0).contains(&side) {
                    size = Some(side / 10.0);
                    break;
                }
            }
        }
    }
    let s = size?;
    let a = match s {
        s if s < 1.8 => 0.3,
        s if s < 2.6 => 0.8,
        s if s < 3.5 => 1.5,
        s if s < 4.6 => 2.0,
        s if s < 6.5 => 3.0,
        s if s < 8.5 => 5.0,
        _ => 8.0,
    };
    Some((a, s))
}

/// What a part draws from a rail it is on, coarsely: the corpus has no datasheets, so a
/// microcontroller, a radio SoC, an addressable LED and an indicator each get a typical figure.
/// FPGA families by part-number prefix, and the core current of a typical design: well above
/// static, well below a full fabric at speed.
pub const FPGA_FAMILIES: &[(&str, f64, &str)] = &[
    ("LFE5", 0.5, "an ECP5 FPGA's core, about 500 mA in a typical design"),
    ("XC7A", 0.6, "an Artix-7 FPGA's core, about 600 mA in a typical design"),
    ("XC7S", 0.3, "a Spartan-7 FPGA's core, about 300 mA in a typical design"),
    ("XC6S", 0.3, "a Spartan-6 FPGA's core, about 300 mA in a typical design"),
    ("XC7Z", 0.8, "a Zynq-7000's core, about 800 mA in a typical design"),
    ("EP4CE", 0.3, "a Cyclone IV FPGA's core, about 300 mA in a typical design"),
    ("10CL", 0.3, "a Cyclone 10 LP FPGA's core, about 300 mA in a typical design"),
    ("10M", 0.1, "a MAX 10 FPGA's core, about 100 mA in a typical design"),
    ("GW1N", 0.05, "a Gowin GW1N FPGA's core, about 50 mA in a typical design"),
    ("ICE40", 0.02, "an iCE40 FPGA's core, about 20 mA in a typical design"),
    ("ICE5", 0.02, "an iCE40 UltraPlus FPGA's core, about 20 mA in a typical design"),
];

/// Plug-in stepper driver modules, by value: they sit in a socket, so a board may call them a
/// connector or a U-part.
pub const STEPPER_MODULES: &[&str] = &["DRV8825", "A4988", "TMC2", "STEPSTICK", "POLOLU"];

/// An FPGA's core current, if `value` names one.
pub fn fpga(value: &str) -> Option<(f64, &'static str)> {
    let v = value.to_ascii_uppercase();
    FPGA_FAMILIES.iter().find(|(p, _, _)| v.starts_with(p)).map(|(_, a, why)| (*a, *why))
}

pub fn load(kind: Kind, value: &str) -> Option<(f64, &'static str)> {
    let v = value.to_ascii_uppercase();
    match kind {
        Kind::Ic if v.contains("ESP32") || v.contains("ESP8266") || v.starts_with("ESP-") => Some((0.35, "a Wi-Fi SoC, about 350 mA averaged over transmit")),
        Kind::Ic if names_family(value, MCU_FAMILIES) => Some((0.05, "a microcontroller, about 50 mA")),
        Kind::Ic | Kind::Connector if STEPPER_MODULES.iter().any(|k| v.contains(k)) => Some((0.4, "a plug-in stepper driver, about 400 mA from its motor supply")),
        Kind::Ic => Some((0.01, "an IC, about 10 mA")),
        Kind::Led if ["WS2812", "SK6812", "APA102", "SK9822", "ARGB", "NEOPIXEL", "WS2813", "APA106"].iter().any(|k| v.contains(k)) => {
            Some((0.04, "an addressable LED, about 40 mA at full white"))
        }
        Kind::Led => Some((0.01, "an LED, about 10 mA")),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Option<f64>, b: f64) -> bool {
        a.is_some_and(|a| (a - b).abs() <= 1e-9 * b.abs())
    }

    #[test]
    fn values() {
        assert!(close(si_value("100n", false), 100e-9));
        assert!(close(si_value("0.1uF", false), 0.1e-6));
        assert!(close(si_value("4K7", true), 4700.0));
        assert!(close(si_value("10k", true), 10e3));
        assert!(close(si_value("2R2", true), 2.2));
        assert_eq!(si_value("VAL**", false), None);
        assert!(close(si_value("1M", true), 1e6));
        assert!(close(hertz("12MHz"), 12e6));
        assert!(close(hertz("32.768kHz"), 32768.0));
    }

    #[test]
    fn kinds() {
        assert_eq!(classify("R1", "10k", "Resistor_SMD:R_0603_1608Metric").0, Kind::Resistor);
        assert_eq!(classify("Y1", "12MHz", "Crystal:Crystal_SMD_3225-4Pin_3.2x2.5mm").0, Kind::Crystal);
        assert_eq!(classify("D3", "LED", "LED_SMD:LED_0603").0, Kind::Led);
        assert_eq!(classify("D1", "SS14", "Diode_SMD:D_SMA").0, Kind::Diode);
        assert_eq!(classify("U2", "USBLC6-2SC6", "Package_TO_SOT_SMD:SOT-23-6").0, Kind::Suppressor);
        assert_eq!(classify("REF**", "AUTO_VIA", "VIA_MATRIX").0, Kind::Mechanical);
        assert_eq!(classify("U1", "RP2350A", "Package_DFN_QFN:QFN-60-1EP_7x7mm_P0.4mm_EP3.4x3.4mm_ThermalVias").0, Kind::Ic);
        assert_eq!(classify("J1", "USB_C", "Connector_USB:USB_C_Receptacle").0, Kind::Connector);
        assert_eq!(classify("L1", "600R@100MHz", "SMD-0805").0, Kind::FerriteBead);
        assert_eq!(classify("L9", "8.2nH", "IND_0402").0, Kind::Other);
        assert_eq!(classify("L1", "4.7uH", "L_1210").0, Kind::Inductor);
        assert!(names_family("AMS1117-3.3", LDO_FAMILIES));
        assert_eq!(ldo_rating("AMS1117-3.3"), Some(1.0));
        assert_eq!(ldo_rating("L78L05"), Some(0.1));
        assert_eq!(ldo_rating("AP7361C-33FGE"), Some(1.0));
        assert!(close(amps("500mA"), 0.5) && close(amps("PTC 1.1A"), 1.1) && amps("SMAJ5.0").is_none());
        assert_eq!(inductor_class("Inductor_SMD:L_Taiyo-Yuden_MD-4040"), Some((2.0, 4.0)));
        assert_eq!(inductor_class("L_4.5X4.5"), Some((2.0, 4.5)));
        assert_eq!(inductor_class("INDUCTOR-CDRH2D18-LD"), Some((0.8, 2.0)));
        assert_eq!(inductor_class("Inductor_SMD:L_1210_3225Metric"), Some((1.5, 3.2)));
        assert!(names_family("ATmega328P-AU", MCU_FAMILIES));
    }
}
