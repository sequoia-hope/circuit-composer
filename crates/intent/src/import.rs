//! Draft intent from an easy_pcb project's phase-3 net classification, `design/nets.csv`
//! (`net,class,v_max,i_cont,i_peak,impedance,length_limit,reason`).
//!
//! Every value becomes a hard constraint citing its row, with the row's reason as the note. The
//! draft still has to pass the validator: nets.csv says nothing about aggressors, blocks,
//! domains or protection paths.

use std::collections::BTreeMap;

use crate::model::*;
use crate::netlist::Netlist;

pub const NETS_CSV_SOURCE: &str = "nets_csv";

#[derive(Debug, Clone, serde::Serialize)]
pub struct ImportNote {
    pub row: usize,
    pub net: String,
    pub message: String,
}

/// Builds the draft and lists what it had to guess or skip.
pub fn from_easy_pcb_nets_csv(text: &str, csv_location: &str, board: &str, netlist: Option<&Netlist>) -> Result<(IntentFile, Vec<ImportNote>), String> {
    let rows = parse_csv(text);
    let header = rows.first().ok_or("empty nets.csv")?;
    let col = |name: &str| header.iter().position(|h| h.trim() == name);
    let need = |name: &str| col(name).ok_or_else(|| format!("nets.csv has no `{name}` column"));
    let (c_net, c_class, c_v) = (need("net")?, need("class")?, need("v_max")?);
    let (c_i, c_ip, c_z, c_len, c_why) = (col("i_cont"), col("i_peak"), col("impedance"), col("length_limit"), col("reason"));

    let mut notes = Vec::new();
    let mut nets = BTreeMap::new();
    let mut short_to_kicad = BTreeMap::new();
    for (i, row) in rows.iter().enumerate().skip(1) {
        let line = i + 1;
        let get = |c: Option<usize>| c.and_then(|c| row.get(c)).map(|s| s.trim()).filter(|s| !s.is_empty());
        let Some(short) = get(Some(c_net)) else { continue };
        let name = match netlist {
            Some(n) => match n.find_net(short) {
                Some(k) => k.to_string(),
                None => {
                    notes.push(ImportNote { row: line, net: short.into(), message: "not in the netlist; skipped".into() });
                    continue;
                }
            },
            None => short.to_string(),
        };
        short_to_kicad.insert(short.to_string(), name.clone());
        let prov = |what: &str| Provenance {
            source: NETS_CSV_SOURCE.into(),
            locator: Some(format!("row {line}, {what}")),
            kind: ClaimKind::Requirement,
            date: None,
            note: get(c_why).map(str::to_string),
        };
        let num = |c: Option<usize>, what: &str, notes: &mut Vec<ImportNote>| -> Option<f64> {
            let s = get(c)?;
            match s.parse::<f64>() {
                Ok(v) => Some(v),
                Err(_) => {
                    notes.push(ImportNote { row: line, net: short.into(), message: format!("{what} `{s}` is not a number; skipped") });
                    None
                }
            }
        };

        let mut class = get(Some(c_class)).map(str::to_string);
        let v = num(Some(c_v), "v_max", &mut notes);
        if class.as_deref() == Some("POWER") && v == Some(0.0) {
            class = Some("GROUND".into());
        }
        let mut net = NetIntent { class, ..Default::default() };
        if let Some(v) = v {
            net.voltage = Some(hard(Voltage { peak_v: v }, prov("v_max")));
        }
        if let Some(ic) = num(c_i, "i_cont", &mut notes) {
            let peak = num(c_ip, "i_peak", &mut notes);
            net.current = Some(hard(Current { continuous_a: ic, peak_a: peak }, prov("i_cont/i_peak")));
        }
        if let Some(z) = get(c_z) {
            match parse_impedance(z) {
                Some((ohms, mode)) => {
                    let mut p = prov("impedance");
                    const ASSUMED: &str = "Tolerance not stated in nets.csv; 10 % assumed.";
                    p.note = Some(match p.note {
                        Some(n) => format!("{n}. {ASSUMED}"),
                        None => ASSUMED.into(),
                    });
                    net.impedance = Some(Attr {
                        value: Impedance { ohms, tolerance_pct: 10.0, mode, pair: None },
                        strength: Strength::Hard,
                        weight: None,
                        provenance: Some(p),
                    });
                }
                None => notes.push(ImportNote { row: line, net: short.into(), message: format!("impedance `{z}` not understood; skipped") }),
            }
        }
        if let Some(mm) = num(c_len, "length_limit", &mut notes) {
            net.max_length = Some(hard(Length { mm }, prov("length_limit")));
        }
        nets.insert(name, net);
    }

    // Differential partners: the net whose name differs by P/N, +/-, or DP/DM.
    let names: Vec<String> = nets.keys().cloned().collect();
    for name in &names {
        let diff = nets[name].impedance.as_ref().is_some_and(|z| z.value.mode == ImpedanceMode::Differential);
        if !diff {
            continue;
        }
        match partner(name).into_iter().find(|p| nets.contains_key(p)) {
            Some(p) => nets.get_mut(name).unwrap().impedance.as_mut().unwrap().value.pair = Some(p),
            None => notes.push(ImportNote { row: 0, net: name.clone(), message: "differential, but no partner net found by name".into() }),
        }
    }

    let mut sources = BTreeMap::new();
    sources.insert(
        NETS_CSV_SOURCE.to_string(),
        Source { title: "easy_pcb phase-3 net classification".into(), location: Some(csv_location.into()), revision: None, sha256: None },
    );
    let intent = IntentFile {
        schema_version: SCHEMA_VERSION.into(),
        board: BoardInfo { name: board.into(), kicad_project: None, layers: None, packages: BTreeMap::new() },
        sources,
        nets,
        blocks: BTreeMap::new(),
        domains: None,
        regions: Vec::new(),
        protection_paths: Vec::new(),
        energy_sources: Vec::new(),
    };
    Ok((intent, notes))
}

fn hard<T>(value: T, provenance: Provenance) -> Attr<T> {
    Attr { value, strength: Strength::Hard, weight: None, provenance: Some(provenance) }
}

/// `90 diff`, `50`, `50 se` → (ohms, mode).
fn parse_impedance(s: &str) -> Option<(f64, ImpedanceMode)> {
    let mut it = s.split_whitespace();
    let ohms: f64 = it.next()?.trim_end_matches(['R', 'Ω']).parse().ok()?;
    let mode = match it.next().map(str::to_ascii_lowercase).as_deref() {
        None | Some("se") | Some("single") => ImpedanceMode::SingleEnded,
        Some("diff") | Some("differential") => ImpedanceMode::Differential,
        _ => return None,
    };
    Some((ohms, mode))
}

/// Candidate partner names for one side of a differential pair.
fn partner(name: &str) -> Vec<String> {
    const SWAPS: &[(&str, &str)] = &[("DP", "DM"), ("D+", "D-"), ("_P", "_N"), ("+", "-"), ("P", "N")];
    let mut out = Vec::new();
    for (a, b) in SWAPS {
        for (from, to) in [(a, b), (b, a)] {
            // Replace the last occurrence only: `USB_DP_C` → `USB_DM_C`.
            if let Some(i) = name.rfind(from) {
                out.push(format!("{}{}{}", &name[..i], to, &name[i + from.len()..]));
            }
        }
    }
    out
}

/// RFC 4180-ish: commas, double-quoted fields with `""` escapes, LF or CRLF.
fn parse_csv(text: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match (quoted, c) {
            (true, '"') if chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            (true, '"') => quoted = false,
            (true, c) => field.push(c),
            (false, '"') => quoted = true,
            (false, ',') => row.push(std::mem::take(&mut field)),
            (false, '\r') => {}
            (false, '\n') => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
            }
            (false, c) => field.push(c),
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_quotes_and_commas() {
        let rows = parse_csv("a,b\n\"x, y\",\"say \"\"hi\"\"\"\n");
        assert_eq!(rows, vec![vec!["a", "b"], vec!["x, y", "say \"hi\""]]);
    }

    #[test]
    fn pairs_by_name() {
        assert!(partner("/USB_DP_C").contains(&"/USB_DM_C".to_string()));
        assert!(partner("/USB_DM_MCU").contains(&"/USB_DP_MCU".to_string()));
        assert!(partner("ETH_TX_P").contains(&"ETH_TX_N".to_string()));
    }

    #[test]
    fn impedance_forms() {
        assert_eq!(parse_impedance("90 diff"), Some((90.0, ImpedanceMode::Differential)));
        assert_eq!(parse_impedance("50"), Some((50.0, ImpedanceMode::SingleEnded)));
        assert_eq!(parse_impedance("fast"), None);
    }
}
