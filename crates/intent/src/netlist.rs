//! The KiCad netlist the intent file is keyed to: components and nets, read from the
//! S-expression netlist `kicad-cli sch export netlist --format kicadsexpr` writes.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum NetlistError {
    #[error("{0}: {1}")]
    Io(String, std::io::Error),
    #[error("netlist parse: {0}")]
    Parse(String),
    #[error("kicad-cli failed on {0}: {1}")]
    KicadCli(String, String),
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Netlist {
    /// Components by reference designator.
    pub components: BTreeMap<String, Component>,
    /// Nets by KiCad name.
    pub nets: BTreeMap<String, Net>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Component {
    pub value: String,
    pub footprint: String,
    /// User fields (`LCSC`, `MPN`, ...), empty ones left out.
    pub fields: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Net {
    /// The net class KiCad assigned, if any other than `Default`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class: Option<String>,
    pub nodes: Vec<Node>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Node {
    #[serde(rename = "ref")]
    pub reference: String,
    pub pin: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function: Option<String>,
}

impl Net {
    /// A net KiCad made for a single unused pin.
    pub fn is_unconnected(&self, name: &str) -> bool {
        name.starts_with("unconnected-") || self.nodes.len() < 2
    }

    pub fn has_pad(&self, reference: &str, pin: &str) -> bool {
        self.nodes.iter().any(|n| n.reference == reference && n.pin == pin)
    }
}

impl Netlist {
    /// Reads a netlist file, or exports one from a `.kicad_sch` (or a `.kicad_pro`, whose
    /// schematic sits beside it) with `kicad-cli`.
    pub fn load(path: &Path) -> Result<Netlist, NetlistError> {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let text = match ext {
            "kicad_sch" | "kicad_pro" => export(&path.with_extension("kicad_sch"))?,
            _ => std::fs::read_to_string(path).map_err(|e| NetlistError::Io(path.display().to_string(), e))?,
        };
        Netlist::parse(&text)
    }

    pub fn parse(text: &str) -> Result<Netlist, NetlistError> {
        let root = sexpr::parse(text).map_err(NetlistError::Parse)?;
        if root.head() != Some("export") {
            return Err(NetlistError::Parse("not a KiCad S-expression netlist (no `export` root)".into()));
        }
        let mut out = Netlist::default();
        for comp in root.child("components").into_iter().flat_map(|c| c.children("comp")) {
            let reference = comp.value_of("ref").unwrap_or_default().to_string();
            let mut c = Component {
                value: comp.value_of("value").unwrap_or_default().to_string(),
                footprint: comp.value_of("footprint").unwrap_or_default().to_string(),
                fields: BTreeMap::new(),
            };
            for f in comp.child("fields").into_iter().flat_map(|f| f.children("field")) {
                if let (Some(name), Some(value)) = (f.value_of("name"), f.atom(2)) {
                    if !value.is_empty() && !matches!(name, "Footprint" | "Datasheet" | "Description") {
                        c.fields.insert(name.to_string(), value.to_string());
                    }
                }
            }
            out.components.insert(reference, c);
        }
        for net in root.child("nets").into_iter().flat_map(|n| n.children("net")) {
            let name = net.value_of("name").unwrap_or_default().to_string();
            let class = net.value_of("class").filter(|c| *c != "Default").map(str::to_string);
            let nodes = net
                .children("node")
                .map(|n| Node {
                    reference: n.value_of("ref").unwrap_or_default().to_string(),
                    pin: n.value_of("pin").unwrap_or_default().to_string(),
                    function: n.value_of("pinfunction").map(str::to_string),
                })
                .collect();
            out.nets.insert(name, Net { class, nodes });
        }
        Ok(out)
    }

    /// The KiCad net name an intent key refers to: exact first, then with or without the root
    /// sheet's leading `/`, so `GPIO0` finds `/GPIO0`.
    pub fn find_net(&self, name: &str) -> Option<&str> {
        if let Some((k, _)) = self.nets.get_key_value(name) {
            return Some(k);
        }
        let alt = match name.strip_prefix('/') {
            Some(bare) => bare.to_string(),
            None => format!("/{name}"),
        };
        self.nets.get_key_value(&alt).map(|(k, _)| k.as_str())
    }
}

fn export(sch: &Path) -> Result<String, NetlistError> {
    let out_path = std::env::temp_dir().join(format!("ccomp-netlist-{}.net", std::process::id()));
    let out = Command::new("kicad-cli")
        .args(["sch", "export", "netlist", "--format", "kicadsexpr", "-o"])
        .arg(&out_path)
        .arg(sch)
        .output()
        .map_err(|e| NetlistError::Io("kicad-cli".into(), e))?;
    if !out.status.success() {
        return Err(NetlistError::KicadCli(sch.display().to_string(), String::from_utf8_lossy(&out.stderr).into()));
    }
    let text = std::fs::read_to_string(&out_path).map_err(|e| NetlistError::Io(out_path.display().to_string(), e))?;
    let _ = std::fs::remove_file(&out_path);
    Ok(text)
}

/// A minimal S-expression reader: lists, bare atoms and quoted strings.
mod sexpr {
    #[derive(Debug, Clone)]
    pub enum Sx {
        Atom(String),
        List(Vec<Sx>),
    }

    impl Sx {
        pub fn head(&self) -> Option<&str> {
            self.atom(0)
        }

        /// The atom at index `i` of a list.
        pub fn atom(&self, i: usize) -> Option<&str> {
            match self {
                Sx::List(items) => match items.get(i) {
                    Some(Sx::Atom(a)) => Some(a),
                    _ => None,
                },
                Sx::Atom(_) => None,
            }
        }

        pub fn children<'a>(&'a self, head: &'a str) -> impl Iterator<Item = &'a Sx> + 'a {
            let items: &[Sx] = match self {
                Sx::List(items) => items,
                Sx::Atom(_) => &[],
            };
            items.iter().filter(move |c| c.head() == Some(head))
        }

        pub fn child<'a>(&'a self, head: &'a str) -> Option<&'a Sx> {
            self.children(head).next()
        }

        /// `(head value)` → value.
        pub fn value_of<'a>(&'a self, head: &'a str) -> Option<&'a str> {
            self.child(head).and_then(|c| c.atom(1))
        }
    }

    pub fn parse(text: &str) -> Result<Sx, String> {
        let mut stack: Vec<Vec<Sx>> = vec![Vec::new()];
        let mut chars = text.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '(' => stack.push(Vec::new()),
                ')' => {
                    let list = stack.pop().ok_or("unbalanced )")?;
                    stack.last_mut().ok_or("unbalanced )")?.push(Sx::List(list));
                }
                '"' => {
                    let mut s = String::new();
                    loop {
                        match chars.next() {
                            Some('\\') => match chars.next() {
                                Some('n') => s.push('\n'),
                                Some(e) => s.push(e),
                                None => return Err("unterminated string".into()),
                            },
                            Some('"') => break,
                            Some(ch) => s.push(ch),
                            None => return Err("unterminated string".into()),
                        }
                    }
                    stack.last_mut().ok_or("string outside a list")?.push(Sx::Atom(s));
                }
                c if c.is_whitespace() => {}
                c => {
                    let mut s = String::from(c);
                    while let Some(&n) = chars.peek() {
                        if n.is_whitespace() || n == '(' || n == ')' {
                            break;
                        }
                        s.push(n);
                        chars.next();
                    }
                    stack.last_mut().ok_or("atom outside a list")?.push(Sx::Atom(s));
                }
            }
        }
        if stack.len() != 1 {
            return Err("unbalanced (".into());
        }
        stack.pop().unwrap().into_iter().next().ok_or_else(|| "empty input".into())
    }
}
