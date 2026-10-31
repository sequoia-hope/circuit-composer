//! The Circuit Composer app's core: one board's floorplan as the page edits it. It holds the
//! floorplan problem and every part's pose, answers what the page draws (each part's courtyard
//! and pins in board millimetres, the ratsnest, the metrics) and runs the annealer, watched, so
//! the page can show it move. Compiled to WASM with the `wasm` feature; the same code runs
//! natively under `cargo test`.
//!
//! Drawing is not done here: the page hands what this returns to a scene (a placeholder until
//! `board-draw` lands; see `docs/app-spec.md`).

use std::collections::BTreeMap;

use floorplan::{rot, Placement, Pose, Problem, Snapshot};
use serde::Serialize;

pub mod draw;

/// What a `.kicad_pcb` says about its parts: where each footprint is, in the floorplan's
/// millimetres and degrees, and the circuit the inspector shows (value, footprint, nets by pin),
/// for a board opened from a project with no `circuit.json` of its own.
#[derive(Debug, Serialize)]
pub struct KicadBoard {
    pub poses: BTreeMap<String, Pose>,
    pub parts: BTreeMap<String, KicadPart>,
    /// Net name -> `REF.pin`, the board's own nets (no `unconnected-…` single pads).
    pub nets: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Serialize)]
pub struct KicadPart {
    pub value: String,
    pub footprint: String,
}

impl KicadBoard {
    pub fn read(text: &str) -> Result<KicadBoard, String> {
        let (pcb, _) = kicad_pcb::parse_board(text).map_err(|e| e.to_string())?;
        let mut b = KicadBoard { poses: BTreeMap::new(), parts: BTreeMap::new(), nets: BTreeMap::new() };
        for fp in &pcb.footprints {
            if fp.reference.is_empty() {
                continue;
            }
            b.poses.insert(fp.reference.clone(), Pose { x: fp.at[0] * 1e3, y: fp.at[1] * 1e3, rotation: fp.rotation_deg });
            b.parts.insert(fp.reference.clone(), KicadPart { value: fp.value.clone(), footprint: fp.library_id.clone() });
            for pad in &fp.pads {
                if let Some(n) = pad.net.as_ref().filter(|n| !n.name.is_empty() && !n.name.starts_with("unconnected-")) {
                    let pin = format!("{}.{}", fp.reference, pad.number);
                    let pins = b.nets.entry(n.name.clone()).or_default();
                    if !pins.contains(&pin) {
                        pins.push(pin);
                    }
                }
            }
        }
        Ok(b)
    }
}

pub struct Session {
    pr: Problem,
    poses: Vec<Pose>,
    index: BTreeMap<String, usize>,
    /// Part index -> (group, member offset and rotation in the anchor's frame); anchors too.
    group_of: Vec<Option<(usize, (f64, f64), f64)>>,
}

/// One part where it is, for drawing and picking.
#[derive(Debug, Serialize)]
pub struct PartView {
    pub reference: String,
    pub pose: Pose,
    pub back: bool,
    pub locked: bool,
    pub block: Option<String>,
    pub group: Option<String>,
    /// The courtyard's four corners.
    pub outline: [[f64; 2]; 4],
    /// Pad centres by pin.
    pub pins: BTreeMap<String, [f64; 2]>,
}

impl Session {
    pub fn new(pr: Problem) -> Session {
        let n = pr.parts.len();
        let index = pr.parts.iter().enumerate().map(|(i, p)| (p.reference.clone(), i)).collect();
        let mut group_of = vec![None; n];
        for (g, r) in pr.groups.iter().enumerate() {
            group_of[r.anchor] = Some((g, (0.0, 0.0), 0.0));
            for (i, off, rr) in &r.members {
                group_of[*i] = Some((g, *off, *rr));
            }
        }
        let poses = pr.parts.iter().map(|p| p.fixed.unwrap_or(Pose { x: 0.0, y: 0.0, rotation: 0.0 })).collect();
        Session { pr, poses, index, group_of }
    }

    pub fn problem(&self) -> &Problem {
        &self.pr
    }

    pub fn index(&self, reference: &str) -> Option<usize> {
        self.index.get(reference).copied()
    }

    /// Take a placement's poses (`ccomp floorplan` output, or any `{parts: {REF: pose}}`);
    /// parts it does not name keep theirs.
    pub fn set_placement(&mut self, parts: &BTreeMap<String, Pose>) {
        for (r, p) in parts {
            if let Some(&i) = self.index.get(r) {
                self.poses[i] = *p;
            }
        }
    }

    pub fn placement(&self) -> BTreeMap<String, Pose> {
        self.pr.parts.iter().zip(&self.poses).map(|(p, q)| (p.reference.clone(), *q)).collect()
    }

    pub fn poses(&self) -> &[Pose] {
        &self.poses
    }

    /// The board outline the floorplanner keeps parts inside: a closed polygon, or none (the
    /// floorplanner then sizes the board itself).
    pub fn set_outline(&mut self, polygon: Option<Vec<[f64; 2]>>) {
        self.pr.outline = None;
        self.pr.outline_polygon = polygon.map(|p| p.into_iter().map(|q| (q[0], q[1])).collect());
    }

    /// The board shape's keep-outs (name, x0, y0, x1, y1, side: "front", "back" or "both"), as
    /// `ccomp floorplan` makes them: fixed parts with no pads, named `shape:NAME`, after every
    /// other part. Replaces the shape's earlier ones.
    pub fn set_keepouts(&mut self, keepouts: &[(String, [f64; 4], String)]) {
        let keep = self.pr.parts.iter().position(|p| p.reference.starts_with("shape:")).unwrap_or(self.pr.parts.len());
        self.pr.parts.truncate(keep);
        self.poses.truncate(keep);
        self.group_of.truncate(keep);
        for (name, [x0, y0, x1, y1], side) in keepouts {
            let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
            let local = [x0 - cx, y0 - cy, x1 - cx, y1 - cy];
            let pose = Pose { x: cx, y: cy, rotation: 0.0 };
            self.pr.parts.push(floorplan::PartGeom {
                reference: format!("shape:{name}"),
                bbox: local,
                pins: BTreeMap::new(),
                block: None,
                near: None,
                edge: None,
                fixed: Some(pose),
                back: side == "back",
                through: if side == "both" { vec![local] } else { vec![] },
            });
            self.poses.push(pose);
            self.group_of.push(None);
        }
        self.index = self.pr.parts.iter().enumerate().map(|(i, p)| (p.reference.clone(), i)).collect();
    }

    /// Every part's pose at once, in problem order (an anneal's snapshot).
    pub fn set_poses(&mut self, poses: &[Pose]) {
        for (i, p) in poses.iter().enumerate().take(self.poses.len()) {
            self.poses[i] = *p;
        }
    }

    /// `text`, the board's `.kicad_pcb`, with every part where it is here: moved, turned and on its
    /// side, written as pcbnew would save it (`kicad_pcb::place`). Every other byte of the board
    /// is kept: its outline, tracks and zones too. The shape's keep-outs (references with a `:`)
    /// are not parts on the board; a part the board does not have is an error.
    pub fn write_kicad_pcb(&self, text: &str) -> Result<String, String> {
        let placements: Vec<kicad_pcb::place::Placement> = self
            .pr
            .parts
            .iter()
            .zip(&self.poses)
            .filter(|(p, _)| !p.reference.contains(':'))
            .map(|(p, q)| kicad_pcb::place::Placement {
                reference: p.reference.clone(),
                at: [q.x * 1e-3, q.y * 1e-3],
                rotation_deg: q.rotation,
                side: if p.back { kicad_pcb::Side::Back } else { kicad_pcb::Side::Front },
            })
            .collect();
        kicad_pcb::place::apply(text, &placements).map_err(|e| e.to_string())
    }

    /// Where the board puts this problem's parts: each footprint's pose, as `set_placement` takes
    /// it. The inverse of `write_kicad_pcb`, so a board read and written back is unchanged. Parts
    /// the problem doesn't have are left out; parts the board doesn't have keep their poses.
    pub fn placement_of(&self, board: &KicadBoard) -> BTreeMap<String, Pose> {
        board.poses.iter().filter(|(r, _)| self.index.contains_key(*r)).map(|(r, p)| (r.clone(), *p)).collect()
    }

    pub fn locked(&self, i: usize) -> bool {
        let anchor = self.group_of[i].map(|(g, _, _)| self.pr.groups[g].anchor).unwrap_or(i);
        self.pr.parts[anchor].fixed.is_some()
    }

    /// Lock a part where it is (its group, if it is in one): the annealer leaves it there.
    pub fn set_locked(&mut self, i: usize, locked: bool) {
        let anchor = self.group_of[i].map(|(g, _, _)| self.pr.groups[g].anchor).unwrap_or(i);
        self.pr.parts[anchor].fixed = locked.then_some(self.poses[anchor]);
    }

    /// Move a part to (x, y); a group member takes its whole group with it.
    pub fn move_to(&mut self, i: usize, x: f64, y: f64) {
        let (dx, dy) = (x - self.poses[i].x, y - self.poses[i].y);
        for j in self.members(i) {
            self.poses[j].x += dx;
            self.poses[j].y += dy;
        }
        self.refix(i);
    }

    /// Turn a part a quarter turn counter-clockwise on screen (KiCad's +90); a group turns about
    /// its anchor.
    pub fn rotate(&mut self, i: usize) {
        match self.group_of[i] {
            None => self.poses[i].rotation = (self.poses[i].rotation + 90.0).rem_euclid(360.0),
            Some((g, _, _)) => {
                let a = self.pr.groups[g].anchor;
                let mut base = self.poses[a];
                base.rotation = (base.rotation + 90.0).rem_euclid(360.0);
                self.poses[a] = base;
                for &(m, off, rr) in &self.pr.groups[g].members {
                    let q = rot(off, base.rotation);
                    self.poses[m] = Pose { x: base.x + q.0, y: base.y + q.1, rotation: (base.rotation + rr).rem_euclid(360.0) };
                }
            }
        }
        self.refix(i);
    }

    fn members(&self, i: usize) -> Vec<usize> {
        match self.group_of[i] {
            None => vec![i],
            Some((g, _, _)) => {
                let r = &self.pr.groups[g];
                std::iter::once(r.anchor).chain(r.members.iter().map(|m| m.0)).collect()
            }
        }
    }

    /// A locked part that was moved by hand stays locked where it now is.
    fn refix(&mut self, i: usize) {
        if self.locked(i) {
            self.set_locked(i, true);
        }
    }

    pub fn parts(&self) -> Vec<PartView> {
        (0..self.pr.parts.len()).map(|i| self.part(i)).collect()
    }

    pub fn part(&self, i: usize) -> PartView {
        let p = &self.pr.parts[i];
        let pose = self.poses[i];
        let at = |q: (f64, f64)| {
            let q = if p.back { (q.0, -q.1) } else { q };
            let r = rot(q, pose.rotation);
            [pose.x + r.0, pose.y + r.1]
        };
        let b = p.bbox;
        PartView {
            reference: p.reference.clone(),
            pose,
            back: p.back,
            locked: self.locked(i),
            block: p.block.clone(),
            group: self.group_of[i].map(|(g, _, _)| self.pr.groups[g].name.clone()),
            outline: [at((b[0], b[1])), at((b[2], b[1])), at((b[2], b[3])), at((b[0], b[3]))],
            pins: p.pins.iter().map(|(k, q)| (k.clone(), at(*q))).collect(),
        }
    }

    /// Each net's shortest spanning tree over its pads: segments as [x0, y0, x1, y1, ...], with
    /// the net index of each segment.
    pub fn ratsnest(&self) -> (Vec<f64>, Vec<u32>) {
        let mut seg = Vec::new();
        let mut which = Vec::new();
        for (k, net) in self.pr.nets.iter().enumerate() {
            let pts: Vec<[f64; 2]> = net
                .pins
                .iter()
                .map(|(i, pin)| {
                    let p = &self.pr.parts[*i];
                    let pose = self.poses[*i];
                    let q = p.pins.get(pin).copied().unwrap_or((0.0, 0.0));
                    let q = if p.back { (q.0, -q.1) } else { q };
                    let r = rot(q, pose.rotation);
                    [pose.x + r.0, pose.y + r.1]
                })
                .collect();
            if pts.len() < 2 {
                continue;
            }
            // Prim's algorithm: nets are small.
            let mut inside = vec![false; pts.len()];
            let mut best = vec![(f64::INFINITY, 0usize); pts.len()];
            inside[0] = true;
            for j in 1..pts.len() {
                best[j] = (d2(pts[0], pts[j]), 0);
            }
            for _ in 1..pts.len() {
                let (j, _) = best.iter().enumerate().filter(|(j, _)| !inside[*j]).min_by(|a, b| a.1 .0.total_cmp(&b.1 .0)).unwrap();
                inside[j] = true;
                let from = best[j].1;
                seg.extend_from_slice(&[pts[from][0], pts[from][1], pts[j][0], pts[j][1]]);
                which.push(k as u32);
                for m in 0..pts.len() {
                    if !inside[m] {
                        let d = d2(pts[j], pts[m]);
                        if d < best[m].0 {
                            best[m] = (d, j);
                        }
                    }
                }
            }
        }
        (seg, which)
    }

    pub fn evaluate(&self) -> Result<floorplan::Metrics, String> {
        floorplan::evaluate(&self.pr, &self.placement())
    }

    /// How a placement from scratch starts: `sa` (a random spread) or `analytic` (gradient
    /// descent). Refining always starts from the current placement.
    pub fn set_placer(&mut self, placer: &str) -> Result<(), String> {
        self.pr.options.placer = serde_json::from_value(serde_json::Value::String(placer.into())).map_err(|_| format!("no placer {placer}: sa or analytic"))?;
        Ok(())
    }

    /// Anneal: from a random spread, or (`warm`) refining the current placement. `on` sees a
    /// snapshot every `every` moves and stops the anneal by returning false. The result's poses
    /// become the session's.
    pub fn anneal(&mut self, seed: u64, moves: usize, warm: bool, every: usize, on: &mut dyn FnMut(&Snapshot) -> bool) -> Placement {
        let mut pr = self.pr.clone();
        pr.options.seed = seed;
        pr.options.moves = moves;
        let start = warm.then(|| self.poses.clone());
        let placement = floorplan::solve_with(&pr, start.as_deref(), every, on);
        self.set_placement(&placement.parts);
        placement
    }
}

fn d2(a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)
}

#[cfg(feature = "wasm")]
mod wasm {
    use super::*;
    use wasm_bindgen::prelude::*;

    fn err(e: impl std::fmt::Display) -> JsValue {
        JsValue::from_str(&e.to_string())
    }

    /// The page's handle on one board.
    /// One board: the floorplan session, and the scene board-draw draws it from.
    #[wasm_bindgen]
    pub struct Board {
        s: Session,
        scene: board_draw::Scene,
        drawing: Option<board_draw::Drawing>,
        outline: Option<Vec<[f64; 2]>>,
    }

    impl Board {
        fn rebuild(&mut self) {
            self.scene = crate::draw::build(&self.s, self.outline.as_deref());
            self.drawing = None;
        }
        fn moved(&mut self) {
            crate::draw::sync_poses(&mut self.scene, &self.s);
        }
    }

    /// A `.kicad_pcb`'s parts and nets (`KicadBoard` as JSON): the circuit of a board opened from
    /// a project's store.
    #[wasm_bindgen]
    pub fn kicad_circuit(text: &str) -> Result<String, JsValue> {
        let b = KicadBoard::read(text).map_err(err)?;
        Ok(serde_json::to_string(&serde_json::json!({ "parts": b.parts, "nets": b.nets })).unwrap())
    }

    /// A `shape.json` document, evaluated (`shape::Shape` as JSON).
    #[wasm_bindgen]
    pub fn shape_evaluate(doc_json: &str) -> Result<String, JsValue> {
        let doc: shape::ShapeDoc = serde_json::from_str(doc_json).map_err(err)?;
        Ok(serde_json::to_string(&shape::evaluate(&doc)).unwrap())
    }

    /// Solve a loose composition's connections into nets and apply the interface rules
    /// (`pinsolve::Solved` as JSON): the interface types (`library/interfaces.json`), each package
    /// instance's circuit and ports as `{ instance: { circuit, ports } }`, and the library's
    /// recipes and stock as `{ recipes: { name: recipe }, stock }` (`{}` when the board has none).
    #[wasm_bindgen]
    pub fn pins_solve(loose_json: &str, types_json: &str, packages_json: &str, library_json: &str) -> Result<String, JsValue> {
        let loose: serde_json::Value = serde_json::from_str(loose_json).map_err(err)?;
        let types: interfaces::Types = serde_json::from_str(types_json).map_err(err)?;
        let mut pkgs = pinsolve::from_json(&serde_json::from_str(packages_json).map_err(err)?).map_err(err)?;
        let (recipes, stock) = library(library_json)?;
        let comp: compose::Composition = serde_json::from_value(loose.clone()).map_err(err)?;
        pinsolve::add_recipes(&mut pkgs, &comp, &recipes, &stock).map_err(err)?;
        let solved = pinsolve::solve(&loose, &types, &pkgs, 2_000_000).map_err(err)?;
        Ok(serde_json::to_string(&solved).unwrap())
    }

    /// Compose a pin-level composition (`compose::Composed` as JSON), with the packages and library
    /// as `pins_solve` takes them: what the solver's composition becomes on the board.
    #[wasm_bindgen]
    pub fn compose_board(composition_json: &str, packages_json: &str, library_json: &str) -> Result<String, JsValue> {
        let c: compose::Composition = serde_json::from_str(composition_json).map_err(err)?;
        let pkgs = pinsolve::from_json(&serde_json::from_str(packages_json).map_err(err)?).map_err(err)?;
        let (recipes, stock) = library(library_json)?;
        let mut circuits: std::collections::BTreeMap<String, compose::Circuit> = compose::expand_recipes(&c, &recipes, &stock).map_err(err)?.into_iter().map(|(k, x)| (k, x.circuit)).collect();
        for (name, inst) in c.instances.iter().filter(|(_, i)| i.recipe.is_none()) {
            let p = pkgs.get(name).ok_or_else(|| err(format!("{name}: its package {} isn't loaded", inst.package)))?;
            circuits.insert(name.clone(), p.circuit.clone());
        }
        Ok(serde_json::to_string(&compose::compose_with(&c, &circuits)).unwrap())
    }

    fn library(json: &str) -> Result<(std::collections::BTreeMap<String, compose::recipe::Recipe>, compose::recipe::Stock), JsValue> {
        let v: serde_json::Value = serde_json::from_str(json).map_err(err)?;
        let recipes = serde_json::from_value(v.get("recipes").cloned().unwrap_or(serde_json::json!({}))).map_err(err)?;
        let stock = serde_json::from_value(v.get("stock").cloned().unwrap_or(serde_json::json!({}))).map_err(err)?;
        Ok((recipes, stock))
    }

    /// One recipe expanded with `params` (`compose::recipe::Expanded` plus its circuit), for a
    /// block a person names by function.
    #[wasm_bindgen]
    pub fn recipe_expand(recipe_json: &str, params_json: &str, stock_json: &str) -> Result<String, JsValue> {
        let r: compose::recipe::Recipe = serde_json::from_str(recipe_json).map_err(err)?;
        let params = serde_json::from_str(params_json).map_err(err)?;
        let stock = serde_json::from_str(stock_json).map_err(err)?;
        let x = compose::recipe::expand(&r, &params, &stock).map_err(err)?;
        let parts: serde_json::Map<String, serde_json::Value> = x.circuit.parts.iter().map(|(k, p)| (k.clone(), serde_json::json!({ "symbol": p.symbol, "value": p.value, "footprint": p.footprint, "lcsc": p.lcsc, "mpn": p.mpn }))).collect();
        let ports: serde_json::Map<String, serde_json::Value> = x.circuit.ports.iter().map(|(k, p)| (k.clone(), serde_json::json!({ "kind": p.kind, "pins": p.pins }))).collect();
        Ok(serde_json::json!({ "parts": parts, "nets": x.circuit.nets, "ports": ports, "interfaces": x.ports["interfaces"], "volts": x.volts, "chosen": x.chosen }).to_string())
    }

    /// The parametric generators and their parameters.
    #[wasm_bindgen]
    pub fn shape_generators() -> String {
        serde_json::to_string(shape::generators::GENERATORS).unwrap()
    }

    /// Features from a drawing: `kind` is "svg" or "dxf".
    #[wasm_bindgen]
    pub fn shape_import(text: &str, kind: &str) -> Result<String, JsValue> {
        let f = match kind {
            "svg" => shape::import::svg(text),
            "dxf" => shape::import::dxf(text),
            _ => Err(format!("unknown drawing kind `{kind}`")),
        }
        .map_err(err)?;
        Ok(serde_json::to_string(&f).unwrap())
    }

    #[wasm_bindgen]
    impl Board {
        /// From `ccomp floorplan --problem` output.
        #[wasm_bindgen(constructor)]
        pub fn new(problem_json: &str) -> Result<Board, JsValue> {
            let pr: Problem = serde_json::from_str(problem_json).map_err(err)?;
            let outline = pr.outline_polygon.as_ref().map(|p| p.iter().map(|q| [q.0, q.1]).collect()).or(pr.outline.map(|o| vec![[o[0], o[1]], [o[2], o[1]], [o[2], o[3]], [o[0], o[3]]]));
            let mut b = Board { s: Session::new(pr), scene: board_draw::Scene::new(), drawing: None, outline };
            b.rebuild();
            Ok(b)
        }

        /// `{parts: {REF: {x, y, rotation}}}`, as `ccomp floorplan` writes it.
        pub fn set_placement(&mut self, json: &str) -> Result<(), JsValue> {
            #[derive(serde::Deserialize)]
            struct P {
                parts: BTreeMap<String, Pose>,
            }
            let p: P = serde_json::from_str(json).map_err(err)?;
            self.s.set_placement(&p.parts);
            self.moved();
            Ok(())
        }

        /// The board's `.kicad_pcb` text with this placement written into it, as pcbnew would save it.
        pub fn write_kicad_pcb(&self, text: &str) -> Result<String, JsValue> {
            self.s.write_kicad_pcb(text).map_err(err)
        }

        /// Take the placement the board's `.kicad_pcb` has (the inverse of `write_kicad_pcb`).
        pub fn set_placement_from_kicad_pcb(&mut self, text: &str) -> Result<(), JsValue> {
            let b = KicadBoard::read(text).map_err(err)?;
            let parts = self.s.placement_of(&b);
            self.s.set_placement(&parts);
            self.moved();
            Ok(())
        }

        pub fn placement(&self) -> String {
            serde_json::to_string(&serde_json::json!({ "parts": self.s.placement() })).unwrap()
        }

        /// The problem as given, with locks as they are now.
        pub fn problem(&self) -> String {
            serde_json::to_string(self.s.problem()).unwrap()
        }

        pub fn parts(&self) -> String {
            serde_json::to_string(&self.s.parts()).unwrap()
        }

        pub fn index(&self, reference: &str) -> i32 {
            self.s.index(reference).map(|i| i as i32).unwrap_or(-1)
        }

        /// Every part's x, y, rotation, in problem order: cheap to call every frame.
        pub fn poses(&self) -> Vec<f64> {
            self.s.poses().iter().flat_map(|p| [p.x, p.y, p.rotation]).collect()
        }

        /// The outline as `[[x, y], ...]` JSON, or an empty string for none.
        pub fn set_outline(&mut self, json: &str) -> Result<(), JsValue> {
            let p = if json.trim().is_empty() { None } else { Some(serde_json::from_str(json).map_err(err)?) };
            self.outline = p.clone();
            self.s.set_outline(p);
            self.rebuild();
            Ok(())
        }

        /// `[{name, rect: [x0, y0, x1, y1], side}]` JSON: the shape's keep-outs, holes and cut-outs.
        pub fn set_keepouts(&mut self, json: &str) -> Result<(), JsValue> {
            #[derive(serde::Deserialize)]
            struct K {
                name: String,
                rect: [f64; 4],
                side: String,
            }
            let ks: Vec<K> = serde_json::from_str(json).map_err(err)?;
            self.s.set_keepouts(&ks.into_iter().map(|k| (k.name, k.rect, k.side)).collect::<Vec<_>>());
            self.rebuild();
            Ok(())
        }

        /// `poses()` back: x, y, rotation per part, in problem order.
        pub fn set_poses(&mut self, flat: &[f64]) {
            let poses: Vec<Pose> = flat.chunks_exact(3).map(|c| Pose { x: c[0], y: c[1], rotation: c[2] }).collect();
            self.s.set_poses(&poses);
            self.moved();
        }

        pub fn ratsnest(&self) -> Vec<f64> {
            self.s.ratsnest().0
        }

        pub fn ratsnest_nets(&self) -> Vec<u32> {
            self.s.ratsnest().1
        }

        pub fn metrics(&self) -> Result<String, JsValue> {
            Ok(serde_json::to_string(&self.s.evaluate().map_err(err)?).unwrap())
        }

        pub fn move_to(&mut self, i: usize, x: f64, y: f64) {
            self.s.move_to(i, x, y);
            self.moved();
        }

        pub fn rotate(&mut self, i: usize) {
            self.s.rotate(i);
            self.moved();
        }

        pub fn set_locked(&mut self, i: usize, locked: bool) {
            self.s.set_locked(i, locked);
        }

        /// `sa` or `analytic`: how `anneal` starts a placement from scratch.
        pub fn set_placer(&mut self, placer: &str) -> Result<(), JsValue> {
            self.s.set_placer(placer).map_err(err)
        }

        /// Runs to the end (or until `on` returns false), calling `on(posesFloat64Array,
        /// metricsJson, done, total)` every `every` moves. Returns the placement's JSON.
        pub fn anneal(&mut self, seed: u32, moves: u32, warm: bool, every: u32, on: &js_sys::Function) -> String {
            let mut cb = |s: &Snapshot| {
                let poses: Vec<f64> = s.poses.iter().flat_map(|p| [p.x, p.y, p.rotation]).collect();
                let r = on.call4(
                    &JsValue::NULL,
                    &js_sys::Float64Array::from(&poses[..]).into(),
                    &JsValue::from_str(&serde_json::to_string(&s.metrics).unwrap()),
                    &JsValue::from(s.done as u32),
                    &JsValue::from(s.total as u32),
                );
                !matches!(r, Ok(v) if v == JsValue::FALSE)
            };
            let p = self.s.anneal(seed as u64, moves as usize, warm, every as usize, &mut cb);
            self.moved();
            serde_json::to_string(&p).unwrap()
        }

        // ---- drawing, through board-draw

        /// The board's mesh for board-draw's renderer at `px` mm per screen pixel: rebuilt when
        /// anything but poses changed, otherwise only the moved parts' vertices are rewritten.
        pub fn mesh(&mut self, px: f32, mask: u32) -> Vec<f32> {
            let mask = mask as u64;
            match &mut self.drawing {
                Some(d) => {
                    self.scene.redraw(d, px, mask);
                }
                None => self.drawing = Some(self.scene.draw(px, mask)),
            }
            self.drawing.as_ref().unwrap().vertices.clone()
        }

        /// Composer's overlay (block colours, ratsnest, locks, selection, a dashed box) as a mesh;
        /// `opts` is `draw::OverlayOpts` as JSON.
        pub fn overlay(&self, px: f32, opts: &str) -> Result<Vec<f32>, JsValue> {
            let o: crate::draw::OverlayOpts = serde_json::from_str(opts).map_err(err)?;
            Ok(crate::draw::overlay(&self.s, px, &o))
        }

        pub fn grid(&self, px: f32, x0: f32, y0: f32, x1: f32, y1: f32, dark: bool) -> Vec<f32> {
            crate::draw::grid(px, x0, y0, x1, y1, dark)
        }

        /// The part a click at (x, y) means, as a problem index, or -1: front before back,
        /// smallest first (board-draw's picking order).
        pub fn pick(&self, x: f32, y: f32) -> i32 {
            self.scene.part_at(x, y).map(|id| id - 1).unwrap_or(-1)
        }

        /// A side turned off: its parts are neither picked nor labelled.
        pub fn set_side_hidden(&mut self, back: bool, hidden: bool) {
            if back {
                self.scene.style.hide_back = hidden;
            } else {
                self.scene.style.hide_front = hidden;
            }
        }

        /// Labels to write for this view: `[{i, name, x, y, size}]`, `i` a problem index.
        pub fn labels(&self, px_per_mm: f32, min_px: f32, selected: &[i32]) -> String {
            let ids: Vec<i32> = selected.iter().map(|&i| i + 1).collect();
            let l: Vec<_> = self
                .scene
                .labels(px_per_mm, min_px, &ids)
                .into_iter()
                .map(|l| serde_json::json!({ "i": l.part - 1, "name": l.name, "x": l.at[0], "y": l.at[1], "size": l.size }))
                .collect();
            serde_json::to_string(&l).unwrap()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn battery() -> Session {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../web/public/examples/rp2350-battery");
        let pr: Problem = serde_json::from_str(&std::fs::read_to_string(root.join("problem.json")).unwrap()).unwrap();
        let mut s = Session::new(pr);
        #[derive(serde::Deserialize)]
        struct P {
            parts: BTreeMap<String, Pose>,
        }
        let p: P = serde_json::from_str(&std::fs::read_to_string(root.join("placement.json")).unwrap()).unwrap();
        s.set_placement(&p.parts);
        s
    }

    /// The board's footprints, from `(footprint` to its closing paren, in file order.
    fn footprints(text: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut cur: Option<Vec<&str>> = None;
        for line in text.lines() {
            if line.starts_with("\t(footprint ") {
                cur = Some(vec![line]);
            } else if let Some(c) = cur.as_mut() {
                c.push(line);
                if line == "\t)" {
                    out.push(cur.take().unwrap().join("\n"));
                }
            }
        }
        out
    }

    #[test]
    fn the_placement_is_written_into_the_board_as_pcbnew_writes_it() {
        // boards/easy2350/floorplan: the composed board, and the same placement put on it by
        // pcbnew (tools/apply_placement.py, J2 and J3 on the back). The app's example is that
        // placement.
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let ex = root.join("app/web/public/examples/easy2350");
        let pr: Problem = serde_json::from_str(&std::fs::read_to_string(ex.join("problem.json")).unwrap()).unwrap();
        let mut s = Session::new(pr);
        #[derive(serde::Deserialize)]
        struct P {
            parts: BTreeMap<String, Pose>,
        }
        let p: P = serde_json::from_str(&std::fs::read_to_string(ex.join("placement.json")).unwrap()).unwrap();
        s.set_placement(&p.parts);
        let fp = root.join("boards/easy2350/floorplan");
        let out = s.write_kicad_pcb(&std::fs::read_to_string(fp.join("composed.kicad_pcb")).unwrap()).unwrap();
        let pcbnew = std::fs::read_to_string(fp.join("placed.kicad_pcb")).unwrap();
        let (ours, theirs) = (footprints(&out), footprints(&pcbnew));
        assert_eq!(ours.len(), 58);
        for (a, b) in ours.iter().zip(&theirs) {
            assert_eq!(a, b);
        }
    }

    #[test]
    fn a_board_read_and_written_back_is_unchanged() {
        // Opened from a project's store, the app takes its placement from the .kicad_pcb; saving
        // with nothing moved must give the same bytes.
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let ex = root.join("app/web/public/examples/easy2350");
        let pr: Problem = serde_json::from_str(&std::fs::read_to_string(ex.join("problem.json")).unwrap()).unwrap();
        let mut s = Session::new(pr);
        let text = std::fs::read_to_string(root.join("boards/easy2350/floorplan/placed.kicad_pcb")).unwrap();
        let b = KicadBoard::read(&text).unwrap();
        let parts = s.placement_of(&b);
        assert_eq!(parts.len(), 58);
        s.set_placement(&parts);
        assert_eq!(s.write_kicad_pcb(&text).unwrap(), text);
        // The same poses as the placement pcbnew put there.
        #[derive(serde::Deserialize)]
        struct P {
            parts: BTreeMap<String, Pose>,
        }
        let p: P = serde_json::from_str(&std::fs::read_to_string(ex.join("placement.json")).unwrap()).unwrap();
        for (r, q) in &p.parts {
            let Some(got) = parts.get(r).copied() else {
                assert!(r.contains(':'), "{r} is not on the board");
                continue;
            };
            assert!((got.x - q.x).abs() < 1e-6 && (got.y - q.y).abs() < 1e-6, "{r}: {got:?} vs {q:?}");
        }
        assert!(b.nets["/+3V3"].contains(&"U1.1".to_string()));
        assert_eq!(b.parts["C1"].value, "100n");
    }

    #[test]
    fn the_saved_placement_is_legal_here_too() {
        let s = battery();
        let m = s.evaluate().unwrap();
        assert_eq!(m.overlaps, 0, "{:?}", m.overlap_pairs);
        let (seg, nets) = s.ratsnest();
        assert_eq!(seg.len(), nets.len() * 4);
        assert!(!nets.is_empty());
    }

    #[test]
    fn a_group_moves_and_turns_as_one() {
        let mut s = battery();
        let g = &s.problem().groups[0];
        let (a, m, off) = (g.anchor, g.members[0].0, g.members[0].1);
        let before = (s.poses()[m].x - s.poses()[a].x, s.poses()[m].y - s.poses()[a].y);
        s.move_to(m, 5.0, 5.0);
        let after = (s.poses()[m].x - s.poses()[a].x, s.poses()[m].y - s.poses()[a].y);
        assert!((before.0 - after.0).abs() < 1e-9 && (before.1 - after.1).abs() < 1e-9);
        s.rotate(a);
        // Turned, a member sits at its offset in the anchor's frame (saved poses are rounded to
        // 1 um, so before the turn it was only within that).
        let q = rot(off, s.poses()[a].rotation);
        assert!((s.poses()[m].x - s.poses()[a].x - q.0).abs() < 1e-9 && (s.poses()[m].y - s.poses()[a].y - q.1).abs() < 1e-9);
    }

    #[test]
    fn a_locked_part_stays_and_a_warm_anneal_keeps_the_board_legal() {
        let mut s = battery();
        let u = s.index("U4").unwrap();
        s.set_locked(u, true);
        let at = s.poses()[u];
        let mut seen = 0;
        let p = s.anneal(3, 30_000, true, 5_000, &mut |snap| {
            seen += 1;
            assert_eq!(snap.total, 30_000);
            true
        });
        assert_eq!(seen, 6);
        assert_eq!(s.poses()[u], at);
        assert_eq!(p.metrics.overlaps, 0, "{:?}", p.metrics.overlap_pairs);
        // Refining never hands back a worse board than it was given.
        let before = battery().evaluate().unwrap().wire_mm;
        assert!(p.metrics.wire_mm <= before + 1e-6, "{} > {before}", p.metrics.wire_mm);
    }

    #[test]
    fn the_analytic_start_places_the_battery_board_legally() {
        let mut s = battery();
        assert!(s.set_placer("anything").is_err());
        s.set_placer("analytic").unwrap();
        let p = s.anneal(1, 20_000, false, 0, &mut |_| true);
        assert_eq!(p.metrics.overlaps, 0, "{:?}", p.metrics);
        assert_eq!(s.evaluate().unwrap().overlaps, 0);
    }

    #[test]
    fn shape_keepouts_are_fixed_parts_that_replace_each_other() {
        let mut s = battery();
        let n = s.problem().parts.len();
        s.set_keepouts(&[("hole 1".into(), [0.0, 0.0, 4.0, 4.0], "both".into()), ("cut".into(), [10.0, 10.0, 12.0, 12.0], "front".into())]);
        assert_eq!(s.problem().parts.len(), n + 2);
        assert!(s.locked(n) && s.index("shape:cut") == Some(n + 1));
        s.set_keepouts(&[("hole 1".into(), [0.0, 0.0, 4.0, 4.0], "both".into())]);
        assert_eq!(s.problem().parts.len(), n + 1);
        let p = s.anneal(2, 20_000, false, 0, &mut |_| true);
        let u = p.parts["shape:hole 1"];
        assert_eq!((u.x, u.y), (2.0, 2.0));
    }

    /// board-draw puts every pad where the floorplan does, turned and on either side.
    #[test]
    fn board_draw_poses_are_the_floorplans() {
        for board in ["rp2350-battery", "bolt-earring", "easy2350"] {
            let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../web/public/examples/{board}/problem.json"));
            let pr: Problem = serde_json::from_str(&std::fs::read_to_string(root).unwrap()).unwrap();
            let mut s = Session::new(pr);
            let poses: Vec<Pose> = (0..s.problem().parts.len()).map(|i| Pose { x: i as f64 * 1.5, y: -(i as f64), rotation: 90.0 * (i % 4) as f64 }).collect();
            s.set_poses(&poses);
            let sc = draw::build(&s, None);
            let mut sides = [0, 0];
            for (i, p) in s.problem().parts.iter().enumerate() {
                sides[p.back as usize] += 1;
                let xf = sc.part(draw::part_id(i)).unwrap().pose.transform();
                for q in p.pins.values() {
                    let want = draw::floorplan_point(&s, i, *q);
                    let got = xf.apply([q.0 as f32, q.1 as f32]);
                    assert!((got[0] as f64 - want[0]).abs() < 1e-3 && (got[1] as f64 - want[1]).abs() < 1e-3, "{board} {} at {}: {got:?} vs {want:?}", p.reference, poses[i].rotation);
                }
            }
            if board == "bolt-earring" {
                assert!(sides[0] > 0 && sides[1] > 0, "both sides checked");
            }
        }
    }

    #[test]
    fn an_anneal_can_be_stopped() {
        let mut s = battery();
        let mut calls = 0;
        s.anneal(1, 100_000, false, 1_000, &mut |_| {
            calls += 1;
            calls < 3
        });
        assert_eq!(calls, 3);
    }
}
