//! `ccomp`: Circuit Composer's command-line harness. Every command reads files, writes files,
//! and prints one JSON result on standard output, so the agent never parses free-form logs.
//!
//! Exit status: 0 success, 1 the input failed validation, 2 the command could not run.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use serde_json::{json, Value};

use intent::netlist::Netlist;
use intent::{presets, validate};

mod floorplan_cmd;
mod package_check;
mod project;

#[derive(Parser)]
#[command(name = "ccomp", version, about = "Circuit Composer: intent, packages and composition for KiCad boards")]
struct Cli {
    /// The project: an easy_pcb project (boards under `compose/`) or this repository (boards
    /// under `boards/`). Inputs a command is not given come from the board's files.
    #[arg(short = 'C', long, global = true)]
    project: Option<PathBuf>,
    /// The board: a name in the project, or a board directory. Needed when the project has more
    /// than one.
    #[arg(short = 'b', long = "board", global = true)]
    board_name: Option<String>,
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
    /// A project's boards, or with `--board` (or a project of one) that board's files: where each
    /// is and whether it exists.
    Boards,
    /// Read a KiCad netlist (`.net`, or a `.kicad_sch` exported with kicad-cli) as JSON.
    Netlist { path: PathBuf },
    /// Validate an intent file against its board's netlist.
    Validate {
        /// The intent file; the board's with `--project` / `--board`.
        intent: Option<PathBuf>,
        /// The netlist or schematic; defaults to `board.kicad_project` in the intent file.
        #[arg(long)]
        netlist: Option<PathBuf>,
        /// Also write the report here.
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Print the nets with their class presets' soft defaults filled in.
    Resolve { intent: Option<PathBuf> },
    /// Fast-tier verifier checks on a block of a KiCad board.
    Check {
        #[command(subcommand)]
        kind: Check,
    },
    /// Net-level measurements on a board, as a spec file lists them: pad-to-pad copper paths,
    /// track with no ground under it, each power pin's nearest decoupling capacitor.
    Measure {
        board: PathBuf,
        /// `{"paths": [{"name", "from": "U1.21", "to": "Y1.1"}], "return_paths": {"ground", "nets",
        /// "step"}, "decoupling": {"part", "nets", "ground"}}`; every key optional.
        #[arg(long)]
        spec: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Generate a block's sub-layout.
    Gen {
        #[command(subcommand)]
        kind: Gen,
    },
    /// Compile intent to router regions (`routing.json`) and KiCad custom rules.
    Compile {
        intent: Option<PathBuf>,
        /// The KiCad board the intent is for (anchored regions are placed on it).
        #[arg(long)]
        pcb: Option<PathBuf>,
        /// Write routing.json here.
        #[arg(short, long)]
        output: PathBuf,
        /// Also write KiCad custom rules here, merged into `--dru-base` if given.
        #[arg(long)]
        dru: Option<PathBuf>,
        /// An existing `.kicad_dru` to keep: its rules stay, the circuit-composer block is replaced.
        #[arg(long, requires = "dru")]
        dru_base: Option<PathBuf>,
    },
    /// Draft an intent file from another tool's output.
    Import {
        #[command(subcommand)]
        from: Import,
    },
    /// A board from package instances, its own parts and the nets joining them: one flat
    /// circuit (parts by designator, nets by pin) and a binding per instance.
    Compose {
        composition: Option<PathBuf>,
        /// Where packages are found, as `DIR/NAME/manifest.toml` (default `packages`, or the
        /// board's with `--project` / `--board`).
        #[arg(long)]
        packages: Option<PathBuf>,
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Also write each instance's binding here, as `INSTANCE.binding.json`.
        #[arg(long)]
        bindings: Option<PathBuf>,
    },
    /// Place every part of a composed board: simulated annealing on wirelength, courtyard
    /// overlap and board area, with block cohesion, placement hints, rigid generated layouts,
    /// edge connectors and block separation. Prints the placement and the outline it needs.
    Floorplan {
        /// The composed board (`compose_kicad.py`), footprints anywhere.
        pcb: Option<PathBuf>,
        /// `ccomp compose -o` output.
        #[arg(long)]
        composed: Option<PathBuf>,
        /// Edges, rigid layouts, separations, options (`floorplan.json`).
        #[arg(long)]
        spec: Option<PathBuf>,
        /// Paths in the spec are relative to this (default `.`, or the packages directory's
        /// parent with `--project` / `--board`).
        #[arg(long)]
        root: Option<PathBuf>,
        #[arg(long)]
        seed: Option<u64>,
        /// Annealing moves (the spec's or 400 000 otherwise).
        #[arg(long)]
        moves: Option<usize>,
        /// Score this board's own placement instead of solving (the parts as placed there).
        #[arg(long)]
        evaluate: Option<PathBuf>,
        /// Print the problem the floorplanner would solve (parts, nets, groups, outline) and stop:
        /// what the app's in-browser annealer runs on.
        #[arg(long, conflicts_with = "evaluate")]
        problem: bool,
        /// Take every part and net from the board itself (its pads' nets) instead of a
        /// composition: any KiCad board becomes a floorplan problem (benchmarks, others' boards).
        #[arg(long, conflicts_with = "composed")]
        nets_from_board: bool,
        /// The board's own Edge.Cuts outline is the one to place inside (unless the spec sets one);
        /// with no Edge.Cuts, the box its parts' courtyards fill as placed.
        #[arg(long)]
        outline_from_board: bool,
        /// Solve this saved problem (`--problem` output) instead of building one.
        #[arg(long, conflicts_with_all = ["pcb", "composed", "spec", "nets_from_board", "outline_from_board", "evaluate"])]
        input_problem: Option<PathBuf>,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// A board's shape (`shape.json`): its outline, holes and keep-outs, evaluated to polygons.
    Shape {
        /// The shape document; the board's with `--project` / `--board`.
        shape: Option<PathBuf>,
        /// List the parametric generators and their parameters instead.
        #[arg(long, conflicts_with = "import")]
        generators: bool,
        /// Make a shape document from a drawing (`.svg` or `.dxf`) instead: each closed shape
        /// becomes a polygon feature.
        #[arg(long)]
        import: Option<PathBuf>,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// The research library: cited claims from `library/` and every package's `rules.json`.
    Library {
        #[command(subcommand)]
        op: LibraryOp,
    },
}

#[derive(Subcommand)]
enum LibraryOp {
    /// Check the database: unique ids, known statuses, every citation's source and every link's
    /// claim found, contradictions recorded on both sides.
    Check {
        /// The repository root holding `library/` and `packages/`.
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },
    /// Claims matching every filter given, with their sources and the links that point at them.
    Query {
        #[arg(long, default_value = ".")]
        root: PathBuf,
        /// A set (`library/buck`, `packages/rp2350`, or just `buck`).
        #[arg(long)]
        set: Option<String>,
        /// A claim id, or an id prefix (`buck.layout`).
        #[arg(long)]
        id: Option<String>,
        /// A scope key and value, `circuit=buck`; repeatable.
        #[arg(long, value_parser = parse_kv)]
        scope: Vec<(String, String)>,
        #[arg(long)]
        attribute: Option<String>,
        #[arg(long)]
        status: Option<String>,
        /// Words in the statement.
        #[arg(long)]
        text: Option<String>,
    },
}

fn parse_kv(s: &str) -> Result<(String, String), String> {
    s.split_once('=').map(|(k, v)| (k.to_string(), v.to_string())).ok_or_else(|| format!("{s}: expected KEY=VALUE"))
}

#[derive(Subcommand)]
enum Check {
    /// A board's copper against a routing.json's region rules (`never` copper inside, `must`
    /// copper outside); `avoid`/`prefer` are reported with the length that pays them.
    Regions {
        pcb: Option<PathBuf>,
        #[arg(long)]
        routing: Option<PathBuf>,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// A buck converter: hot-loop and output-loop area, switch-node area, feedback clearance,
    /// copper under the switch node.
    Buck {
        board: PathBuf,
        /// Take the block's roles and limits from this intent file ...
        #[arg(long, requires = "block")]
        intent: Option<PathBuf>,
        /// ... and this block in it.
        #[arg(long)]
        block: Option<String>,
        /// Or give the roles directly: `vin=U1.49,sw=U1.48,pgnd=U1.47,fb=U1.50,cin=C6,cout=C7,inductor=L1`.
        #[arg(long, conflicts_with = "intent")]
        roles: Option<String>,
        /// Extra limits, `check<=value` or `check>=value`; repeatable.
        #[arg(long)]
        limit: Vec<String>,
        /// Draw the block with its loops to this SVG.
        #[arg(long)]
        svg: Option<PathBuf>,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// A board against a package's blocks, by role: net wiring, block constraints, net length
    /// limits and part values, each judged with its source.
    Package {
        board: PathBuf,
        /// The package directory (`packages/rp2350`).
        #[arg(long)]
        package: PathBuf,
        /// `{"package": "rp2350@0.2.0", "variant": "RP2350A", "chip": "U1", "parts": {"CIN": "C14", ...}}`.
        #[arg(long)]
        binding: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum Gen {
    /// Place CIN, the inductor and COUT around a regulator IC, with their copper, in the IC's frame.
    Buck {
        /// A board with the IC and the three parts on it (their footprints are read from it).
        board: PathBuf,
        #[arg(long, requires = "block")]
        intent: Option<PathBuf>,
        #[arg(long)]
        block: Option<String>,
        #[arg(long, conflicts_with = "intent")]
        roles: Option<String>,
        /// Generator parameters as JSON (any subset of the defaults `ccomp gen buck --defaults` prints).
        #[arg(long)]
        params: Option<PathBuf>,
        /// Print the default parameters and stop.
        #[arg(long)]
        defaults: bool,
        /// Write the layout here.
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum Import {
    /// From an easy_pcb project's `design/nets.csv`, for the board named by `--board`.
    EasyPcb {
        nets_csv: PathBuf,
        /// The board's netlist or schematic, to key nets by their KiCad names.
        #[arg(long)]
        netlist: Option<PathBuf>,
        #[arg(short, long)]
        output: PathBuf,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let ctx = Ctx { project: cli.project, board: cli.board_name, resolved: None };
    match run(cli.cmd, ctx) {
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

/// `--project` / `--board`, resolved on first use: a command that was given all its inputs never
/// needs a board.
struct Ctx {
    project: Option<PathBuf>,
    board: Option<String>,
    resolved: Option<project::Board>,
}

impl Ctx {
    /// Given `--project` or `--board`, or run inside a project with its boards under `compose/`.
    fn active(&self) -> bool {
        self.project.is_some() || self.board.is_some() || Path::new("compose").is_dir()
    }

    fn board(&mut self) -> Result<&project::Board, String> {
        if self.resolved.is_none() {
            self.resolved = Some(project::resolve(self.project.as_deref(), self.board.as_deref())?);
        }
        Ok(self.resolved.as_ref().unwrap())
    }

    /// An input as given, else the board's: `what` names it in the error when there is no board.
    fn or_board(&mut self, given: Option<PathBuf>, what: &str, pick: impl Fn(&project::Board) -> PathBuf) -> Result<PathBuf, String> {
        match given {
            Some(p) => Ok(p),
            None if self.active() => Ok(pick(self.board()?)),
            None => Err(format!("give {what}, or --project / --board")),
        }
    }
}

fn run(cmd: Cmd, mut ctx: Ctx) -> Result<(Value, bool), String> {
    match cmd {
        Cmd::Boards => {
            if ctx.board.is_none() {
                let project = ctx.project.clone().unwrap_or_else(|| PathBuf::from("."));
                let names = project::list(&project)?;
                if names.len() != 1 {
                    return Ok((json!({ "ok": true, "project": project, "root": project::boards_root(&project), "boards": names }), true));
                }
            }
            let b = ctx.board()?;
            let mut value = serde_json::to_value(b).unwrap();
            value["ok"] = true.into();
            value["exists"] = b.present();
            Ok((value, true))
        }
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
            let path = ctx.or_board(path, "an intent file", |b| b.intent.clone())?;
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
            let path = ctx.or_board(path, "an intent file", |b| b.intent.clone())?;
            let file = intent::load(&path)?;
            let nets: serde_json::Map<String, Value> =
                file.nets.iter().map(|(k, n)| (k.clone(), serde_json::to_value(presets::resolve(n)).unwrap())).collect();
            Ok((json!({ "nets": nets }), true))
        }
        Cmd::Measure { board, spec, output } => {
            let b = pcb::Board::load(&board)?;
            let text = std::fs::read_to_string(&spec).map_err(|e| format!("{}: {e}", spec.display()))?;
            let spec: MeasureSpec = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", spec.display()))?;
            let mut errors = Vec::new();
            let mut paths = serde_json::Map::new();
            for p in &spec.paths {
                match verify::nets::path(&b, &p.from, &p.to) {
                    Ok(r) => {
                        paths.insert(p.name.clone(), serde_json::to_value(r).unwrap());
                    }
                    Err(e) => errors.push(json!({ "path": p.name, "error": e })),
                }
            }
            let returns: Vec<_> = match &spec.return_paths {
                Some(r) => r.nets.iter().map(|n| verify::nets::return_path(&b, n, &r.ground, r.step.unwrap_or(0.05))).collect(),
                None => Vec::new(),
            };
            let decoupling = match &spec.decoupling {
                Some(d) => serde_json::to_value(verify::nets::decoupling(&b, &d.part, &d.nets, &d.ground)?).unwrap(),
                None => Value::Null,
            };
            let value = json!({ "ok": errors.is_empty(), "paths": paths, "return_paths": returns, "decoupling": decoupling, "errors": errors });
            if let Some(o) = output {
                write_json(&o, &value)?;
            }
            Ok((value, errors.is_empty()))
        }
        Cmd::Check { kind: Check::Regions { pcb: board, routing, output } } => {
            let board = ctx.or_board(board, "a board", |b| b.pcb.clone())?;
            let routing = ctx.or_board(routing, "--routing", |b| b.routing.clone())?;
            let b = pcb::Board::load(&board)?;
            let text = std::fs::read_to_string(&routing).map_err(|e| format!("{}: {e}", routing.display()))?;
            let r: compile::Routing = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", routing.display()))?;
            let findings = compile::check_regions(&b, &r);
            let violations = findings.iter().filter(|f| f.violation).count();
            let value = json!({ "ok": violations == 0, "violations": violations, "findings": findings });
            if let Some(o) = output {
                write_json(&o, &value)?;
            }
            Ok((value, violations == 0))
        }
        Cmd::Check { kind: Check::Buck { board, intent: intent_path, block, roles, limit, svg, output } } => {
            let b = pcb::Board::load(&board)?;
            let (name, role_map, mut limits) = block_roles(intent_path.as_deref(), block, roles)?;
            for l in &limit {
                let (check, op, v) = if let Some((c, v)) = l.split_once("<=") {
                    (c, "<=", v)
                } else if let Some((c, v)) = l.split_once(">=") {
                    (c, ">=", v)
                } else {
                    return Err(format!("limit `{l}` is not check<=value or check>=value"));
                };
                let value = v.trim().parse::<f64>().map_err(|_| format!("limit `{l}`: not a number"))?;
                limits.insert(check.trim().to_string(), verify::Limit { op, value });
            }
            let roles = verify::buck::BuckRoles::from_map(&role_map)?;
            let report = verify::buck::check(&b, &name, &roles, &limits)?;
            if let Some(p) = svg {
                let title = format!("{} — {}", board.file_stem().unwrap_or_default().to_string_lossy(), name);
                std::fs::write(&p, verify::svg::render(&b, &report, &title, 1.0)).map_err(|e| format!("{}: {e}", p.display()))?;
            }
            let ok = report.measurements.iter().all(|m| m.passed != Some(false));
            let mut value = serde_json::to_value(&report).unwrap();
            value["ok"] = json!(ok);
            if let Some(o) = output {
                write_json(&o, &value)?;
            }
            Ok((value, ok))
        }
        Cmd::Check { kind: Check::Package { board, package, binding, output } } => {
            let report = package_check::check(&board, &package, &binding)?;
            let value = serde_json::to_value(&report).unwrap();
            if let Some(o) = output {
                write_json(&o, &value)?;
            }
            Ok((value, report.ok))
        }
        Cmd::Gen { kind: Gen::Buck { board, intent: intent_path, block, roles, params, defaults, output } } => {
            if defaults {
                return Ok((serde_json::to_value(blockgen::buck::Params::default()).unwrap(), true));
            }
            let b = pcb::Board::load(&board)?;
            let (_, role_map, _) = block_roles(intent_path.as_deref(), block, roles)?;
            let roles = verify::buck::BuckRoles::from_map(&role_map)?;
            let params: blockgen::buck::Params = match params {
                Some(p) => serde_json::from_str(&std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?).map_err(|e| format!("{}: {e}", p.display()))?,
                None => Default::default(),
            };
            let started = std::time::Instant::now();
            let g = blockgen::buck::generate(&b, &roles, &params)?;
            let mut value = serde_json::to_value(&g).unwrap();
            value["params"] = serde_json::to_value(&params).unwrap();
            value["seconds"] = json!(started.elapsed().as_secs_f64());
            if let Some(o) = output {
                write_json(&o, &value)?;
            }
            Ok((value, true))
        }
        Cmd::Compile { intent: path, pcb: board, output, dru, dru_base } => {
            let path = ctx.or_board(path, "an intent file", |b| b.intent.clone())?;
            let board = ctx.or_board(board, "--pcb", |b| b.pcb.clone())?;
            let file = intent::load(&path)?;
            let b = pcb::Board::load(&board)?;
            let (routing, notes) = compile::compile(&file, &b)?;
            write_json(&output, &serde_json::to_value(&routing).unwrap())?;
            if let Some(d) = &dru {
                let base = match &dru_base {
                    Some(p) => Some(std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?),
                    None => None,
                };
                std::fs::write(d, compile::merge_dru(base.as_deref(), &routing)).map_err(|e| format!("{}: {e}", d.display()))?;
            }
            Ok((
                json!({
                    "ok": true,
                    "wrote": output,
                    "dru": dru,
                    "regions": routing.regions.len(),
                    "rules": routing.regions.iter().map(|r| r.rules.len()).sum::<usize>(),
                    "max_lengths": routing.max_lengths.len(),
                    "notes": notes,
                }),
                true,
            ))
        }
        Cmd::Compose { composition, packages, output, bindings } => {
            let composition = ctx.or_board(composition, "a composition", |b| b.composition.clone())?;
            let packages = match packages {
                Some(p) => p,
                None if ctx.active() => ctx.board()?.packages.clone(),
                None => PathBuf::from("packages"),
            };
            let text = std::fs::read_to_string(&composition).map_err(|e| format!("{}: {e}", composition.display()))?;
            let c: compose::Composition = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", composition.display()))?;
            let r = compose::compose(&c, &packages)?;
            let value = serde_json::to_value(&r).unwrap();
            if let Some(o) = output {
                write_json(&o, &value)?;
            }
            if let Some(dir) = bindings.filter(|_| r.ok) {
                std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
                for (name, b) in &r.bindings {
                    write_json(&dir.join(format!("{name}.binding.json")), &serde_json::to_value(b).unwrap())?;
                }
            }
            Ok((value, r.ok))
        }
        Cmd::Floorplan { pcb: board, composed, spec, root, seed, moves, evaluate, problem, nets_from_board, outline_from_board, input_problem, output } => {
            if let Some(p) = input_problem {
                let mut pr: floorplan::Problem = serde_json::from_value(read_json_value(&p)?).map_err(|e| format!("{}: {e}", p.display()))?;
                if let Some(s) = seed {
                    pr.options.seed = s;
                }
                if let Some(m) = moves {
                    pr.options.moves = m;
                }
                return Ok(solve_and_report(&pr, output.as_deref())?);
            }
            let board = ctx.or_board(board, "a board", |b| b.pcb.clone())?;
            let spec = match spec {
                Some(p) => Some(p),
                None if ctx.active() => Some(ctx.board()?.floorplan.clone()).filter(|p| p.exists()),
                None => None,
            };
            let root = match root {
                Some(r) => r,
                None if ctx.active() => ctx.board()?.spec_root(),
                None => PathBuf::from("."),
            };
            let comp: Value = if nets_from_board {
                composed_from_board(&pcb::Board::load(&board)?)
            } else {
                read_json_value(&ctx.or_board(composed, "--composed", |b| b.composed.clone())?)?
            };
            let mut spec: floorplan_cmd::Spec = match spec {
                Some(p) => serde_json::from_value(read_json_value(&p)?).map_err(|e| format!("{}: {e}", p.display()))?,
                None => serde_json::from_value(json!({})).unwrap(),
            };
            if ctx.active() {
                let shape_path = ctx.board()?.shape.clone();
                if shape_path.exists() {
                    shape_into_spec(&shape_path, &mut spec)?;
                }
            }
            if outline_from_board && spec.outline.is_none() && spec.outline_polygon.is_none() {
                let b = pcb::Board::load(&board)?;
                spec.outline_polygon = Some(match b.outline {
                    Some(o) => o.into_iter().map(|p| (p[0], p[1])).collect(),
                    None => {
                        // No Edge.Cuts (benchmark boards): the box the parts' courtyards (or pads)
                        // fill where they are now, the space the board's own placement used.
                        let mut bb = [f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY];
                        for part in b.parts.values() {
                            let pts: Vec<pcb::geom::P> = match &part.courtyard {
                                Some(c) if !c.is_empty() => c.clone(),
                                _ => part.pads.iter().flat_map(|&k| b.pads[k].shape.iter().flatten().copied()).collect(),
                            };
                            for q in pts {
                                bb = [bb[0].min(q[0]), bb[1].min(q[1]), bb[2].max(q[0]), bb[3].max(q[1])];
                            }
                        }
                        if !bb[0].is_finite() {
                            return Err(format!("{}: no outline and no parts", board.display()));
                        }
                        vec![(bb[0], bb[1]), (bb[2], bb[1]), (bb[2], bb[3]), (bb[0], bb[3])]
                    }
                });
            }
            let mut pr = floorplan_cmd::problem(&board, &comp, &spec, &root)?;
            if let Some(s) = seed {
                pr.options.seed = s;
            }
            if let Some(m) = moves {
                pr.options.moves = m;
            }
            if problem {
                let value = serde_json::to_value(&pr).unwrap();
                if let Some(o) = output {
                    write_json(&o, &value)?;
                }
                return Ok((value, true));
            }
            if let Some(placed) = evaluate {
                let b = pcb::Board::load(&placed)?;
                let poses = b.parts.iter().map(|(r, p)| (r.clone(), floorplan::Pose { x: p.at[0], y: p.at[1], rotation: p.rotation })).collect();
                for part in pr.parts.iter_mut() {
                    part.back = b.parts.get(&part.reference).is_some_and(|p| p.back);
                }
                let m = floorplan::evaluate(&pr, &poses)?;
                return Ok((json!({ "evaluated": placed, "metrics": m, "nets_scored": pr.nets.len() }), true));
            }
            solve_and_report(&pr, output.as_deref())
        }
        Cmd::Shape { shape: path, generators, import, output } => {
            let value = if generators {
                serde_json::to_value(shape::generators::GENERATORS).unwrap()
            } else if let Some(drawing) = import {
                let text = std::fs::read_to_string(&drawing).map_err(|e| format!("{}: {e}", drawing.display()))?;
                let ext = drawing.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
                let features = match ext.as_str() {
                    "svg" => shape::import::svg(&text),
                    "dxf" => shape::import::dxf(&text),
                    _ => Err("give an .svg or a .dxf".into()),
                }
                .map_err(|e| format!("{}: {e}", drawing.display()))?;
                let doc = shape::ShapeDoc { note: Some(format!("Imported from {}", drawing.display())), features, ..Default::default() };
                serde_json::to_value(&doc).unwrap()
            } else {
                let path = ctx.or_board(path, "a shape file", |b| b.shape.clone())?;
                let doc: shape::ShapeDoc = serde_json::from_value(read_json_value(&path)?).map_err(|e| format!("{}: {e}", path.display()))?;
                let s = shape::evaluate(&doc);
                let mut v = serde_json::to_value(&s).unwrap();
                v["ok"] = (s.warnings.is_empty() && s.outline.len() == 1).into();
                v
            };
            if let Some(o) = output {
                write_json(&o, &value)?;
            }
            let ok = value.get("ok").and_then(Value::as_bool).unwrap_or(true);
            Ok((value, ok))
        }
        Cmd::Library { op: LibraryOp::Check { root } } => {
            let report = library::Library::load(&root)?.check();
            Ok((serde_json::to_value(&report).unwrap(), report.ok))
        }
        Cmd::Library { op: LibraryOp::Query { root, set, id, scope, attribute, status, text } } => {
            let lib = library::Library::load(&root)?;
            let hits = lib.query(&library::Query { set, status, attribute, scope, id, text });
            Ok((json!({ "count": hits.len(), "claims": hits }), true))
        }
        Cmd::Import { from: Import::EasyPcb { nets_csv, netlist, output } } => {
            let board = ctx.board.clone().ok_or("give the board's name with --board")?;
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

/// Solve a floorplan problem and say whether the result is legal; with the time it took.
fn solve_and_report(pr: &floorplan::Problem, output: Option<&Path>) -> Result<(Value, bool), String> {
    let started = std::time::Instant::now();
    let placement = floorplan::solve(pr);
    let ok = placement.metrics.overlaps == 0 && placement.metrics.edge_misses.is_empty() && placement.metrics.separation_misses.is_empty();
    let mut value = serde_json::to_value(&placement).unwrap();
    value["ok"] = ok.into();
    value["nets_scored"] = pr.nets.len().into();
    value["seconds"] = started.elapsed().as_secs_f64().into();
    if let Some(o) = output {
        write_json(o, &value)?;
    }
    Ok((value, ok))
}

/// A composition made from a board itself: every part, and every net its pads are on (not the
/// unconnected ones KiCad names per pad).
fn composed_from_board(b: &pcb::Board) -> Value {
    let parts: serde_json::Map<String, Value> = b.parts.keys().map(|r| (r.clone(), json!({}))).collect();
    let mut nets: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for pad in &b.pads {
        if pad.net.is_empty() || pad.net.starts_with("unconnected-") || pad.part.is_empty() {
            continue;
        }
        let e = nets.entry(pad.net.clone()).or_default();
        let id = format!("{}.{}", pad.part, pad.pin);
        if !e.contains(&id) {
            e.push(id);
        }
    }
    json!({ "parts": parts, "nets": nets })
}

/// A board's shape into its floorplan spec: the outline, unless the spec sets one, and every
/// keep-out, hole and cut-out as a keep-out rectangle (their bounding boxes, to be safe).
fn shape_into_spec(path: &Path, spec: &mut floorplan_cmd::Spec) -> Result<(), String> {
    let doc: shape::ShapeDoc = serde_json::from_value(read_json_value(path)?).map_err(|e| format!("{}: {e}", path.display()))?;
    let s = shape::evaluate(&doc);
    if s.outline.len() != 1 {
        return Err(format!("{}: the outline is in {} pieces; the floorplanner needs one", path.display(), s.outline.len()));
    }
    if spec.outline.is_none() && spec.outline_polygon.is_none() {
        spec.outline_polygon = Some(s.outline[0].iter().map(|p| (p[0], p[1])).collect());
    }
    let bbox = |pts: &mut dyn Iterator<Item = &shape::P>| {
        pts.fold([f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY], |b, p| [b[0].min(p[0]), b[1].min(p[1]), b[2].max(p[0]), b[3].max(p[1])])
    };
    let side = |s: shape::Side| match s {
        shape::Side::Both => "both",
        shape::Side::Front => "front",
        shape::Side::Back => "back",
    };
    for k in &s.keepouts {
        spec.keepouts.push(floorplan_cmd::Keepout { name: k.name.clone(), rect: bbox(&mut k.polygon.iter().flatten()), side: side(k.side).into() });
    }
    for (i, c) in s.cutouts.iter().enumerate() {
        spec.keepouts.push(floorplan_cmd::Keepout { name: format!("cut-out {}", i + 1), rect: bbox(&mut c.iter()), side: "both".into() });
    }
    for (i, h) in s.holes.iter().enumerate() {
        let r = h.d / 2.0 + h.clearance;
        spec.keepouts.push(floorplan_cmd::Keepout { name: format!("hole {}", i + 1), rect: [h.x - r, h.y - r, h.x + r, h.y + r], side: "both".into() });
    }
    Ok(())
}

/// A block's name, its roles, and the limits on its checks.
type BlockRoles = (String, BTreeMap<String, String>, BTreeMap<String, verify::Limit>);

#[derive(serde::Deserialize)]
struct MeasureSpec {
    #[serde(default)]
    paths: Vec<PathSpec>,
    return_paths: Option<ReturnSpec>,
    decoupling: Option<DecouplingSpec>,
}

#[derive(serde::Deserialize)]
struct PathSpec {
    name: String,
    from: String,
    to: String,
}

#[derive(serde::Deserialize)]
struct ReturnSpec {
    ground: String,
    nets: Vec<String>,
    step: Option<f64>,
}

#[derive(serde::Deserialize)]
struct DecouplingSpec {
    part: String,
    nets: Vec<String>,
    ground: String,
}

/// A block's name, roles and limits: from an intent file's block, or from `k=v,...`.
fn block_roles(intent_path: Option<&Path>, block: Option<String>, roles: Option<String>) -> Result<BlockRoles, String> {
    let mut limits = BTreeMap::new();
    match (intent_path, roles) {
        (Some(p), _) => {
            let file = intent::load(p)?;
            let id = block.ok_or("--intent needs --block")?;
            let blk = file.blocks.get(&id).ok_or_else(|| format!("no block `{id}` in {}", p.display()))?;
            for c in blk.placement_goal.iter().flat_map(|g| &g.constraints) {
                let check = serde_json::to_value(c.value.metric).unwrap().as_str().unwrap_or_default().to_string();
                let op = match c.value.op {
                    intent::BoundOp::Le => "<=",
                    intent::BoundOp::Ge => ">=",
                };
                limits.insert(check, verify::Limit { op, value: c.value.limit });
            }
            Ok((id, blk.roles.clone(), limits))
        }
        (None, Some(r)) => {
            let map = r
                .split(',')
                .map(|kv| kv.split_once('=').map(|(k, v)| (k.trim().to_string(), v.trim().to_string())).ok_or(format!("role `{kv}` is not key=value")))
                .collect::<Result<BTreeMap<_, _>, _>>()?;
            Ok((block.unwrap_or_else(|| "buck".into()), map, limits))
        }
        (None, None) => Err("give --intent and --block, or --roles".into()),
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

fn read_json_value(p: &Path) -> Result<Value, String> {
    let text = std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", p.display()))
}

#[cfg(test)]
mod tests {
    /// Every subcommand's arguments are consistent with the global `--project` / `--board`.
    #[test]
    fn cli_is_consistent() {
        use clap::CommandFactory;
        super::Cli::command().debug_assert();
    }
}
