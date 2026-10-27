//! The buck generator: places CIN, the inductor and COUT around the regulator IC's pins, with
//! the copper that joins them, by enumerating poses on a grid and keeping the best few at each
//! step (beam search).
//!
//! Copper is an escape from each IC pin straight out past its tip, then one straight run to the
//! passive's pad, as people lay out a QFN regulator. Every pose is checked for courtyard overlap
//! and copper clearance before it is scored.
//!
//! Score, in mm²: `w_hot · hot loop + w_sw · switch-node copper + w_out · output loop`, with the
//! feedback path's clearance to switch-node copper a hard floor. Order: CIN (only poses that leave
//! the switch pin a way out), the inductor (scored with a lookahead on the output loop), COUT,
//! then the feedback run (straight or one bend), then ground vias clear of the keepout.

use std::collections::BTreeMap;

use pcb::geom::{self, P};
use pcb::Board;
use serde::{Deserialize, Serialize};
use verify::buck::BuckRoles;

use crate::layout::{Frame, LKeepout, LTrack, LVia, Layout, Pose};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Params {
    /// Placement grid, mm.
    pub grid: f64,
    /// How far out from the switch pin's row the parts may go, mm.
    pub reach: f64,
    /// How far along the row either side of the switch pin, mm.
    pub along: f64,
    pub rotations: Vec<f64>,
    /// Copper-to-copper clearance for every pair of nets, mm. `None`: the board's own net-class
    /// clearances from its `.kicad_pro`.
    pub clearance: Option<f64>,
    /// Widths to try for the power runs (VIN, SW, output, GND), widest first, mm. The widest
    /// that fits is kept.
    pub power_widths: Vec<f64>,
    /// Width of the feedback run, mm.
    pub fb_width: f64,
    /// How far past a pin's tip the escape runs before turning, mm.
    pub escape: f64,
    /// How long a run stays at its pin's width before widening, mm.
    pub neck: f64,
    /// Floor on the feedback path's clearance to switch-node copper, mm.
    pub fb_clearance_min: f64,
    pub w_hot: f64,
    pub w_sw: f64,
    pub w_out: f64,
    /// Poses kept after each step.
    pub beam: usize,
    /// Inductor poses kept for the COUT step: wider than `beam`, since the output loop is only
    /// known once COUT is down.
    pub beam_inductor: usize,
    /// Kept poses differ by at least a cell of this size, mm.
    pub diversity: f64,
    /// How many COUT arrangements, best first, may be tried for a feedback route before giving up.
    pub fb_attempts: usize,
    /// Ground vias beside CIN's ground pad, to the plane.
    pub gnd_vias: usize,
    pub via_size: f64,
    pub via_drill: f64,
    /// The layer the copper is drawn on; the next layer down gets the switch-node keepout.
    pub layer: String,
    /// Keep the layer under the switch node clear (a rule area), `None` for no keepout.
    pub keepout_layer: Option<String>,
    /// How far the keepout reaches past the switch-node copper, mm.
    pub keepout_margin: f64,
}

impl Default for Params {
    fn default() -> Self {
        Params {
            grid: 0.1,
            reach: 5.0,
            along: 4.0,
            rotations: vec![0.0, 90.0, 180.0, 270.0],
            clearance: None,
            power_widths: vec![0.4, 0.3, 0.2],
            fb_width: 0.2,
            escape: 0.2,
            neck: 0.6,
            fb_clearance_min: 0.3,
            w_hot: 1.0,
            w_sw: 0.5,
            w_out: 0.5,
            beam: 60,
            diversity: 0.3,
            beam_inductor: 240,
            fb_attempts: 20_000,
            gnd_vias: 2,
            via_size: 0.6,
            via_drill: 0.3,
            layer: "F.Cu".into(),
            keepout_layer: Some("In1.Cu".into()),
            keepout_margin: 0.2,
        }
    }
}

/// The generator's own estimate of the metrics for one arrangement. The verifier measures the
/// written board afterwards; the two should agree closely.
#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct Estimate {
    pub hot_loop_area_mm2: f64,
    pub switch_node_area_mm2: f64,
    pub output_loop_area_mm2: f64,
    pub feedback_clearance_mm: f64,
    pub score: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Generated {
    pub layout: Layout,
    pub estimate: Estimate,
    /// Poses tried and found legal, per step.
    pub tried: [usize; 3],
    pub legal: [usize; 3],
    /// The next best arrangements' estimates, for a sense of how flat the optimum is.
    pub runners_up: Vec<Estimate>,
}

// ---------------------------------------------------------------------------------------------
// Geometry in the anchor's frame.

#[derive(Debug, Clone)]
struct Shape {
    net: String,
    poly: Vec<P>,
    lo: P,
    hi: P,
}

impl Shape {
    fn new(net: &str, poly: Vec<P>) -> Shape {
        let (lo, hi) = geom::bbox(poly.iter().copied()).unwrap_or(([0.0; 2], [0.0; 2]));
        Shape { net: net.to_string(), poly, lo, hi }
    }

    /// Whether the boxes come within `gap` of each other.
    fn near(&self, lo: P, hi: P, gap: f64) -> bool {
        self.hi[0] + gap >= lo[0] && hi[0] + gap >= self.lo[0] && self.hi[1] + gap >= lo[1] && hi[1] + gap >= self.lo[1]
    }
}

#[derive(Debug, Clone)]
struct Seg {
    net: String,
    a: P,
    b: P,
    w: f64,
}

#[derive(Debug, Clone)]
struct LocalPad {
    net: String,
    centre: P,
    poly: Vec<P>,
}

/// A part's pads and courtyard in its own frame.
#[derive(Debug, Clone)]
struct Footprint {
    pads: Vec<LocalPad>,
    courtyard: Vec<P>,
    /// Whether copper may not pass under it (the inductor).
    no_copper_under: bool,
}

impl Footprint {
    fn of(board: &Board, reference: &str) -> Result<Footprint, String> {
        let part = board.parts.get(reference).ok_or_else(|| format!("part {reference} is not on the board"))?;
        if part.back {
            return Err(format!("{reference} is on the back; the generator places front-side parts"));
        }
        let f = Frame { at: part.at, rotation: part.rotation };
        let pads: Vec<LocalPad> = part
            .pads
            .iter()
            .map(|&i| &board.pads[i])
            .map(|p| LocalPad { net: p.net.clone(), centre: f.to_local(p.centre), poly: p.shape.first().map(|s| s.iter().map(|q| f.to_local(*q)).collect()).unwrap_or_default() })
            .collect();
        let courtyard = match &part.courtyard {
            Some(c) => c.iter().map(|q| f.to_local(*q)).collect(),
            None => {
                let pts: Vec<P> = pads.iter().flat_map(|p| p.poly.iter().copied()).collect();
                let (lo, hi) = geom::bbox(pts).ok_or_else(|| format!("{reference} has no pads"))?;
                let m = 0.25;
                vec![[lo[0] - m, lo[1] - m], [hi[0] + m, lo[1] - m], [hi[0] + m, hi[1] + m], [lo[0] - m, hi[1] + m]]
            }
        };
        Ok(Footprint { pads, courtyard, no_copper_under: false })
    }

    fn placed(&self, pose: Pose) -> Placed {
        let f = Frame { at: pose.at, rotation: pose.rotation };
        let pads: Vec<LocalPad> = self.pads.iter().map(|p| LocalPad { net: p.net.clone(), centre: f.to_board(p.centre), poly: p.poly.iter().map(|q| f.to_board(*q)).collect() }).collect();
        let body = self.no_copper_under.then(|| Body { hull: geom::hull(&pads.iter().flat_map(|p| p.poly.iter().copied()).collect::<Vec<_>>()), pads: pads.clone() });
        Placed {
            body,
            pads,
            courtyard: self.courtyard.iter().map(|q| f.to_board(*q)).collect(),
        }
    }
}

#[derive(Debug, Clone)]
struct Placed {
    pads: Vec<LocalPad>,
    courtyard: Vec<P>,
    body: Option<Body>,
}

impl Placed {
    fn pad(&self, net: &str) -> Option<&LocalPad> {
        self.pads.iter().find(|p| p.net == net)
    }
}

/// Clearance between two nets.
struct Clr {
    fixed: Option<f64>,
    per_net: std::collections::HashMap<String, f64>,
    floor: f64,
}

impl Clr {
    fn new(board: &Board, fixed: Option<f64>) -> Clr {
        let mut per_net = std::collections::HashMap::new();
        let floor = board.rules.as_ref().map_or(0.15, |r| r.min_clearance * 1e3);
        for p in &board.pads {
            if !per_net.contains_key(&p.net) {
                per_net.insert(p.net.clone(), board.clearance(&p.net, &p.net).unwrap_or(0.15));
            }
        }
        Clr { fixed, per_net, floor }
    }

    fn get(&self, a: &str, b: &str) -> f64 {
        if let Some(c) = self.fixed {
            return c;
        }
        let of = |n: &str| self.per_net.get(n).copied().unwrap_or(self.floor);
        of(a).max(of(b))
    }

    fn max(&self) -> f64 {
        self.fixed.unwrap_or_else(|| self.per_net.values().copied().fold(self.floor, f64::max))
    }
}

#[derive(Debug, Clone)]
struct Body {
    hull: Vec<P>,
    pads: Vec<LocalPad>,
}

impl Body {
    /// Whether a run puts copper under the body anywhere but on its own net's pads: sampled
    /// every 50 µm along its centreline.
    fn violated_by(&self, t: &Seg) -> bool {
        if geom::polyline_poly_dist(&[t.a, t.b], &self.hull) >= t.w / 2.0 {
            return false;
        }
        let n = (geom::dist(t.a, t.b) / 0.05).ceil().max(1.0) as usize;
        (0..=n).any(|i| {
            let q = geom::add(t.a, geom::scale(geom::sub(t.b, t.a), i as f64 / n as f64));
            geom::inside(&self.hull, q) && !self.pads.iter().any(|p| p.net == t.net && geom::inside(&p.poly, q))
        })
    }
}

/// What is already down: shapes and segments, checked for clearance against what comes next.
#[derive(Debug, Clone, Default)]
struct Scene {
    shapes: Vec<Shape>,
    segs: Vec<Seg>,
    courts: Vec<Vec<P>>,
    /// No-copper bodies: the inductor's footprint between its pads, with those pads. Inside a
    /// body a run may only lie on a pad of its own net (RP2350 datasheet §6.3.8 pp.411-412: no
    /// copper under the inductor).
    bodies: Vec<Body>,
}

impl Scene {
    fn shape_ok(&self, s: &Shape, clr: &Clr) -> bool {
        let reach = clr.max();
        for o in self.shapes.iter().filter(|o| o.net != s.net && o.near(s.lo, s.hi, reach)) {
            if geom::poly_poly_dist(&o.poly, &s.poly) < clr.get(&o.net, &s.net) {
                return false;
            }
        }
        for t in self.segs.iter().filter(|t| t.net != s.net) {
            if geom::polyline_poly_dist(&[t.a, t.b], &s.poly) < clr.get(&t.net, &s.net) + t.w / 2.0 {
                return false;
            }
        }
        true
    }

    fn seg_ok(&self, t: &Seg, clr: &Clr) -> bool {
        if self.bodies.iter().any(|b| b.violated_by(t)) {
            return false;
        }
        let lo = [t.a[0].min(t.b[0]), t.a[1].min(t.b[1])];
        let hi = [t.a[0].max(t.b[0]), t.a[1].max(t.b[1])];
        let reach = clr.max();
        for o in self.shapes.iter().filter(|o| o.net != t.net && o.near(lo, hi, reach + t.w)) {
            if geom::polyline_poly_dist(&[t.a, t.b], &o.poly) < clr.get(&o.net, &t.net) + t.w / 2.0 {
                return false;
            }
        }
        for u in self.segs.iter().filter(|u| u.net != t.net) {
            if geom::seg_seg_dist(t.a, t.b, u.a, u.b) < clr.get(&u.net, &t.net) + (t.w + u.w) / 2.0 {
                return false;
            }
        }
        true
    }

    fn court_ok(&self, c: &[P]) -> bool {
        !self.courts.iter().any(|o| geom::convex_overlap(o, c, 1e-6))
    }

    /// The part alone: courtyard clear, pads at clearance.
    fn part_ok(&self, p: &Placed, clr: &Clr) -> bool {
        self.court_ok(&p.courtyard) && p.pads.iter().all(|q| self.shape_ok(&Shape::new(&q.net, q.poly.clone()), clr))
    }

    /// Runs for a part that fits: at clearance from the scene and from the part's own pads.
    /// Runs of different nets in one option are checked against each other too.
    fn segs_ok(&self, p: &Placed, segs: &[Seg], clr: &Clr) -> bool {
        let under_body = |t: &Seg| p.body.as_ref().is_some_and(|b| b.violated_by(t));
        !segs.iter().any(under_body)
            && segs.iter().all(|s| self.seg_ok(s, clr))
            && segs.iter().all(|t| p.pads.iter().filter(|q| q.net != t.net).all(|q| geom::polyline_poly_dist(&[t.a, t.b], &q.poly) >= clr.get(&q.net, &t.net) + t.w / 2.0))
            && segs.iter().enumerate().all(|(i, t)| {
                segs[i + 1..].iter().filter(|u| u.net != t.net).all(|u| geom::seg_seg_dist(t.a, t.b, u.a, u.b) >= clr.get(&u.net, &t.net) + (t.w + u.w) / 2.0)
            })
    }

    fn commit(&mut self, p: &Placed, segs: &[Seg]) {
        self.courts.push(p.courtyard.clone());
        self.bodies.extend(p.body.clone());
        self.shapes.extend(p.pads.iter().map(|q| Shape::new(&q.net, q.poly.clone())));
        self.segs.extend(segs.iter().cloned());
    }
}

/// An IC pin: its centre, outward normal, width across, and the escape point past its tip.
#[derive(Debug, Clone)]
struct Pin {
    net: String,
    centre: P,
    escape: P,
    width: f64,
    poly: Vec<P>,
}

fn pin(board: &Board, frame: &Frame, ref_pin: &str, escape: f64) -> Result<Pin, String> {
    let p = board.pad(ref_pin).ok_or_else(|| format!("pad {ref_pin} is not on the board"))?;
    let poly: Vec<P> = p.shape.first().ok_or("pad has no shape")?.iter().map(|q| frame.to_local(*q)).collect();
    let c = frame.to_local(p.centre);
    let (lo, hi) = geom::bbox(poly.iter().copied()).unwrap();
    let (w, h) = (hi[0] - lo[0], hi[1] - lo[1]);
    // A QFN pin is long across the package edge: its long axis is the outward direction.
    let (n, len, width) = if w > h { ([c[0].signum(), 0.0], w, h) } else { ([0.0, c[1].signum()], h, w) };
    Ok(Pin { net: p.net.clone(), centre: c, escape: geom::add(c, geom::scale(n, len / 2.0 + escape)), width, poly })
}

/// A run from a pin's escape point to `to`: necked to the pin's width for `neck` mm, so runs
/// from neighbouring pins can fan out, then `width`.
fn run(pin: &Pin, to: P, width: f64, neck: f64) -> Vec<Seg> {
    let len = geom::dist(pin.escape, to);
    if len <= neck || width <= pin.width {
        return connection(&pin.net, &[pin.escape, to], &[width.min(pin.width.max(width))]);
    }
    let n = geom::add(pin.escape, geom::scale(geom::sub(to, pin.escape), neck / len));
    connection(&pin.net, &[pin.escape, n, to], &[pin.width, width])
}

/// A pin's run as points: the pin centre, its escape, then each segment's end.
fn path_points(pin: &Pin, segs: &[Seg]) -> Vec<P> {
    let mut pts = vec![pin.centre, pin.escape];
    pts.extend(segs.iter().map(|s| s.b));
    pts
}

fn path_len(pts: &[P]) -> f64 {
    pts.windows(2).map(|w| geom::dist(w[0], w[1])).sum()
}

fn connection(net: &str, pts: &[P], widths: &[f64]) -> Vec<Seg> {
    pts.windows(2).zip(widths).map(|(w, &width)| Seg { net: net.into(), a: w[0], b: w[1], w: width }).collect()
}

/// Scores a placed part against a state: its copper options, each with its estimate.
type Eval<'a> = dyn Fn(&State, &Placed) -> Vec<(Vec<Seg>, Estimate)> + Sync + 'a;

#[derive(Clone)]
struct State {
    poses: Vec<Pose>,
    placed: Vec<Placed>,
    scene: Scene,
    est: Estimate,
}

pub fn generate(board: &Board, roles: &BuckRoles, params: &Params) -> Result<Generated, String> {
    let p = params;
    let ic_ref = roles.ic();
    let ic = board.parts.get(ic_ref).ok_or_else(|| format!("anchor {ic_ref} is not on the board"))?;
    let frame = Frame { at: ic.at, rotation: ic.rotation };
    let (vin, sw, pgnd, fb) = (pin(board, &frame, &roles.vin, p.escape)?, pin(board, &frame, &roles.sw, p.escape)?, pin(board, &frame, &roles.pgnd, p.escape)?, pin(board, &frame, &roles.fb, p.escape)?);
    let (n_vin, n_sw, n_gnd, n_out) = (vin.net.clone(), sw.net.clone(), pgnd.net.clone(), fb.net.clone());

    // The anchor: its pads and courtyard are the fixed scene.
    let mut base = Scene::default();
    for &i in &ic.pads {
        let pad = &board.pads[i];
        for s in &pad.shape {
            base.shapes.push(Shape::new(&pad.net, s.iter().map(|q| frame.to_local(*q)).collect()));
        }
    }
    if let Some(c) = &ic.courtyard {
        base.courts.push(c.iter().map(|q| frame.to_local(*q)).collect());
    }
    // Pin escapes are copper too.
    for pn in [&vin, &sw, &pgnd, &fb] {
        base.segs.push(Seg { net: pn.net.clone(), a: pn.centre, b: pn.escape, w: pn.width });
    }

    let mut inductor = Footprint::of(board, &roles.inductor)?;
    inductor.no_copper_under = true;
    let fps = [Footprint::of(board, &roles.cin)?, inductor, Footprint::of(board, &roles.cout)?];
    for (fp, (r, nets)) in fps.iter().zip([(&roles.cin, [&n_vin, &n_gnd]), (&roles.inductor, [&n_sw, &n_out]), (&roles.cout, [&n_out, &n_gnd])]) {
        for net in nets {
            if !fp.pads.iter().any(|q| q.net == **net) {
                return Err(format!("{r} has no pad on `{net}`"));
            }
        }
    }

    // Candidate poses: a grid in the band outside the switch pin's row.
    let out_dir = geom::scale(geom::sub(sw.escape, sw.centre), 1.0 / geom::dist(sw.escape, sw.centre));
    let along_dir = [-out_dir[1], out_dir[0]];
    let mut poses = Vec::new();
    let steps_a = (p.along / p.grid).round() as i64;
    let steps_o = (p.reach / p.grid).round() as i64;
    for i in -steps_a..=steps_a {
        for j in 1..=steps_o {
            let at = geom::add(geom::add(sw.centre, geom::scale(along_dir, i as f64 * p.grid)), geom::scale(out_dir, j as f64 * p.grid));
            for &rotation in &p.rotations {
                poses.push(Pose { at, rotation });
            }
        }
    }

    let clr = &Clr::new(board, p.clearance);
    let pw = p.power_widths.iter().copied().fold(0.0, f64::max);
    let wmin = p.power_widths.iter().copied().fold(f64::INFINITY, f64::min);
    let mut tried = [0usize; 3];
    let mut legal = [0usize; 3];
    let pad_area = |q: &LocalPad| geom::area(&q.poly);
    let sw_ic_area = geom::area(&sw.poly);

    // Each step tries every pose against each state kept by the last, scores the legal ones,
    // and builds full states only for the best `beam`.
    type Cand = (usize, Pose, usize, Estimate);
    // Every legal pose against every state, best first: (parent, pose, copper option, estimate).
    let candidates = |states: &[State], fp: &Footprint, tried: &mut usize, legal: &mut usize, eval: &Eval| -> Vec<Cand> {
        use rayon::prelude::*;
        *tried += states.len() * poses.len();
        let mut cands: Vec<Cand> = states
            .par_iter()
            .enumerate()
            .flat_map_iter(|(si, st)| {
                poses.iter().filter_map(move |&pose| {
                    let placed = fp.placed(pose);
                    if !st.scene.part_ok(&placed, clr) {
                        return None;
                    }
                    let mut options = eval(st, &placed);
                    options.sort_by(|a, b| a.1.score.total_cmp(&b.1.score));
                    let k = options.iter().position(|(segs, _)| st.scene.segs_ok(&placed, segs, clr))?;
                    Some((si, pose, k, options[k].1))
                })
            })
            .collect();
        *legal += cands.len();
        cands.sort_by(|a, b| a.3.score.total_cmp(&b.3.score));
        // Keep the beam varied: one candidate per parent, cell and rotation, so near-copies of
        // one pose cannot crowd out every alternative.
        let mut seen = std::collections::HashSet::new();
        let cell = p.diversity;
        cands.retain(|(si, pose, _, _)| seen.insert((*si, (pose.at[0] / cell).floor() as i64, (pose.at[1] / cell).floor() as i64, pose.rotation as i64)));
        cands
    };
    // The full state for one candidate.
    let materialize = |states: &[State], fp: &Footprint, eval: &Eval, (si, pose, option, est): Cand| -> State {
        let placed = fp.placed(pose);
        let mut options = eval(&states[si], &placed);
        options.sort_by(|a, b| a.1.score.total_cmp(&b.1.score));
        let (segs, _) = options.swap_remove(option);
        let mut s = states[si].clone();
        s.scene.commit(&placed, &segs);
        s.poses.push(pose);
        s.placed.push(placed);
        s.est = est;
        s
    };
    let step = |states: &[State], fp: &Footprint, k: usize, tried: &mut usize, legal: &mut usize, eval: &Eval| -> Vec<State> {
        let mut cands = candidates(states, fp, tried, legal, eval);
        cands.truncate(k);
        cands.into_iter().map(|c| materialize(states, fp, eval, c)).collect()
    };
    let root = State { poses: vec![], placed: vec![], scene: base.clone(), est: Estimate::default() };

    // Step 1: CIN, scored on the hot loop.
    let beam = step(&[root], &fps[0], p.beam, &mut tried[0], &mut legal[0], &|_, placed| {
        let (Some(cv), Some(cg)) = (placed.pad(&n_vin), placed.pad(&n_gnd)) else { return vec![] };
        let (cv, cg) = (cv.centre, cg.centre);
        let hot = geom::loop_area(&[vin.centre, vin.escape, cv, cg, pgnd.escape, pgnd.centre], 60);
        let est = Estimate { hot_loop_area_mm2: hot, score: p.w_hot * hot, ..Default::default() };
        let options: Vec<(Vec<Seg>, Estimate)> =
            p.power_widths.iter().map(|&w| (run(&vin, cv, w, p.neck).into_iter().chain(run(&pgnd, cg, w, p.neck)).collect(), est)).collect();
        // The switch pin sits between VIN and PGND on many regulators: keep only poses that
        // leave it a way out, a narrowest-width run to the edge of the search band.
        let escapable = |segs: &[Seg]| {
            let mut sc = base.clone();
            sc.commit(placed, segs);
            (-6..=6).any(|k| {
                let t = (k as f64 * 12.0).to_radians();
                let d = geom::rotate(out_dir, t.to_degrees());
                let to = geom::add(sw.escape, geom::scale(d, p.reach));
                run(&sw, to, wmin, p.neck).iter().all(|s| sc.seg_ok(s, clr))
            })
        };
        options.into_iter().filter(|(segs, _)| escapable(segs)).collect()
    });

    // Step 2: the inductor, scored on the switch node. The switch run may go straight, or
    // straight out from the pin first and then turn (as LX runs between CIN's pads on the RP2350
    // reference), every width; the step keeps the best-scoring option that fits.
    let beam = step(&beam, &fps[1], p.beam_inductor, &mut tried[1], &mut legal[1], &|st, placed| {
        let Some(ls) = placed.pad(&n_sw) else { return vec![] };
        let cg = st.placed[0].pad(&n_gnd).map_or(pgnd.escape, |q| q.centre);
        let lo = placed.pad(&n_out).map_or(ls.centre, |q| q.centre);
        let mut shapes: Vec<Vec<Seg>> = Vec::new();
        for &w in &p.power_widths {
            shapes.push(run(&sw, ls.centre, w, p.neck));
            let mut d = 0.5;
            while d <= p.reach {
                let corner = geom::add(sw.escape, geom::scale(out_dir, d));
                if geom::dist(corner, ls.centre) > 0.2 {
                    let mut segs = run(&sw, corner, w, p.neck);
                    segs.push(Seg { net: n_sw.clone(), a: corner, b: ls.centre, w });
                    shapes.push(segs);
                    // Or jog sideways at the corner and go on straight out into the pad, entering
                    // it from the pin's side (as the RP2350 reference's LX copper does).
                    let jog = geom::add(corner, geom::scale(along_dir, geom::dot(geom::sub(ls.centre, corner), along_dir)));
                    if geom::dist(jog, corner) > 0.05 && geom::dist(jog, ls.centre) > 0.05 {
                        let mut segs = run(&sw, corner, w, p.neck);
                        segs.push(Seg { net: n_sw.clone(), a: corner, b: jog, w });
                        segs.push(Seg { net: n_sw.clone(), a: jog, b: ls.centre, w });
                        shapes.push(segs);
                    }
                }
                d += 0.5;
            }
        }
        shapes
            .into_iter()
            .map(|segs| {
                let copper: f64 = segs.iter().map(|s| geom::dist(s.a, s.b) * s.w).sum();
                let sw_area = sw_ic_area + pad_area(ls) + geom::dist(sw.centre, sw.escape) * sw.width + copper;
                let mut est = st.est;
                est.switch_node_area_mm2 = sw_area;
                // Lookahead: the output loop as if COUT sat on the way from the inductor's output
                // pad back to CIN's ground, so step 2 does not trade it away for switch copper.
                let mut lp = path_points(&sw, &segs);
                lp.extend([lo, cg, pgnd.escape, pgnd.centre]);
                let out_guess = geom::loop_area(&lp, 60);
                est.score = p.w_hot * est.hot_loop_area_mm2 + p.w_sw * sw_area + p.w_out * out_guess;
                (segs, est)
            })
            .collect()
    });

    // Step 3: COUT, scored on the output loop. The feedback run is routed afterwards, best
    // candidates first, because it may need a bend: straight, or one corner from a grid.
    let cout_eval = |st: &State, placed: &Placed| -> Vec<(Vec<Seg>, Estimate)> {
        let cg = st.placed[0].pad(&n_gnd).unwrap().centre;
        let l = &st.placed[1];
        let lo = l.pad(&n_out).unwrap();
        let (Some(co), Some(cg2)) = (placed.pad(&n_out), placed.pad(&n_gnd)) else { return vec![] };
        let (co, cg2) = (co.centre, cg2.centre);
        let sw_segs: Vec<Seg> = st.scene.segs.iter().filter(|s| s.net == n_sw && s.a != sw.centre).cloned().collect();
        let mut lp = path_points(&sw, &sw_segs);
        lp.extend([lo.centre, co, cg2, cg, pgnd.escape, pgnd.centre]);
        let out = geom::loop_area(&lp, 60);
        let mut est = st.est;
        est.output_loop_area_mm2 = out;
        est.score = p.w_hot * est.hot_loop_area_mm2 + p.w_sw * est.switch_node_area_mm2 + p.w_out * out;
        p.power_widths
            .iter()
            .map(|&w| (connection(&n_out, &[lo.centre, co], &[w]).into_iter().chain(connection(&n_gnd, &[cg2, cg], &[w])).collect(), est))
            .collect()
    };
    let out_cands = candidates(&beam, &fps[2], &mut tried[2], &mut legal[2], &cout_eval);
    let mut corners: Vec<P> = Vec::new();
    for i in -8..=8 {
        for j in 0..=12 {
            corners.push(geom::add(geom::add(fb.escape, geom::scale(along_dir, i as f64 * 0.15)), geom::scale(out_dir, j as f64 * 0.2)));
        }
    }
    let mut done: Vec<State> = Vec::new();
    // States are built lazily: the feedback run decides, so walk the whole list if needed.
    for c in out_cands.into_iter().take(p.fb_attempts) {
        if done.len() >= 8 {
            break;
        }
        let st = materialize(&beam, &fps[2], &cout_eval, c);
        let co = st.placed[2].pad(&n_out).unwrap().centre;
        let ls = st.placed[1].pad(&n_sw).unwrap();
        let sw_copper: Vec<Vec<P>> = std::iter::once(ls.poly.clone())
            .chain(st.scene.segs.iter().filter(|s| s.net == n_sw && s.a != sw.centre).map(|s| geom::capsule(s.a, s.b, s.w)))
            .collect();
        let mut routes: Vec<Vec<P>> = vec![vec![fb.escape, co]];
        routes.extend(corners.iter().map(|&q| vec![fb.escape, q, co]));
        routes.sort_by(|a, b| path_len(a).total_cmp(&path_len(b)));
        let found = routes.into_iter().find_map(|r| {
            let mut line = vec![fb.centre];
            line.extend(&r);
            let fbc = sw_copper.iter().map(|s| geom::polyline_poly_dist(&line, s)).fold(f64::INFINITY, f64::min);
            if fbc < p.fb_clearance_min {
                return None;
            }
            let segs = connection(&n_out, &r, &vec![p.fb_width; r.len() - 1]);
            segs.iter().all(|s| st.scene.seg_ok(s, clr)).then_some((segs, fbc))
        });
        if let Some((segs, fbc)) = found {
            let mut s = st;
            s.scene.segs.extend(segs);
            s.est.feedback_clearance_mm = fbc;
            done.push(s);
        }
    }
    let best = done
        .first()
        .cloned()
        .ok_or_else(|| format!("no legal arrangement (legal poses per step {legal:?} of {tried:?}): widen the search (reach, along) or loosen the clearance"))?;

    let mut keepouts = Vec::new();
    if let Some(layer) = &p.keepout_layer {
        let ls = best.placed[1].pad(&n_sw).unwrap();
        let mut pts: Vec<P> = Vec::new();
        let mut shapes: Vec<Vec<P>> = vec![ls.poly.clone(), sw.poly.clone()];
        shapes.extend(best.scene.segs.iter().filter(|s| s.net == n_sw).map(|s| geom::capsule(s.a, s.b, s.w)));
        for q in shapes.iter().flatten() {
            pts.extend(geom::circle(*q, p.keepout_margin, 8));
        }
        keepouts.push(LKeepout {
            layer: layer.clone(),
            polygon: geom::hull(&pts),
            reason: "No copper under the switch node on the next layer (RP2350 datasheet §6.3.8 p.412, Figure 27)".into(),
        });
    }
    // Ground vias beside CIN's ground pad, as close as they fit and clear of the keepout (a
    // via there could not reach the plane it is for).
    let mut scene = best.scene.clone();
    let cg = best.placed[0].pad(&n_gnd).unwrap().clone();
    let mut vias: Vec<LVia> = Vec::new();
    'rings: for k in 0..12 {
        let r = 0.5 + 0.1 * k as f64;
        for a in 0..24 {
            if vias.len() >= p.gnd_vias {
                break 'rings;
            }
            let t = a as f64 / 24.0 * std::f64::consts::TAU;
            let at = [cg.centre[0] + r * t.cos(), cg.centre[1] + r * t.sin()];
            let disc = Shape::new(&n_gnd, geom::circle(at, p.via_size / 2.0, 16));
            let link = Seg { net: n_gnd.clone(), a: cg.centre, b: at, w: pw };
            let spaced = vias.iter().all(|v| geom::dist(v.at, at) >= p.via_size + clr.get(&n_gnd, &n_gnd));
            let clear_of_keepout = keepouts.iter().all(|k| geom::point_poly_dist(at, &k.polygon) >= p.via_size / 2.0);
            let clear_of_bodies = scene.bodies.iter().all(|b| geom::point_poly_dist(at, &b.hull) >= p.via_size / 2.0);
            if spaced && clear_of_keepout && clear_of_bodies && scene.shape_ok(&disc, clr) && scene.seg_ok(&link, clr) {
                scene.shapes.push(disc);
                scene.segs.push(link);
                vias.push(LVia { at, size: p.via_size, drill: p.via_drill, net: "gnd".into() });
            }
        }
    }

    // The layout, by role.
    let role_of = |net: &str| -> String {
        if net == n_vin {
            "vin"
        } else if net == n_sw {
            "sw"
        } else if net == n_gnd {
            "gnd"
        } else {
            "out"
        }
        .to_string()
    };
    let mut parts = BTreeMap::new();
    for (role, pose) in ["cin", "inductor", "cout"].iter().zip(&best.poses) {
        parts.insert(role.to_string(), *pose);
    }
    let tracks: Vec<LTrack> = scene
        .segs
        .iter()
        .map(|s| LTrack { a: s.a, b: s.b, width: s.w, layer: p.layer.clone(), net: role_of(&s.net) })
        .collect();
    let layout = Layout { block_type: "buck".into(), anchor_footprint: ic.footprint.clone(), parts, tracks, vias, keepouts };
    Ok(Generated { layout, estimate: best.est, tried, legal, runners_up: done.iter().skip(1).map(|s| s.est).collect() })
}

