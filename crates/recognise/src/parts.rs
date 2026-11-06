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
        assert!(names_family("ATmega328P-AU", MCU_FAMILIES));
    }
}
