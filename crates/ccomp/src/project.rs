//! Boards inside a project. A project is a directory: an easy_pcb project keeps its boards under
//! `compose/<board>/`, this repository under `boards/<board>/`. Each board directory may hold a
//! `board.json` naming where its files are (relative to the board directory); what it leaves out
//! takes the layout in `docs/app-spec.md`. Commands given `--project` / `--board` take any input
//! they were not given from here. Outputs are never defaulted: a command writes only with `-o`.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const MANIFEST: &str = "board.json";

/// `board.json`: every field optional.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    #[serde(default)]
    note: Option<String>,
    composition: Option<String>,
    composed: Option<String>,
    intent: Option<String>,
    floorplan: Option<String>,
    placement: Option<String>,
    shape: Option<String>,
    pcb: Option<String>,
    routing: Option<String>,
    dru: Option<String>,
    packages: Option<String>,
}

/// One board's files, resolved.
#[derive(Debug, Serialize)]
pub struct Board {
    pub name: String,
    pub project: PathBuf,
    pub dir: PathBuf,
    /// Whether the board directory has a `board.json`.
    pub manifest: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub composition: PathBuf,
    pub composed: PathBuf,
    pub intent: PathBuf,
    pub floorplan: PathBuf,
    pub placement: PathBuf,
    /// The board's shape (`shape.json`): outline, holes, keep-outs.
    pub shape: PathBuf,
    /// The KiCad board the floorplanner and the compiler read.
    pub pcb: PathBuf,
    pub routing: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dru: Option<PathBuf>,
    pub packages: PathBuf,
}

impl Board {
    /// Paths in a floorplan spec (rigid layouts) are relative to the packages directory's parent.
    pub fn spec_root(&self) -> PathBuf {
        self.packages.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."))
    }

    /// Which of the board's files exist, by field name.
    pub fn present(&self) -> serde_json::Value {
        let mut m = serde_json::Map::new();
        for (k, p) in [
            ("composition", &self.composition),
            ("composed", &self.composed),
            ("intent", &self.intent),
            ("floorplan", &self.floorplan),
            ("placement", &self.placement),
            ("shape", &self.shape),
            ("pcb", &self.pcb),
            ("routing", &self.routing),
            ("packages", &self.packages),
        ] {
            m.insert(k.into(), p.exists().into());
        }
        if let Some(d) = &self.dru {
            m.insert("dru".into(), d.exists().into());
        }
        m.into()
    }
}

/// Where a project keeps its boards: `compose/` if it has one, else `boards/`.
pub fn boards_root(project: &Path) -> PathBuf {
    let compose = project.join("compose");
    if compose.is_dir() {
        compose
    } else {
        project.join("boards")
    }
}

/// The boards in a project: subdirectories of its boards root holding a `board.json` or a
/// `compose.json`.
pub fn list(project: &Path) -> Result<Vec<String>, String> {
    let root = boards_root(project);
    let entries = std::fs::read_dir(&root).map_err(|e| format!("{}: {e}", root.display()))?;
    let mut names: Vec<String> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().join(MANIFEST).is_file() || e.path().join("compose.json").is_file())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    Ok(names)
}

/// Resolve `--project` and `--board`. `board` is a name under the project's boards root, or a
/// path to a board directory. With no project, the current directory is the project. With no
/// board, the project must hold exactly one.
pub fn resolve(project: Option<&Path>, board: Option<&str>) -> Result<Board, String> {
    let project = project.map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
    let dir = match board {
        Some(b) if is_board_dir(Path::new(b)) && !boards_root(&project).join(b).is_dir() => PathBuf::from(b),
        Some(b) => boards_root(&project).join(b),
        None => match list(&project)?.as_slice() {
            [one] => boards_root(&project).join(one),
            [] => return Err(format!("no boards in {}", boards_root(&project).display())),
            many => return Err(format!("{} boards in {}: pick one with --board ({})", many.len(), boards_root(&project).display(), many.join(", "))),
        },
    };
    if !dir.is_dir() {
        return Err(format!("no board directory {}", dir.display()));
    }
    let name = dir.canonicalize().ok().and_then(|d| d.file_name().map(|n| n.to_string_lossy().into_owned())).unwrap_or_else(|| dir.display().to_string());
    let path = dir.join(MANIFEST);
    let m: Manifest = if path.is_file() {
        let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?
    } else {
        Manifest::default()
    };
    let at = |field: &Option<String>, default: PathBuf| field.as_ref().map(|p| dir.join(p)).unwrap_or(default);
    let packages = match &m.packages {
        Some(p) => dir.join(p),
        None => default_packages(&project),
    };
    Ok(Board {
        composition: at(&m.composition, dir.join("compose.json")),
        composed: at(&m.composed, dir.join("out/composed.json")),
        intent: at(&m.intent, dir.join("intent.json")),
        floorplan: at(&m.floorplan, dir.join("floorplan.json")),
        placement: at(&m.placement, dir.join("placement.json")),
        shape: at(&m.shape, dir.join("shape.json")),
        pcb: at(&m.pcb, project.join("hw").join(&name).join("a").join(format!("{name}.kicad_pcb"))),
        routing: at(&m.routing, dir.join("out/routing.json")),
        dru: m.dru.as_ref().map(|p| dir.join(p)),
        packages,
        manifest: path.is_file(),
        note: m.note,
        name,
        project,
        dir,
    })
}

fn is_board_dir(p: &Path) -> bool {
    p.join(MANIFEST).is_file() || p.join("compose.json").is_file()
}

/// `$CCOMP_PACKAGES`, else the project's own `packages/`, else this repository's.
fn default_packages(project: &Path) -> PathBuf {
    if let Some(p) = std::env::var_os("CCOMP_PACKAGES") {
        return PathBuf::from(p);
    }
    let own = project.join("packages");
    if own.is_dir() {
        return own;
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packages")
}
