//! Parametric outlines: a parameter schema (what the app shows as sliders) and a function from
//! parameters to closed polygons in board millimetres.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::{from_mm, hull, inflate, intersect, round, to_mm, union, P};

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Param {
    pub name: &'static str,
    pub default: f64,
    pub min: f64,
    pub max: f64,
    pub step: f64,
    pub help: &'static str,
}

#[derive(Serialize)]
pub struct Generator {
    pub name: &'static str,
    pub title: &'static str,
    pub help: &'static str,
    pub params: &'static [Param],
    #[serde(skip)]
    pub build: fn(&BTreeMap<String, f64>) -> Result<Vec<Vec<P>>, String>,
}

pub static GENERATORS: &[Generator] = &[BOLT];

pub fn find(name: &str) -> Option<&'static Generator> {
    GENERATORS.iter().find(|g| g.name == name)
}

/// Every parameter, given or default.
pub fn with_defaults(g: &Generator, given: &BTreeMap<String, f64>) -> BTreeMap<String, f64> {
    g.params.iter().map(|p| (p.name.to_string(), given.get(p.name).copied().unwrap_or(p.default))).collect()
}

const fn param(name: &'static str, default: f64, min: f64, max: f64, step: f64, help: &'static str) -> Param {
    Param { name, default, min, max, step, help }
}

/// The lightning-bolt earring's outline (boards/bolt-earring/scripts/geometry.py): a band along
/// a leaning axis whose centreline steps sideways at the middle (an odd tanh step) and whose
/// half-width tapers to a point at each end, so a half turn about its centre leaves it unchanged;
/// a USB-C tab grafted onto the top point, flaring into the body; every corner rounded.
pub const BOLT: Generator = Generator {
    name: "bolt",
    title: "Lightning bolt",
    help: "A fat zigzag band with pointed ends and a connector tab at the top (the bolt earring).",
    params: &[
        param("body_len", 68.0, 20.0, 120.0, 0.5, "end point to end point, along the axis"),
        param("axis_deg", 14.0, -45.0, 45.0, 0.5, "lean of the axis from vertical, top to the right"),
        param("half_w", 10.2, 2.0, 25.0, 0.1, "band half-width in the middle"),
        param("taper", 21.0, 1.0, 60.0, 0.5, "length over which each end narrows to its point"),
        param("taper_exp", 0.85, 0.2, 2.5, 0.05, "taper profile: below 1 fuller, above 1 a sharper point"),
        param("centre_x", 0.0, -50.0, 50.0, 0.5, "x of the body's centre of symmetry"),
        param("centre_y", 40.0, 10.0, 80.0, 0.5, "y of the body's centre of symmetry"),
        param("step", 3.8, -15.0, 15.0, 0.1, "the centreline's sideways step either side of the centre"),
        param("step_sharp", 1.1, 0.05, 20.0, 0.05, "length of the step's rise: smaller is a crisper notch"),
        param("tab", 5.3, 0.0, 12.0, 0.05, "half-width of the connector tab (0: no tab)"),
        param("tab_len", 7.0, 0.0, 20.0, 0.25, "the tab's straight part, from the top edge down"),
        param("flare", 9.0, 0.0, 30.0, 0.25, "how much of the body's top end the tab's flare takes in"),
        param("corner_r", 0.6, 0.0, 5.0, 0.05, "corner rounding radius"),
    ],
    build: bolt,
};

fn bolt(p: &BTreeMap<String, f64>) -> Result<Vec<Vec<P>>, String> {
    let v = |k: &str| p[k];
    let a = v("axis_deg").to_radians();
    let u = (a.sin(), -a.cos()); // along the axis, toward the top end
    let n = (a.cos(), a.sin()); // across it, to the right
    let (cx, cy) = (v("centre_x"), v("centre_y"));
    let (len, half_w, taper, step, sharp) = (v("body_len"), v("half_w"), v("taper"), v("step"), v("step_sharp"));
    if len <= 0.0 || half_w <= 0.0 || taper <= 0.0 || sharp <= 0.0 {
        return Err("body_len, half_w, taper and step_sharp must be positive".into());
    }
    let steps = 400;
    let (mut left, mut right) = (Vec::new(), Vec::new());
    for i in 0..=steps {
        let t = -len / 2.0 + len * i as f64 / steps as f64;
        let off = step * (t / sharp).tanh();
        let end = len / 2.0 - t.abs();
        let w = half_w * (end / taper).min(1.0).max(0.0).powf(v("taper_exp"));
        let (px, py) = (cx + t * u.0 + off * n.0, cy + t * u.1 + off * n.1);
        left.push([px - w * n.0, py - w * n.1]);
        right.push([px + w * n.0, py + w * n.1]);
    }
    left.reverse();
    right.extend(left);
    // buffer(0): the band may overlap itself where a sharp step folds it.
    let r = v("corner_r");
    let mut body = union(&from_mm(&[right]), &Vec::new());
    if r > 0.0 {
        body = inflate(&inflate(&body, -r), r);
    }
    let body_mm = to_mm(&body);
    let top = body_mm.iter().flatten().copied().min_by(|a, b| a[1].total_cmp(&b[1])).ok_or("the body vanished")?;
    let mut g = body.clone();
    let tab = v("tab");
    if tab > 0.0 {
        let tab_box = vec![[top[0] - tab, 0.0], [top[0] + tab, 0.0], [top[0] + tab, v("tab_len")], [top[0] - tab, v("tab_len")]];
        let mut pts = tab_box.clone();
        if v("flare") > 0.0 {
            let ring: Vec<P> = (0..96).map(|i| {
                let t = 2.0 * std::f64::consts::PI * i as f64 / 96.0;
                [top[0] + v("flare") * t.cos(), top[1] + v("flare") * t.sin()]
            }).collect();
            pts.extend(to_mm(&intersect(&body, &from_mm(&[ring]))).into_iter().flatten());
        }
        g = union(&union(&g, &from_mm(&[tab_box])), &from_mm(&[hull(pts)]));
    }
    if r > 0.0 {
        g = round(&g, r);
    }
    Ok(to_mm(&g))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{evaluate, Feature, Geom, Op, ShapeDoc};

    #[test]
    fn the_bolt_is_the_bolt_earrings_outline() {
        // boards/bolt-earring/scripts/geometry.py with its defaults, through shapely: bounds
        // (-16.713, 0.0, 16.981, 71.759), area 1118.066 mm². Within 0.05 mm: the tab is centred
        // on the body's lowest vertex, which on a rounded tip depends on where each library puts
        // its arc's chords (shapely 16 a quarter, Clipper by tolerance).
        let doc = ShapeDoc {
            features: vec![Feature { name: "bolt".into(), op: Op::Add, geom: Geom::Generator { name: "bolt".into(), params: BTreeMap::new() }, hidden: false }],
            ..Default::default()
        };
        let s = evaluate(&doc);
        assert!(s.warnings.is_empty(), "{:?}", s.warnings);
        assert_eq!(s.outline.len(), 1);
        let want = [-16.713, 0.0, 16.981, 71.759];
        for k in 0..4 {
            assert!((s.bbox[k] - want[k]).abs() < 0.05, "bbox {:?} vs {want:?}", s.bbox);
        }
        assert!((s.area_mm2 - 1118.066).abs() < 1.0, "area {}", s.area_mm2);
    }

    #[test]
    fn every_parameter_is_in_range_by_default() {
        for g in GENERATORS {
            for p in g.params {
                assert!(p.min <= p.default && p.default <= p.max, "{}.{}", g.name, p.name);
            }
            assert!((g.build)(&with_defaults(g, &BTreeMap::new())).is_ok(), "{}", g.name);
        }
    }
}
