//! `ccomp`: Circuit Composer's command-line harness. Every command reads files, writes files,
//! and prints one JSON result on standard output, so the agent never parses free-form logs.
//!
//! Exit status: 0 success, 1 the input failed validation, 2 the command could not run.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use serde_json::{json, Value};

use intent::netlist::Netlist;
use intent::{presets, validate};

#[derive(Parser)]
#[command(name = "ccomp", version, about = "Circuit Composer: intent, packages and composition for KiCad boards")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Print the intent file's JSON Schema.
    Schema {
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// List the net class presets and block types.
    Classes,
    /// Read a KiCad netlist (`.net`, or a `.kicad_sch` exported with kicad-cli) as JSON.
    Netlist { path: PathBuf },
    /// Validate an intent file against its board's netlist.
    Validate {
        intent: PathBuf,
        /// The netlist or schematic; defaults to `board.kicad_project` in the intent file.
        #[arg(long)]
        netlist: Option<PathBuf>,
        /// Also write the report here.
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Print the nets with their class presets' soft defaults filled in.
    Resolve { intent: PathBuf },
    /// Draft an intent file from another tool's output.
    Import {
        #[command(subcommand)]
        from: Import,
    },
}

#[derive(Subcommand)]
enum Import {
    /// From an easy_pcb project's `design/nets.csv`.
    EasyPcb {
        nets_csv: PathBuf,
        #[arg(long)]
        board: String,
        /// The board's netlist or schematic, to key nets by their KiCad names.
        #[arg(long)]
        netlist: Option<PathBuf>,
        #[arg(short, long)]
        output: PathBuf,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli.cmd) {
        Ok((value, ok)) => {
            println!("{}", serde_json::to_string_pretty(&value).unwrap());
            if ok {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Err(e) => {
            println!("{}", json!({ "ok": false, "error": e }));
            ExitCode::from(2)
        }
    }
}

fn run(cmd: Cmd) -> Result<(Value, bool), String> {
    match cmd {
        Cmd::Schema { output } => {
            let schema = intent::json_schema();
            match output {
                Some(p) => {
                    write_json(&p, &schema)?;
                    Ok((json!({ "ok": true, "wrote": p }), true))
                }
                None => Ok((schema, true)),
            }
        }
        Cmd::Classes => {
            let classes: Vec<_> = presets::NET_CLASSES
                .iter()
                .map(|c| {
                    json!({
                        "name": c.name,
                        "summary": c.summary,
                        "required": c.required.iter().map(|f| f.name()).collect::<Vec<_>>(),
                        "default_aggressor": c.aggressor,
                        "default_victim": c.victim,
                    })
                })
                .collect();
            let blocks: Vec<_> = presets::BLOCK_TYPES
                .iter()
                .map(|b| json!({ "name": b.name, "summary": b.summary, "generator": b.generator }))
                .collect();
            Ok((json!({ "net_classes": classes, "block_types": blocks }), true))
        }
        Cmd::Netlist { path } => {
            let n = Netlist::load(&path).map_err(|e| e.to_string())?;
            Ok((serde_json::to_value(n).unwrap(), true))
        }
        Cmd::Validate { intent: path, netlist, output } => {
            let file = intent::load(&path)?;
            let netlist_path = netlist.or_else(|| file.board.kicad_project.as_ref().map(|p| beside(&path, p)));
            let netlist = match &netlist_path {
                Some(p) => Some(Netlist::load(p).map_err(|e| e.to_string())?),
                None => None,
            };
            let report = validate::validate(&file, netlist.as_ref());
            let value = serde_json::to_value(&report).unwrap();
            if let Some(o) = output {
                write_json(&o, &value)?;
            }
            Ok((value, report.ok))
        }
        Cmd::Resolve { intent: path } => {
            let file = intent::load(&path)?;
            let nets: serde_json::Map<String, Value> =
                file.nets.iter().map(|(k, n)| (k.clone(), serde_json::to_value(presets::resolve(n)).unwrap())).collect();
            Ok((json!({ "nets": nets }), true))
        }
        Cmd::Import { from: Import::EasyPcb { nets_csv, board, netlist, output } } => {
            let text = std::fs::read_to_string(&nets_csv).map_err(|e| format!("{}: {e}", nets_csv.display()))?;
            let netlist = match &netlist {
                Some(p) => Some(Netlist::load(p).map_err(|e| e.to_string())?),
                None => None,
            };
            let out_dir = output.parent().unwrap_or(Path::new("."));
            let location = relative(&nets_csv, out_dir);
            let (file, notes) = intent::import::from_easy_pcb_nets_csv(&text, &location, &board, netlist.as_ref())?;
            write_json(&output, &serde_json::to_value(&file).unwrap())?;
            Ok((json!({ "ok": true, "wrote": output, "nets": file.nets.len(), "notes": notes }), true))
        }
    }
}

/// A path written inside `file` resolved against `file`'s directory.
fn beside(file: &Path, rel: &str) -> PathBuf {
    file.parent().unwrap_or(Path::new(".")).join(rel)
}

/// `target` relative to `dir` when both canonicalise, else `target` as given.
fn relative(target: &Path, dir: &Path) -> String {
    let (Ok(t), Ok(d)) = (target.canonicalize(), dir.canonicalize()) else {
        return target.display().to_string();
    };
    let tc: Vec<_> = t.components().collect();
    let dc: Vec<_> = d.components().collect();
    let common = tc.iter().zip(&dc).take_while(|(a, b)| a == b).count();
    let mut out = PathBuf::new();
    for _ in common..dc.len() {
        out.push("..");
    }
    for c in &tc[common..] {
        out.push(c);
    }
    out.display().to_string()
}

fn write_json(path: &Path, value: &Value) -> Result<(), String> {
    let text = serde_json::to_string_pretty(value).unwrap() + "\n";
    std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}
