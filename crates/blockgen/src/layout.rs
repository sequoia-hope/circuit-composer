//! A generated sub-layout, in its anchor part's frame so it can be transplanted onto any board
//! that has the same anchor footprint: parts and copper by role, not by reference or net name.

use std::collections::BTreeMap;

use pcb::geom::{self, P};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Layout {
    pub block_type: String,
    /// The anchor footprint's library id; a board must use the same one.
    pub anchor_footprint: String,
    /// Part poses by role, in the anchor's frame: `at` in mm, `rotation` in degrees added to the
    /// anchor's own.
    pub parts: BTreeMap<String, Pose>,
    pub tracks: Vec<LTrack>,
    pub vias: Vec<LVia>,
    /// Copper keepouts (rule areas: no pour, tracks or vias).
    pub keepouts: Vec<LKeepout>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Pose {
    pub at: P,
    pub rotation: f64,
}

/// Net roles: `vin`, `sw`, `gnd`, `out`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LTrack {
    pub a: P,
    pub b: P,
    pub width: f64,
    pub layer: String,
    pub net: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LVia {
    pub at: P,
    pub size: f64,
    pub drill: f64,
    pub net: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LKeepout {
    pub layer: String,
    pub polygon: Vec<P>,
    /// Why, with its citation.
    pub reason: String,
}

/// The anchor's frame: board point ↔ frame point.
#[derive(Debug, Clone, Copy)]
pub struct Frame {
    pub at: P,
    pub rotation: f64,
}

impl Frame {
    pub fn to_local(&self, p: P) -> P {
        geom::rotate(geom::sub(p, self.at), -self.rotation)
    }

    pub fn to_board(&self, p: P) -> P {
        geom::add(self.at, geom::rotate(p, self.rotation))
    }
}
