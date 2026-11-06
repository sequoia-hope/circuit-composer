//! Synced state, persistent ID minting, pulling, locking, taking over and publishing through
//! `board-interface`.

use std::collections::BTreeSet;

use board_interface::{
    Author, Comparison, Driver, Handover, Hole as BiHole, Keepout as BiKeepout, Loop, Manifest,
    Record, SCHEMA_VERSION, compare, hash, within, write,
};
use serde::{Deserialize, Serialize};

use crate::curves::{evaluate_curves, nm_to_mm, side_from_bi, slugify};
use crate::{Feature, Geom, Hole, Keepout, Op, ShapeDoc, Symmetry, SymmetryKind};

/// Synced interface state stored in `shape.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncState {
    pub driver: Driver,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub following: Option<String>,
    /// Revision last observed at `following` when pulled (or written directly to `following`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub followed_revision: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_handover: Option<Handover>,
    /// Set by `take_over` to the revision taken over from; written as `last_handover` on the next
    /// publish.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_handover_from: Option<String>,
    /// Last synced or published `Record`, used as `base` for `status` and `within(..., 1)`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_record: Option<Record>,
    /// Read-only synced geometry while locked. Never enters Clipper2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub synced: Option<SyncedGeometry>,
    /// Composer's own recipe set aside when pulling an incoming outline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recipe_aside: Option<RecipeAside>,
}

/// Pulled geometry kept verbatim in whole nanometres so it never passes through Clipper2.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncedGeometry {
    pub outline: Loop,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cutouts: Vec<Loop>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub holes: Vec<BiHole>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keepouts: Vec<BiKeepout>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub approximate: bool,
}

impl SyncedGeometry {
    pub fn from_record(r: &Record) -> Self {
        Self {
            outline: r.outline.clone(),
            cutouts: r.cutouts.clone(),
            holes: r.holes.clone(),
            keepouts: r.keepouts.clone(),
            approximate: r.manifest.approximate,
        }
    }
}

/// Composer's previous feature recipe, preserved aside when pulling a synced shape.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RecipeAside {
    #[serde(default)]
    pub features: Vec<Feature>,
    #[serde(default)]
    pub corner_radius: f64,
    #[serde(default)]
    pub symmetry: Symmetry,
}

#[derive(Debug, Clone, Serialize)]
pub struct PullOutcome {
    pub revision: String,
    pub driver: Driver,
    pub geometry_hash: String,
    pub approximate: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TakeOverOutcome {
    pub previous_driver: Driver,
    pub from_revision: Option<String>,
    pub kept_ids: usize,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum PublishOutcome {
    Unchanged {
        revision: String,
        geometry_hash: String,
        warnings: Vec<String>,
    },
    Write {
        dxf: String,
        manifest_json: String,
        record: Box<Record>,
        warnings: Vec<String>,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct StatusOutcome {
    pub base_revision: String,
    pub theirs_revision: String,
    pub driver: Driver,
    pub comparison: Comparison,
}

pub fn driver_label(driver: Driver) -> &'static str {
    match driver {
        Driver::Waffle => "Waffle Iron",
        Driver::Composer => "Circuit Composer",
        Driver::Kicad => "KiCad",
    }
}

/// Whether `doc` is currently locked against direct edits in Composer.
pub fn is_locked(doc: &ShapeDoc) -> bool {
    doc.sync
        .as_ref()
        .is_some_and(|s| s.driver != Driver::Composer || s.synced.is_some())
}

/// Mint stable `composer:<...>` IDs once for any holes and keep-outs that lack one.
/// Never re-mints an existing or retired ID across edits and process restarts.
/// Returns `true` if any ID was newly minted.
pub fn mint_ids(doc: &mut ShapeDoc) -> bool {
    let mut used = collect_existing_ids(doc);
    let mut changed = false;

    for id in &used {
        if let Some(n) = id
            .strip_prefix("composer:hole/")
            .and_then(|s| s.parse::<u64>().ok())
        {
            doc.next_hole_id = doc.next_hole_id.max(n);
        }
        if let Some(n) = id
            .strip_prefix("composer:keepout/")
            .and_then(|s| s.parse::<u64>().ok())
        {
            doc.next_keepout_id = doc.next_keepout_id.max(n);
        }
    }
    for h in &doc.holes {
        if let Some(id) = h.id.as_ref().filter(|s| !s.trim().is_empty()) {
            doc.minted_ids.insert(id.clone());
        }
    }
    for ko in &doc.keepouts {
        if let Some(id) = ko.id.as_ref().filter(|s| !s.trim().is_empty()) {
            doc.minted_ids.insert(id.clone());
        }
    }

    for h in &mut doc.holes {
        if h.id.as_ref().is_none_or(|s| s.trim().is_empty()) {
            loop {
                doc.next_hole_id += 1;
                let cand = format!("composer:hole/{}", doc.next_hole_id);
                if used.insert(cand.clone()) {
                    doc.minted_ids.insert(cand.clone());
                    h.id = Some(cand);
                    changed = true;
                    break;
                }
            }
        }
    }

    for ko in &mut doc.keepouts {
        if ko.id.as_ref().is_none_or(|s| s.trim().is_empty()) {
            let slug = slugify(&ko.name);
            let preferred = format!("composer:keepout/{slug}");
            let chosen = if !slug.is_empty() && slug != "feature" && used.insert(preferred.clone())
            {
                preferred
            } else {
                loop {
                    doc.next_keepout_id += 1;
                    let cand = format!("composer:keepout/{}", doc.next_keepout_id);
                    if used.insert(cand.clone()) {
                        break cand;
                    }
                }
            };
            doc.minted_ids.insert(chosen.clone());
            ko.id = Some(chosen);
            changed = true;
        }
    }

    changed
}

fn collect_existing_ids(doc: &ShapeDoc) -> BTreeSet<String> {
    let mut out = doc.minted_ids.clone();
    for h in &doc.holes {
        if let Some(id) = &h.id {
            out.insert(id.clone());
        }
    }
    for ko in &doc.keepouts {
        if let Some(id) = &ko.id {
            out.insert(id.clone());
        }
        if let Geom::Path { elements, .. } = &ko.geom {
            for el in elements {
                if let Some(id) = &el.id {
                    out.insert(id.clone());
                }
            }
        }
    }
    for f in &doc.features {
        if let Geom::Path { elements, .. } = &f.geom {
            for el in elements {
                if let Some(id) = &el.id {
                    out.insert(id.clone());
                }
            }
        }
    }
    if let Some(s) = &doc.sync {
        if let Some(synced) = &s.synced {
            collect_loop_and_item_ids(
                &synced.outline,
                &synced.cutouts,
                &synced.holes,
                &synced.keepouts,
                &mut out,
            );
        }
        if let Some(br) = &s.base_record {
            collect_loop_and_item_ids(
                &br.outline,
                &br.cutouts,
                &br.holes,
                &br.keepouts,
                &mut out,
            );
        }
        if let Some(aside) = &s.recipe_aside {
            for f in &aside.features {
                if let Geom::Path { elements, .. } = &f.geom {
                    for el in elements {
                        if let Some(id) = &el.id {
                            out.insert(id.clone());
                        }
                    }
                }
            }
        }
    }
    out
}

fn collect_loop_and_item_ids(
    outline: &Loop,
    cutouts: &[Loop],
    holes: &[BiHole],
    keepouts: &[BiKeepout],
    out: &mut BTreeSet<String>,
) {
    for el in &outline.elements {
        if let Some(id) = &el.id {
            out.insert(id.clone());
        }
    }
    for lp in cutouts {
        for el in &lp.elements {
            if let Some(id) = &el.id {
                out.insert(id.clone());
            }
        }
    }
    for h in holes {
        if let Some(id) = &h.id {
            out.insert(id.clone());
        }
    }
    for ko in keepouts {
        for el in &ko.boundary.elements {
            if let Some(id) = &el.id {
                out.insert(id.clone());
            }
        }
    }
}

fn describe_recipe(aside: &RecipeAside) -> String {
    let mut parts: Vec<String> = Vec::new();
    for f in &aside.features {
        match &f.geom {
            Geom::Generator { name, .. } => parts.push(format!("generator `{name}` ({})", f.name)),
            _ => parts.push(f.name.clone()),
        }
    }
    if aside.corner_radius > 0.0 {
        parts.push(format!("corner_radius {} mm", aside.corner_radius));
    }
    if aside.symmetry.kind != SymmetryKind::None {
        parts.push(format!("symmetry {:?}", aside.symmetry.kind));
    }
    if parts.is_empty() {
        "empty recipe".into()
    } else {
        parts.join(", ")
    }
}

pub(crate) fn bi_holes_to_doc(holes: &[BiHole]) -> Vec<Hole> {
    holes
        .iter()
        .map(|h| Hole {
            id: h.id.clone(),
            x: nm_to_mm(h.center.x),
            y: nm_to_mm(h.center.y),
            d: nm_to_mm(h.diameter_nm),
            plated: h.plated,
            clearance: nm_to_mm(h.clearance_nm),
        })
        .collect()
}

pub(crate) fn bi_keepouts_to_doc(keepouts: &[BiKeepout]) -> Vec<Keepout> {
    keepouts
        .iter()
        .enumerate()
        .map(|(i, ko)| {
            let name = ko
                .boundary
                .elements
                .first()
                .and_then(|e| e.id.as_deref())
                .map(|s| s.rsplit(':').next().unwrap_or(s).to_string())
                .unwrap_or_else(|| format!("keepout {}", i + 1));
            let single_id = if ko.boundary.elements.len() == 1 {
                ko.boundary.elements[0].id.clone()
            } else {
                None
            };
            Keepout {
                id: single_id,
                name,
                geom: Geom::Path {
                    elements: ko.boundary.elements.clone(),
                    approximate: false,
                },
                side: side_from_bi(ko.side),
                max_height: ko.max_height_nm.map(nm_to_mm),
            }
        })
        .collect()
}

/// Pull an incoming `Record` into `doc` as read-only synced geometry, setting any active Composer
/// recipe aside with a warning.
pub fn pull(doc: &mut ShapeDoc, record: &Record, following: Option<String>) -> PullOutcome {
    let mut warnings = Vec::new();
    let prev_sync = doc.sync.take();
    let had_synced = prev_sync.as_ref().and_then(|s| s.synced.as_ref()).is_some();

    let has_active_recipe = !had_synced
        && (!doc.features.is_empty()
            || doc.corner_radius > 0.0
            || doc.symmetry.kind != SymmetryKind::None);

    let recipe_aside = if has_active_recipe {
        let aside = RecipeAside {
            features: std::mem::take(&mut doc.features),
            corner_radius: doc.corner_radius,
            symmetry: doc.symmetry,
        };
        doc.corner_radius = 0.0;
        doc.symmetry = Symmetry::default();
        warnings.push(format!(
            "Composer recipe ({}) set aside while following {} outline",
            describe_recipe(&aside),
            driver_label(record.manifest.driver)
        ));
        Some(aside)
    } else {
        doc.features.clear();
        doc.corner_radius = 0.0;
        doc.symmetry = Symmetry::default();
        prev_sync.as_ref().and_then(|s| s.recipe_aside.clone())
    };

    doc.thickness = record.manifest.thickness_mm;
    doc.holes = bi_holes_to_doc(&record.holes);
    doc.keepouts = bi_keepouts_to_doc(&record.keepouts);

    let prev_following = prev_sync.and_then(|s| s.following);
    let rev = record.manifest.revision.clone();
    let geom_hash = record.manifest.geometry_hash.clone();

    // Record incoming hole and keep-out IDs so Composer never mints a colliding ID later.
    for h in &record.holes {
        if let Some(id) = &h.id {
            doc.minted_ids.insert(id.clone());
        }
    }
    for ko in &record.keepouts {
        for el in &ko.boundary.elements {
            if let Some(id) = &el.id {
                doc.minted_ids.insert(id.clone());
            }
        }
    }

    doc.sync = Some(SyncState {
        driver: record.manifest.driver,
        revision: Some(rev.clone()),
        base: Some(rev.clone()),
        following: following.or(prev_following),
        followed_revision: Some(rev.clone()),
        last_handover: record.manifest.last_handover.clone(),
        pending_handover_from: None,
        base_record: Some(record.clone()),
        synced: Some(SyncedGeometry::from_record(record)),
        recipe_aside,
    });

    PullOutcome {
        revision: rev,
        driver: record.manifest.driver,
        geometry_hash: geom_hash,
        approximate: record.manifest.approximate,
        warnings,
    }
}

/// Explicit unlock ("take over"): warns what is lost, keeps all synced IDs, starts a new Composer
/// recipe from the synced shape, and schedules `last_handover` with `from_revision` for the next
/// publish.
pub fn take_over(doc: &mut ShapeDoc) -> Result<TakeOverOutcome, String> {
    if !is_locked(doc) {
        return Err("outline is already controlled by Composer".into());
    }
    let sync = doc.sync.as_mut().expect("checked by is_locked");
    let previous_driver = sync.driver;
    let mut warnings = Vec::new();

    if let Some(aside) = sync.recipe_aside.take() {
        if !aside.features.is_empty()
            || aside.corner_radius > 0.0
            || aside.symmetry.kind != SymmetryKind::None
        {
            warnings.push(format!(
                "discarded set-aside Composer recipe ({})",
                describe_recipe(&aside)
            ));
        }
    }

    if previous_driver != Driver::Composer {
        warnings.push(format!(
            "taking over outline from {}: upstream CAD constraints are not carried across",
            driver_label(previous_driver)
        ));
    }

    let mut kept_ids = 0usize;
    if let Some(synced) = sync.synced.take() {
        kept_ids += synced
            .outline
            .elements
            .iter()
            .filter(|e| e.id.is_some())
            .count();
        for c in &synced.cutouts {
            kept_ids += c.elements.iter().filter(|e| e.id.is_some()).count();
        }
        kept_ids += synced.holes.iter().filter(|h| h.id.is_some()).count();
        for ko in &synced.keepouts {
            kept_ids += ko
                .boundary
                .elements
                .iter()
                .filter(|e| e.id.is_some())
                .count();
        }

        let mut features = Vec::with_capacity(1 + synced.cutouts.len());
        features.push(Feature {
            name: "outline".into(),
            op: Op::Add,
            geom: Geom::Path {
                elements: synced.outline.elements,
                approximate: synced.approximate,
            },
            hidden: false,
        });
        for (idx, cutout) in synced.cutouts.into_iter().enumerate() {
            features.push(Feature {
                name: format!("cutout {}", idx + 1),
                op: Op::Subtract,
                geom: Geom::Path {
                    elements: cutout.elements,
                    approximate: false,
                },
                hidden: false,
            });
        }
        doc.features = features;
        doc.corner_radius = 0.0;
        doc.symmetry = Symmetry::default();
        doc.holes = bi_holes_to_doc(&synced.holes);
        doc.keepouts = bi_keepouts_to_doc(&synced.keepouts);
    }

    let from_revision = if previous_driver != Driver::Composer {
        let rev = sync
            .base
            .clone()
            .or_else(|| sync.revision.clone())
            .ok_or_else(|| "cannot take over without a synced base revision".to_string())?;
        sync.pending_handover_from = Some(rev.clone());
        Some(rev)
    } else {
        sync.pending_handover_from = None;
        None
    };

    sync.driver = Driver::Composer;

    Ok(TakeOverOutcome {
        previous_driver,
        from_revision,
        kept_ids,
        warnings,
    })
}

/// Observed status of `sync.following` when preparing to publish.
#[derive(Debug, Clone)]
pub enum FollowedState<'a> {
    /// `sync.following` is unset.
    None,
    /// `sync.following` was read from disk.
    Readable(&'a Record),
    /// `sync.following` was set to `path`, but reading it failed with `error`.
    Unreadable { path: String, error: String },
}

fn record_descends_from(rec: Option<&Record>, ancestor_rev: &str) -> bool {
    let Some(r) = rec else {
        return false;
    };
    r.manifest.parent.as_deref() == Some(ancestor_rev)
        || r.manifest
            .last_handover
            .as_ref()
            .is_some_and(|h| h.from_revision == ancestor_rev)
}

#[cfg(not(target_arch = "wasm32"))]
fn read_followed_from_fs(following: &str) -> Result<Record, String> {
    use std::path::Path;
    let p = Path::new(following);
    let (dxf_path, json_path) = if p.is_dir() {
        (p.join("board-shape.dxf"), p.join("board-shape.json"))
    } else {
        let ext = p
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if ext == "json" || ext == "dxf" {
            (p.with_extension("dxf"), p.with_extension("json"))
        } else {
            let stem_json = p.with_extension("json");
            let stem_dxf = p.with_extension("dxf");
            if stem_json.is_file() || stem_dxf.is_file() {
                (stem_dxf, stem_json)
            } else {
                (p.join("board-shape.dxf"), p.join("board-shape.json"))
            }
        }
    };
    let dxf = std::fs::read(&dxf_path).map_err(|e| format!("{}: {e}", dxf_path.display()))?;
    let manifest =
        std::fs::read(&json_path).map_err(|e| format!("{}: {e}", json_path.display()))?;
    board_interface::read(&dxf, &manifest).map_err(|e| format!("{}: {e}", json_path.display()))
}

/// Evaluate `doc`, enforce driver / base / followed / single-outline rules, check `within(..., 1)`,
/// verify readback via `board_interface::read`, and prepare `(dxf, manifest_json, written_record)`.
pub fn prepare_publish(
    doc: &mut ShapeDoc,
    board: &str,
    author_source: &str,
    at: &str,
    on_disk: Option<&Record>,
) -> Result<PublishOutcome, String> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let owned_fol;
        let (fol_state, same_target) =
            if let Some(fol_str) = doc.sync.as_ref().and_then(|s| s.following.clone()) {
                match read_followed_from_fs(&fol_str) {
                    Ok(rec) => {
                        let same = on_disk.is_some_and(|d| {
                            d.manifest.revision == rec.manifest.revision
                                && d.manifest.driver == rec.manifest.driver
                        });
                        owned_fol = rec;
                        (FollowedState::Readable(&owned_fol), same)
                    }
                    Err(error) => (FollowedState::Unreadable { path: fol_str, error }, false),
                }
            } else {
                (FollowedState::None, false)
            };
        prepare_publish_with_followed(doc, board, author_source, at, on_disk, fol_state, same_target)
    }
    #[cfg(target_arch = "wasm32")]
    {
        let fol_state = if let Some(fol_str) = doc.sync.as_ref().and_then(|s| s.following.clone()) {
            FollowedState::Unreadable {
                path: fol_str,
                error: "filesystem unavailable in browser".into(),
            }
        } else {
            FollowedState::None
        };
        prepare_publish_with_followed(doc, board, author_source, at, on_disk, fol_state, false)
    }
}

/// Like [`prepare_publish`], with an explicit [`FollowedState`] resolved by the caller.
pub fn prepare_publish_with_followed(
    doc: &mut ShapeDoc,
    board: &str,
    author_source: &str,
    at: &str,
    on_disk: Option<&Record>,
    followed: FollowedState<'_>,
    publishing_to_followed: bool,
) -> Result<PublishOutcome, String> {
    if let Some(s) = &doc.sync {
        if s.driver != Driver::Composer {
            return Err(format!(
                "refusing to publish: outline is controlled by {} (take over first)",
                driver_label(s.driver)
            ));
        }
    }

    let mut extra_warnings: Vec<String> = Vec::new();
    match followed {
        FollowedState::None => {}
        FollowedState::Readable(fol_rec) => {
            let fol_rev = fol_rec.manifest.revision.as_str();
            let expected_fol = doc
                .sync
                .as_ref()
                .and_then(|s| s.followed_revision.as_deref().or(s.base.as_deref()));
            let base_rev = doc.sync.as_ref().and_then(|s| s.base.as_deref());
            let base_rec = doc.sync.as_ref().and_then(|s| s.base_record.as_ref());
            let fol_matches = expected_fol == Some(fol_rev)
                || base_rev == Some(fol_rev)
                || record_descends_from(base_rec, fol_rev);
            if !fol_matches {
                return Err(format!(
                    "refusing to publish: base moved (followed interface is at `{fol_rev}`, expected {}; run `ccomp shape pull` to sync with the driver's latest revision before publishing)",
                    expected_fol
                        .map(|b| format!("`{b}`"))
                        .unwrap_or_else(|| "no prior base".to_string())
                ));
            }
        }
        FollowedState::Unreadable { path, error } => {
            extra_warnings.push(format!(
                "followed interface `{path}` could not be read ({error}); could not verify whether upstream moved"
            ));
        }
    }

    if let Some(disk) = on_disk {
        let disk_rev = disk.manifest.revision.as_str();
        let expected_base = doc.sync.as_ref().and_then(|s| s.base.as_deref());
        let base_rec = doc.sync.as_ref().and_then(|s| s.base_record.as_ref());
        let matches_base = expected_base == Some(disk_rev);
        let matches_ancestor = record_descends_from(base_rec, disk_rev);
        if !matches_base && !matches_ancestor {
            return Err(match expected_base {
                Some(b) => format!(
                    "refusing to publish: base moved (target interface is at `{disk_rev}`, which is neither the synced base `{b}` nor an ancestor revision `{b}` descended from; run `ccomp shape status` or `ccomp shape pull` from the interface before publishing)"
                ),
                None => format!(
                    "refusing to publish: base moved (target interface already exists at `{disk_rev}`, while shape.json has no synced base; run `ccomp shape pull` to sync with the existing interface before publishing)"
                ),
            });
        }
    }

    if !doc.thickness.is_finite() || doc.thickness <= 0.0 {
        return Err(format!(
            "refusing to publish: invalid board thickness {} mm",
            doc.thickness
        ));
    }

    mint_ids(doc);
    let mut ce = evaluate_curves(doc);
    extra_warnings.append(&mut ce.warnings);

    if ce.outline.is_empty() {
        return Err("refusing to publish: nothing is left of the outline".into());
    }
    if ce.outline.len() > 1 {
        return Err(format!(
            "refusing to publish: the outline is in {} separate pieces; board-shape holds one outer loop",
            ce.outline.len()
        ));
    }

    // `doc.sync.base` takes precedence over `on_disk` so publishing back to Composer's own folder
    // (still holding an older revision `A`) after taking over from `W` sets `parent = Some(W)`.
    let parent = doc
        .sync
        .as_ref()
        .and_then(|s| s.base.clone())
        .or_else(|| on_disk.map(|d| d.manifest.revision.clone()));

    let handover_from = doc
        .sync
        .as_ref()
        .and_then(|s| s.pending_handover_from.clone())
        .or_else(|| {
            on_disk
                .filter(|d| d.manifest.driver != Driver::Composer)
                .map(|d| d.manifest.revision.clone())
        });

    let last_handover = if let Some(from_rev) = &handover_from {
        Some(Handover {
            by: Driver::Composer,
            at: at.to_string(),
            from_revision: from_rev.clone(),
        })
    } else {
        doc.sync
            .as_ref()
            .and_then(|s| s.last_handover.clone())
            .or_else(|| on_disk.and_then(|d| d.manifest.last_handover.clone()))
    };

    let board_name = if !board.trim().is_empty() {
        board.to_string()
    } else if let Some(d) = on_disk {
        d.manifest.board.clone()
    } else if let Some(br) = doc.sync.as_ref().and_then(|s| s.base_record.as_ref()) {
        br.manifest.board.clone()
    } else {
        "board".to_string()
    };

    let mut candidate = Record {
        outline: ce.outline.into_iter().next().unwrap(),
        cutouts: ce.cutouts,
        holes: ce.holes,
        keepouts: ce.keepouts,
        manifest: Manifest {
            schema: SCHEMA_VERSION,
            board: board_name,
            revision: String::new(),
            parent: parent.clone(),
            author: Author {
                app: Driver::Composer,
                source: if author_source.trim().is_empty() {
                    "working-tree".to_string()
                } else {
                    author_source.to_string()
                },
                at: at.to_string(),
            },
            driver: Driver::Composer,
            last_handover,
            thickness_mm: doc.thickness,
            geometry_hash: String::new(),
            approximate: ce.approximate,
        },
    };

    let geom_hash = hash(&candidate);
    candidate.manifest.geometry_hash = geom_hash.clone();

    // Publish-nothing rule: if the target already exists on disk with Composer as driver at our
    // current `base` (not a handover or catch-up write) and nothing moved by > 1 nm, write nothing.
    if let Some(disk) = on_disk {
        let disk_is_current_base = doc
            .sync
            .as_ref()
            .and_then(|s| s.base.as_deref())
            .is_none_or(|b| b == disk.manifest.revision);
        if handover_from.is_none()
            && disk.manifest.driver == Driver::Composer
            && disk_is_current_base
            && within(disk, &candidate, 1)
        {
            return Ok(PublishOutcome::Unchanged {
                revision: disk.manifest.revision.clone(),
                geometry_hash: disk.manifest.geometry_hash.clone(),
                warnings: extra_warnings,
            });
        }
    }

    // The revision derives from the geometry hash, driver, parent and last handover
    // (board-interface v0.2.0), so a handover with no geometry edit still differs from its parent.
    board_interface::seal(&mut candidate);

    let (dxf, manifest_json) = write(&candidate);

    // Self-readback gate: never allow publishing bytes that `board_interface::read` rejects or
    // whose parsed geometry hash drifts.
    let readback = board_interface::read(dxf.as_bytes(), manifest_json.as_bytes()).map_err(|e| {
        format!("refusing to publish: generated interface failed readback verification: {e}")
    })?;
    let readback_hash = hash(&readback);
    if readback_hash != candidate.manifest.geometry_hash {
        return Err(format!(
            "refusing to publish: generated interface drifted on readback (wrote `{}`, read `{readback_hash}`)",
            candidate.manifest.geometry_hash
        ));
    }

    let prev_following = doc.sync.as_ref().and_then(|s| s.following.clone());
    let prev_followed_rev = doc.sync.as_ref().and_then(|s| s.followed_revision.clone());
    let next_followed_rev = if publishing_to_followed {
        Some(candidate.manifest.revision.clone())
    } else {
        prev_followed_rev
    };
    let prev_synced = doc.sync.as_ref().and_then(|s| s.synced.clone());
    let prev_aside = doc.sync.as_ref().and_then(|s| s.recipe_aside.clone());

    doc.sync = Some(SyncState {
        driver: Driver::Composer,
        revision: Some(candidate.manifest.revision.clone()),
        base: Some(candidate.manifest.revision.clone()),
        following: prev_following,
        followed_revision: next_followed_rev,
        last_handover: candidate.manifest.last_handover.clone(),
        pending_handover_from: None,
        base_record: Some(candidate.clone()),
        synced: prev_synced,
        recipe_aside: prev_aside,
    });

    Ok(PublishOutcome::Write {
        dxf,
        manifest_json,
        record: Box::new(candidate),
        warnings: extra_warnings,
    })
}

/// Compare a followed `theirs` interface record against `doc`'s recorded `base_record` (or a
/// transient evaluation of `doc` if never synced yet).
pub fn status(doc: &ShapeDoc, board: &str, theirs: &Record) -> Result<StatusOutcome, String> {
    let owned_base;
    let base: &Record = if let Some(br) = doc.sync.as_ref().and_then(|s| s.base_record.as_ref()) {
        br
    } else {
        let ce = evaluate_curves(doc);
        if ce.outline.len() != 1 {
            return Err(format!(
                "cannot compare status without a synced base when local outline has {} piece(s)",
                ce.outline.len()
            ));
        }
        let driver = doc
            .sync
            .as_ref()
            .map(|s| s.driver)
            .unwrap_or(Driver::Composer);
        let mut rec = Record {
            outline: ce.outline.into_iter().next().unwrap(),
            cutouts: ce.cutouts,
            holes: ce.holes,
            keepouts: ce.keepouts,
            manifest: Manifest {
                schema: SCHEMA_VERSION,
                board: if board.trim().is_empty() {
                    theirs.manifest.board.clone()
                } else {
                    board.to_string()
                },
                revision: String::new(),
                parent: doc.sync.as_ref().and_then(|s| s.base.clone()),
                author: Author {
                    app: driver,
                    source: "working-tree".into(),
                    at: "1970-01-01T00:00:00Z".into(),
                },
                driver,
                last_handover: doc.sync.as_ref().and_then(|s| s.last_handover.clone()),
                thickness_mm: doc.thickness,
                geometry_hash: String::new(),
                approximate: ce.approximate,
            },
        };
        let gh = hash(&rec);
        rec.manifest.geometry_hash = gh.clone();
        rec.manifest.revision = doc
            .sync
            .as_ref()
            .and_then(|s| s.base.clone().or_else(|| s.revision.clone()))
            .unwrap_or(gh);
        owned_base = rec;
        &owned_base
    };

    let comparison = compare(base, theirs);
    Ok(StatusOutcome {
        base_revision: base.manifest.revision.clone(),
        theirs_revision: theirs.manifest.revision.clone(),
        driver: theirs.manifest.driver,
        comparison,
    })
}
