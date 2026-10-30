//! The board as board-draw sees it: the scene Composer and UltraRoute Studio share. Each part's
//! pads (as discs at its pin centres: the floorplan knows no pad shapes) and courtyard go in once,
//! in the part's own frame; moving a part is a pose. Overlays Composer adds on top (block colours,
//! the ratsnest, selection, locks, a dashed outline when the floorplanner sizes the board) are a
//! second mesh, rebuilt each frame.

use board_draw::{AreaKind, Geom, Item, Mesh, Pose as BdPose, Prim, Rgba, Scene};
use floorplan::{rot, Pose};

use crate::Session;

/// Part `i` of the problem is board-draw part `i + 1`; 0 is the board itself.
pub fn part_id(i: usize) -> i32 {
    i as i32 + 1
}

const PAD_R: f32 = 0.22;

/// The floorplan's pose as board-draw's. The floorplan turns a part counter-clockwise on screen
/// (KiCad's sense, y down) and mirrors a bottom-side part in y; board-draw turns from +x towards
/// +y and mirrors in x. Mirroring in y is mirroring in x and a half turn, so a bottom part's
/// angle gains 180 degrees before the sign flips.
pub fn bd_pose(p: &Pose, back: bool) -> BdPose {
    let r = if back { -(p.rotation + 180.0) } else { -p.rotation };
    BdPose { x: p.x as f32, y: p.y as f32, rotation: r.rem_euclid(360.0) as f32, bottom: back }
}

/// Every part's items into a fresh scene: pads and courtyard in the part's own frame.
pub fn build(s: &Session, outline: Option<&[[f64; 2]]>) -> Scene {
    let mut sc = Scene::new();
    sc.set_layers(&["F.Cu", "B.Cu"]);
    let pr = s.problem();
    let mut next = 1u32;
    for (i, p) in pr.parts.iter().enumerate() {
        let id = part_id(i);
        let mut items = Vec::new();
        for q in p.pins.values() {
            let at = [q.0 as f32, q.1 as f32];
            items.push((next, Item::new(0, id, Geom::Pad { at, pads: vec![(0, Prim::Circle([at[0], at[1], PAD_R]))], drill: None })));
            next += 1;
        }
        let b = p.bbox;
        let poly = vec![b[0] as f32, b[1] as f32, b[2] as f32, b[1] as f32, b[2] as f32, b[3] as f32, b[0] as f32, b[3] as f32];
        items.push((next, Item::new(0, id, Geom::Area { kind: AreaKind::Courtyard, tiles: vec![(0, poly)] })));
        next += 1;
        sc.insert_part(id, &p.reference, bd_pose(&s.poses()[i], p.back), items);
    }
    if let Some(o) = outline {
        let ring: Vec<f32> = o.iter().flat_map(|p| [p[0] as f32, p[1] as f32]).collect();
        sc.insert(next, Item::new(0, 0, Geom::Outline { polys: vec![ring] }));
    }
    sc
}

/// Poses after a move or an anneal: cheap, no rebuild.
pub fn sync_poses(sc: &mut Scene, s: &Session) {
    let pr = s.problem();
    for (i, p) in s.poses().iter().enumerate() {
        sc.set_pose(part_id(i), bd_pose(p, pr.parts[i].back));
    }
}

/// What the overlay shows, from the page.
#[derive(serde::Deserialize, Default)]
pub struct OverlayOpts {
    #[serde(default)]
    pub selected: Vec<usize>,
    #[serde(default)]
    pub ratsnest: bool,
    #[serde(default)]
    pub blocks: bool,
    #[serde(default)]
    pub front: bool,
    #[serde(default)]
    pub back: bool,
    /// A board box to draw dashed: the floorplanner's own outline when none is set.
    #[serde(default)]
    pub dashed_box: Option<[f64; 4]>,
    #[serde(default)]
    pub dark: bool,
}

/// Block colours, the ratsnest, selection rings and locks, as a mesh drawn over the board.
pub fn overlay(s: &Session, px: f32, o: &OverlayOpts) -> Vec<f32> {
    let mut m = Mesh::new(px);
    let parts = s.parts();
    if o.blocks {
        for p in &parts {
            if (p.back && !o.back) || (!p.back && !o.front) {
                continue;
            }
            let Some(key) = p.block.as_ref().or(p.group.as_ref()) else { continue };
            let c = hue(key, if p.back { 0.10 } else { 0.28 }, o.dark);
            m.fill(&ring(&p.outline), [0.0, 0.0], c);
        }
    }
    if let Some(b) = o.dashed_box {
        let pts = [b[0], b[1], b[2], b[1], b[2], b[3], b[0], b[3]].map(|v| v as f32);
        m.dashed_ring(&pts, px * 1.2, px * 6.0, if o.dark { [0.24, 0.81, 0.56, 0.9] } else { [0.12, 0.48, 0.32, 0.9] });
    }
    if o.ratsnest {
        let (seg, _) = s.ratsnest();
        let c: Rgba = if o.dark { [0.47, 0.63, 1.0, 0.55] } else { [0.24, 0.35, 0.78, 0.55] };
        for k in seg.chunks_exact(4) {
            m.segment([k[0] as f32, k[1] as f32], [k[2] as f32, k[3] as f32], px * 0.6, c);
        }
    }
    let lock: Rgba = if o.dark { [0.94, 0.71, 0.24, 0.9] } else { [0.69, 0.49, 0.06, 0.9] };
    for p in &parts {
        if p.locked && !p.reference.starts_with("shape:") && ((p.back && o.back) || (!p.back && o.front)) {
            let c = centre(&p.outline);
            m.disc(c, (px * 3.0).min(0.35), lock);
        }
    }
    let sel: Rgba = if o.dark { [1.0, 0.54, 0.30, 1.0] } else { [0.85, 0.28, 0.06, 1.0] };
    for &i in &o.selected {
        if let Some(p) = parts.get(i) {
            m.ring(&ring(&p.outline), px * 1.6, sel);
        }
    }
    m.into_vec()
}

/// A light grid: 1 mm lines where they are wide enough apart, 10 mm always.
pub fn grid(px: f32, x0: f32, y0: f32, x1: f32, y1: f32, dark: bool) -> Vec<f32> {
    let mut m = Mesh::new(px);
    for (step, a) in [(1.0f32, 0.05f32), (10.0, 0.11)] {
        if step / px < 6.0 {
            continue;
        }
        let c: Rgba = if dark { [1.0, 1.0, 1.0, a] } else { [0.0, 0.0, 0.0, a * 0.8] };
        let mut x = (x0 / step).floor() * step;
        while x <= x1 {
            m.segment([x, y0], [x, y1], px * 0.5, c);
            x += step;
        }
        let mut y = (y0 / step).floor() * step;
        while y <= y1 {
            m.segment([x0, y], [x1, y], px * 0.5, c);
            y += step;
        }
    }
    m.into_vec()
}

fn ring(o: &[[f64; 2]; 4]) -> Vec<f32> {
    o.iter().flat_map(|p| [p[0] as f32, p[1] as f32]).collect()
}

fn centre(o: &[[f64; 2]; 4]) -> [f32; 2] {
    let (x, y) = o.iter().fold((0.0, 0.0), |a, p| (a.0 + p[0], a.1 + p[1]));
    [(x / 4.0) as f32, (y / 4.0) as f32]
}

/// A colour per block name, as the placeholder scene picked it.
fn hue(name: &str, alpha: f32, dark: bool) -> Rgba {
    let mut h: u32 = 0;
    for c in name.chars() {
        h = h.wrapping_mul(31).wrapping_add(c as u32);
    }
    let hue = (h % 360) as f32 / 60.0;
    let (s, l) = if dark { (0.55, 0.55) } else { (0.55, 0.5) };
    let c = (1.0 - (2.0 * l - 1.0f32).abs()) * s;
    let x = c * (1.0 - (hue % 2.0 - 1.0).abs());
    let (r, g, b) = match hue as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    [r + m, g + m, b + m, alpha]
}

/// A point in part `i`'s frame, where the floorplan puts it: for checking board-draw's poses.
pub fn floorplan_point(s: &Session, i: usize, q: (f64, f64)) -> [f64; 2] {
    let p = &s.problem().parts[i];
    let pose = s.poses()[i];
    let q = if p.back { (q.0, -q.1) } else { q };
    let r = rot(q, pose.rotation);
    [pose.x + r.0, pose.y + r.1]
}
