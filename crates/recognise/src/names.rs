//! What a net's name says about it. Half the corpus's nets are named by their designer, and a
//! name like `+3V3`, `USB_D+` or `XTAL_IN` is the strongest evidence there is.

/// The name's last hierarchical segment, upper case, KiCad's overbar markup removed:
/// `/power/+3V3` → `+3V3`, `~{RESET}` → `RESET`.
pub fn norm(name: &str) -> String {
    let last = name.rsplit('/').find(|s| !s.is_empty()).unwrap_or(name);
    last.replace("~{", "").replace('}', "").to_ascii_uppercase()
}

/// A name KiCad made up for a net nobody named.
pub fn is_auto(name: &str) -> bool {
    let n = name.trim_start_matches('/');
    n.is_empty() || n.starts_with("Net-(") || n.starts_with("unconnected-") || n.starts_with("N-0") || n.starts_with("N$") || n.starts_with("/N$")
}

/// Splits a name into words at anything not a letter, digit, `+`, `-` or `.`.
fn words(n: &str) -> Vec<&str> {
    n.split(|c: char| !(c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.')).filter(|w| !w.is_empty()).collect()
}

pub fn is_ground(n: &str) -> bool {
    const GROUNDS: &[&str] = &["GND", "DGND", "AGND", "PGND", "SGND", "GNDA", "GNDD", "GNDPWR", "GNDREF", "GROUND", "VSS", "VSSA", "AVSS", "DVSS", "0V", "EARTH", "CHASSIS", "GNDS", "-BATT", "BAT-", "VBAT-", "GND1", "GND2", "VEE"];
    let n = norm(n);
    // `VEE` is a negative rail on analog boards, but its pins are grounds on ECL and most MCUs.
    GROUNDS.contains(&n.as_str()) || n.starts_with("GND_") || n.ends_with("_GND") || n.starts_with("GND") && n[3..].chars().all(|c| c.is_ascii_digit() || c == '_')
}

/// A rail's nominal voltage from its name: `+3V3`, `3.3V`, `VCC_5V`, `+12V`, `1V8`, `-5V`,
/// `VBUS` (5 V).
pub fn rail_voltage(n: &str) -> Option<f64> {
    let n = norm(n);
    if n.contains("VBUS") || n.contains("VUSB") || n == "USB_5V" {
        return Some(5.0);
    }
    for w in words(&n) {
        let neg = w.starts_with('-');
        let w = w.trim_start_matches(['+', '-']);
        // Find `<digits>V<digits>` or `<digits[.digits]>V` anywhere in the word: `VCC3V3`.
        let b = w.as_bytes();
        for i in 0..b.len() {
            if b[i] != b'V' || i == 0 || !b[i - 1].is_ascii_digit() {
                continue;
            }
            let start = w[..i].rfind(|c: char| !(c.is_ascii_digit() || c == '.')).map_or(0, |j| j + 1);
            let int = &w[start..i];
            let frac: String = w[i + 1..].chars().take_while(|c| c.is_ascii_digit()).collect();
            let text = if frac.is_empty() || int.contains('.') { int.to_string() } else { format!("{int}.{frac}") };
            if let Ok(v) = text.parse::<f64>() {
                if v > 0.0 && v <= 1000.0 {
                    return Some(if neg { -v } else { v });
                }
            }
        }
    }
    None
}

/// A supply name with no voltage in it.
pub fn is_supply_word(n: &str) -> bool {
    const SUPPLIES: &[&str] = &[
        "VCC", "VDD", "VIN", "VSYS", "VAA", "AVDD", "DVDD", "AVCC", "DVCC", "VCCIO", "VDDIO", "VDDA", "VCCA", "PWR", "POWER", "V+", "VBAT", "VBATT", "+BATT", "BATT", "BAT", "BAT+",
        "VBAT+", "VCORE", "VREG", "VOUT", "VPP", "VM", "VMOT", "VMOTOR", "VS", "VCC_IO", "VLED", "VPWR", "VRAW", "RAW", "VDD_IO", "VDDCORE", "IOVDD", "DVDD_IO", "VDDQ", "+VIN",
        "VIN+", "V_IN", "V_OUT", "VSUPPLY", "SUPPLY", "+VCC", "+VDD", "VCHG", "VCHRG",
    ];
    let n = norm(n);
    if SUPPLIES.contains(&n.as_str()) {
        return true;
    }
    // `VCC_MCU`, `VDD_RF`, `+VBAT`: a supply word followed by a qualifier.
    let head = n.split('_').next().unwrap_or("");
    matches!(head.trim_start_matches('+'), "VCC" | "VDD" | "AVDD" | "DVDD" | "VBAT" | "VSYS" | "VIN" | "PVIN" | "VDDA" | "VCCA" | "AVCC")
}

/// What kind of pair a name belongs to, by the keywords around its P/N or +/- suffix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairKind {
    Usb,
    Ethernet,
    Lvds,
    Can,
    Rs485,
    Hdmi,
}

impl PairKind {
    /// Differential impedance, ohms, and what it rests on.
    pub fn impedance(self) -> (f64, &'static str) {
        match self {
            PairKind::Usb => (90.0, "USB 2.0 specifies 90 ohm differential"),
            PairKind::Ethernet => (100.0, "Ethernet media pairs are 100 ohm differential"),
            PairKind::Lvds => (100.0, "LVDS is terminated in 100 ohm"),
            PairKind::Can => (120.0, "CAN is terminated in 120 ohm"),
            PairKind::Rs485 => (120.0, "RS-485 is terminated in 120 ohm"),
            PairKind::Hdmi => (100.0, "HDMI TMDS pairs are 100 ohm differential"),
        }
    }

    /// Whether a pair of this kind is fast enough to be a DIFF_HS net; CAN and RS-485 are
    /// differential but slow.
    pub fn high_speed(self) -> bool {
        !matches!(self, PairKind::Can | PairKind::Rs485)
    }
}

pub fn pair_kind(n: &str, connector_hint: bool) -> Option<PairKind> {
    let n = norm(n);
    let has = |k: &str| n.contains(k);
    if has("USB") || connector_hint {
        return Some(PairKind::Usb);
    }
    if has("CAN") {
        return Some(PairKind::Can);
    }
    if has("485") || has("422") {
        return Some(PairKind::Rs485);
    }
    if has("ETH") || has("MDI") || has("TD") || has("RD") || has("TX") && has("RX") || has("PHY") {
        return Some(PairKind::Ethernet);
    }
    if has("LVDS") || has("MIPI") || has("CSI") || has("DSI") {
        return Some(PairKind::Lvds);
    }
    if has("HDMI") || has("TMDS") {
        return Some(PairKind::Hdmi);
    }
    None
}

/// The partner of one side of a differential pair, for the suffix styles designers use, with
/// how the pair is written: `D+`/`D-`, `DP`/`DM`, `_P`/`_N`, `+`/`-`, `H`/`L` (CAN).
pub fn partners(name: &str) -> Vec<String> {
    const SWAPS: &[(&str, &str)] = &[("D+", "D-"), ("DP", "DM"), ("DP", "DN"), ("_P", "_N"), ("+", "-"), ("P", "N"), ("_H", "_L"), ("H", "L")];
    let mut out = Vec::new();
    for (a, b) in SWAPS {
        for (from, to) in [(a, b), (b, a)] {
            // The last occurrence only: `USB_DP_C` → `USB_DM_C`.
            if let Some(i) = name.rfind(from) {
                out.push(format!("{}{}{}", &name[..i], to, &name[i + from.len()..]));
            }
        }
    }
    out
}

/// Words that name a crystal or oscillator net.
pub fn is_clock_name(n: &str) -> bool {
    let n = norm(n);
    ["XTAL", "XIN", "XOUT", "XI", "XO", "OSC", "XTAL1", "XTAL2", "OSC_IN", "OSC_OUT", "OSC32", "XC1", "XC2", "XL1", "XL2"].iter().any(|k| n == *k || n.starts_with(&format!("{k}_")) || n.ends_with(&format!("_{k}")))
        || n.contains("XTAL")
        || n.contains("CRYSTAL")
}

/// Names that say a net is a fast single-ended bus: QSPI, SDIO, parallel memory.
pub fn fast_bus(n: &str) -> Option<&'static str> {
    let n = norm(n);
    // `VDDR_IO` is the DDR supply; `SD_DET` a card-detect switch.
    if n.starts_with('V') || n.contains("VDD") || n.contains("VTT") || n.contains("VREF") || ["DET", "CD", "WP", "PWR", "EN"].iter().any(|w| words(&n).contains(w)) {
        return None;
    }
    if n.contains("QSPI") || n.contains("QSPI") || n.starts_with("FLASH_") || n.starts_with("PSRAM") {
        return Some("QSPI");
    }
    if n.contains("SDIO") || n.starts_with("SD_D") || n == "SD_CLK" || n == "SD_CMD" || n.starts_with("SDMMC") || n.starts_with("EMMC") {
        return Some("SDIO");
    }
    if n.contains("DDR") || n.contains("SDRAM") || n.starts_with("FMC_") || n.starts_with("RGMII") || n.starts_with("RMII") || n.starts_with("ULPI") {
        return Some("parallel memory or PHY bus");
    }
    None
}

/// Names that say a net is analog and sensitive.
pub fn analog_name(n: &str) -> Option<&'static str> {
    let n = norm(n);
    let ws = words(&n);
    let any = |ks: &[&str]| ws.iter().any(|w| ks.iter().any(|k| w == k || w.starts_with(k) && w[k.len()..].chars().all(|c| c.is_ascii_digit() || c == '+' || c == '-')));
    // An ADC's digital side: `ADC_RDY`, `ADC_CS`, `ADC_SCK`.
    const DIGITAL: &[&str] = &["RDY", "DRDY", "CS", "NCS", "SCK", "SCLK", "MISO", "MOSI", "CLK", "SDA", "SCL", "EN", "RST", "RESET", "INT", "IRQ", "DIN", "DOUT", "SYNC", "START", "BUSY", "TX", "RX", "DATA", "SDI", "SDO", "ALERT", "PWDN", "LRCK", "BCLK", "MCLK", "WS"];
    if ws.iter().any(|w| DIGITAL.contains(w)) {
        return None;
    }
    if any(&["FB", "VFB", "FEEDBACK", "ADJ"]) {
        return Some("feedback");
    }
    if any(&["ISENSE", "ISNS", "SENSE", "SNS", "SHUNT", "IMON", "IPROPI"]) {
        return Some("current sense");
    }
    if any(&["VREF", "AREF", "REF", "REFIN", "REFOUT"]) {
        return Some("reference");
    }
    if any(&["AIN", "ADC", "AN", "ANALOG", "MIC", "AUDIO", "THERM", "NTC", "TEMP", "VSENSE", "VMON", "BAT_SENSE", "VBAT_SENSE"]) {
        return Some("analog input");
    }
    None
}

pub fn switch_name(n: &str) -> bool {
    let n = norm(n);
    let ws = words(&n);
    ws.iter().any(|w| matches!(*w, "SW" | "LX" | "LX1" | "LX2" | "PH" | "PHASE" | "SWN" | "SWNODE"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rails() {
        assert_eq!(rail_voltage("+3V3"), Some(3.3));
        assert_eq!(rail_voltage("/power/+3.3V"), Some(3.3));
        assert_eq!(rail_voltage("VCC_5V"), Some(5.0));
        assert_eq!(rail_voltage("1V8"), Some(1.8));
        assert_eq!(rail_voltage("-12V"), Some(-12.0));
        assert_eq!(rail_voltage("VBUS"), Some(5.0));
        assert_eq!(rail_voltage("VCC3V3"), Some(3.3));
        assert_eq!(rail_voltage("SDA"), None);
        assert_eq!(rail_voltage("LED_V2"), None);
        assert!(is_ground("GND") && is_ground("/AGND") && is_ground("GND_ISO") && !is_ground("GNDSENSE"));
        assert!(is_supply_word("VCC") && is_supply_word("VDD_RF") && !is_supply_word("VCCX_EN"));
    }

    #[test]
    fn pairs_and_names() {
        assert!(partners("/USB_D+").contains(&"/USB_D-".to_string()));
        assert!(partners("USB_DP_C").contains(&"USB_DM_C".to_string()));
        assert!(partners("CANH").contains(&"CANL".to_string()));
        assert_eq!(pair_kind("ETH_TX_P", false), Some(PairKind::Ethernet));
        assert_eq!(analog_name("VFB"), Some("feedback"));
        assert_eq!(analog_name("ADC0"), Some("analog input"));
        assert_eq!(analog_name("SDA"), None);
        assert_eq!(analog_name("ADC_RDY"), None);
        assert_eq!(fast_bus("VDDR_IO"), None);
        assert_eq!(fast_bus("SD_DET"), None);
        assert_eq!(fast_bus("SDIO_D3"), Some("SDIO"));
        assert_eq!(pair_kind("RSTH", false), None);
        assert!(is_clock_name("XTAL_IN") && is_clock_name("XIN") && !is_clock_name("SCK"));
        assert!(switch_name("SW") && switch_name("BUCK_SW") && !switch_name("SWDIO"));
        assert_eq!(norm("~{RESET}"), "RESET");
        assert!(is_auto("Net-(C1-Pad1)") && !is_auto("/GPIO0"));
    }
}
