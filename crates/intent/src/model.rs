//! The intent file: what a board needs that KiCad cannot express, keyed by KiCad net names and
//! reference designators. KiCad stays the source of truth for geometry.
//!
//! Every constrained value is an [`Attr`]: a value, a strength, and a provenance.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The schema version this crate reads and writes.
pub const SCHEMA_VERSION: &str = "0.1";

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct IntentFile {
    /// Schema version, `"0.1"`.
    pub schema_version: String,
    pub board: BoardInfo,
    /// Documents that provenances cite by id.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub sources: BTreeMap<String, Source>,
    /// Per-net intent, keyed by the KiCad net name exactly as the netlist spells it
    /// (`/GPIO0`, `+3V3`, `GND`).
    #[serde(default)]
    pub nets: BTreeMap<String, NetIntent>,
    /// Blocks keyed by a board-unique id.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub blocks: BTreeMap<String, Block>,
    /// The root of the domain tree: the board itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domains: Option<Domain>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub regions: Vec<Region>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub protection_paths: Vec<ProtectionPath>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub energy_sources: Vec<EnergySource>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BoardInfo {
    pub name: String,
    /// The KiCad project this file is the sidecar of, relative to this file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kicad_project: Option<String>,
    /// Copper layer count, when decided.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layers: Option<u32>,
    /// Exact package versions this board was composed from, like a lockfile: `rp2350 = "0.1.0"`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub packages: BTreeMap<String, String>,
}

/// A cited document.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub title: String,
    /// A URL, or a path relative to this file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    /// Document revision, when it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    /// SHA-256 of the file as read, so a changed document is noticed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

// ---------------------------------------------------------------------------------------------
// Attributes

/// A constrained value: what it is, how binding, and where it came from.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Attr<T> {
    pub value: T,
    pub strength: Strength,
    /// For `soft` only: how much this weighs against other soft goals, in (0, 1].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weight: Option<f64>,
    /// Required for `hard`; the validator refuses a hard constraint without one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<Provenance>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Strength {
    Hard,
    Soft,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Provenance {
    /// A key of `sources`.
    pub source: String,
    /// Page, section, table, or video timestamp.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locator: Option<String>,
    pub kind: ClaimKind,
    /// When the claim was made or checked, ISO 8601.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    /// The reasoning in a sentence, when the citation alone does not carry it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ClaimKind {
    /// Stated by a manufacturer: datasheet, design guide, reference board.
    Datasheet,
    /// Stated by a person with expertise: a book, a talk, a reviewer.
    Expert,
    /// Shown by simulation in this system's verifier.
    Simulated,
    /// The board's own requirement, decided by its designer (a budget, a connector rating).
    Requirement,
    /// Filled in from a class preset; never satisfies a hard constraint.
    Preset,
}

// ---------------------------------------------------------------------------------------------
// Nets

/// Intent for one net. Categories (`class`) are presets over these attributes.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NetIntent {
    /// A net class preset name (`POWER`, `SWITCH`, `DIFF_HS`, ...). It decides which attributes
    /// are required and supplies soft defaults for the rest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub class: Option<String>,
    /// How much noise the net emits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aggressor: Option<Attr<Noise>>,
    /// How sensitive the net is to coupled noise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub victim: Option<Attr<Noise>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bandwidth: Option<Attr<Bandwidth>>,
    /// Peak working voltage, for creepage and clearance.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voltage: Option<Attr<Voltage>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current: Option<Attr<Current>>,
    /// Current the copper must survive until protection opens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fault_current: Option<Attr<FaultCurrent>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub impedance: Option<Attr<Impedance>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topology: Option<Attr<Topology>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub match_group: Option<Attr<MatchGroup>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub safety: Option<Attr<Safety>>,
    /// Longest allowed routed length, pad to pad.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_length: Option<Attr<Length>>,
    /// Free text for people; no tool reads it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    None,
    Weak,
    Moderate,
    Strong,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Noise {
    pub level: Level,
    /// 10–90 % edge time, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edge_ns: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Bandwidth {
    pub kind: BandwidthKind,
    /// Highest frequency of interest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_hz: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BandwidthKind {
    Dc,
    SlowPrecise,
    Digital,
    Wideband,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Voltage {
    /// Peak working voltage against the domain's reference ground.
    pub peak_v: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Current {
    pub continuous_a: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub peak_a: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FaultCurrent {
    pub amps: f64,
    /// How long until protection opens.
    pub duration_ms: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Impedance {
    pub ohms: f64,
    pub tolerance_pct: f64,
    pub mode: ImpedanceMode,
    /// For `differential`: the other net of the pair.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pair: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ImpedanceMode {
    SingleEnded,
    Differential,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Topology {
    PointToPoint,
    DaisyChain,
    Star,
    FlyBy,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MatchGroup {
    pub group: String,
    /// Allowed spread within the group, as length or as delay (one of the two).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tolerance_mm: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tolerance_ps: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Safety {
    None,
    Functional,
    PatientConnected,
    HighVoltage,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Length {
    pub mm: f64,
}

// ---------------------------------------------------------------------------------------------
// Blocks

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Block {
    /// A block type from the block library (`buck`, `ldo`, `crystal`, ...).
    #[serde(rename = "type")]
    pub kind: String,
    /// The package this block comes from, `name@version`, when composed from one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package: Option<String>,
    /// Member parts by reference designator.
    pub parts: Vec<String>,
    /// Who plays each role the block type names: pads as `REF.PIN`, parts by reference. A
    /// buck names `vin`, `sw`, `pgnd`, `fb` (the IC's pads) and `cin`, `cout`, `inductor`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub roles: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement_goal: Option<PlacementGoal>,
    /// External interface, keyed by port name.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub ports: BTreeMap<String, Port>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// An objective plus constraints, in the metric vocabulary the generator and verifier score.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlacementGoal {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub objectives: Vec<Objective>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub constraints: Vec<Attr<MetricBound>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Objective {
    pub metric: Metric,
    pub sense: Sense,
    /// Relative weight among this goal's objectives.
    #[serde(default = "one")]
    pub weight: f64,
    /// The nets the metric is measured on, when it is per net.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nets: Vec<String>,
}

fn one() -> f64 {
    1.0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Sense {
    Minimize,
    Maximize,
}

/// `metric op limit`, e.g. `feedback_clearance_mm >= 1.0` between `nets`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MetricBound {
    pub metric: Metric,
    pub op: BoundOp,
    pub limit: f64,
    /// The nets the metric is measured on; for a clearance, the two sides.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nets: Vec<String>,
    /// Parts the metric is measured on, when it is about placement.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parts: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum BoundOp {
    #[serde(rename = "<=")]
    Le,
    #[serde(rename = ">=")]
    Ge,
}

/// The metric vocabulary. A metric is here only once a generator or verifier check computes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Metric {
    /// Area enclosed by a switching block's high-di/dt loop, mm².
    HotLoopAreaMm2,
    /// Area enclosed by a buck's output loop (switch, inductor, output capacitor), mm².
    OutputLoopAreaMm2,
    /// Other nets' copper on the layer under a switch node's outer copper, mm².
    SwitchNodeUnderlapMm2,
    /// Copper area of a switch node, mm².
    SwitchNodeAreaMm2,
    /// Distance from a feedback net to switch-node copper, mm.
    FeedbackClearanceMm,
    /// Distance between two nets' copper, mm.
    ClearanceMm,
    /// Distance between two parts' nearest pads, mm.
    PartDistanceMm,
    /// Routed length of a net, mm.
    TraceLengthMm,
    /// Vias on a net.
    ViaCount,
    /// Track of a net with no ground fill on a neighbouring copper layer (a broken return path), mm.
    UnreferencedLengthMm,
    /// Copper path from a power pin to its nearest decoupling capacitor, mm.
    DecouplingPathMm,
}

/// A block's external interface.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Port {
    pub kind: PortKind,
    /// The board nets the port binds to.
    pub nets: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<Direction>,
    /// Supply ports: rail voltage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voltage_v: Option<f64>,
    /// Supply ports: current drawn (input) or available (output).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_a: Option<f64>,
    /// Supply ports: ripple and noise the port tolerates (input) or produces (output), mV p-p.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub noise_mvpp: Option<f64>,
    /// Bus ports: protocol (`usb2-fs`, `qspi`, `swd`, `ws2812`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protocol: Option<String>,
    /// Signal ports: the net class preset of the nets it carries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub class: Option<String>,
    /// Differential ports: target impedance.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub impedance_ohm: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PortKind {
    Supply,
    Ground,
    Signal,
    Bus,
    DiffPair,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    In,
    Out,
    Bidirectional,
}

// ---------------------------------------------------------------------------------------------
// Domains

/// A node of the domain tree: board, then isolation islands, then ground zones, then blocks.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Domain {
    /// Unique across the tree.
    pub name: String,
    pub kind: DomainKind,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub supplies: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference_ground: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub barriers: Vec<Barrier>,
    /// Whether the domain can be powered off while its siblings run.
    #[serde(default)]
    pub switchable: bool,
    /// Block ids that live directly in this domain.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocks: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<Domain>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DomainKind {
    Board,
    IsolationIsland,
    GroundZone,
    Group,
}

/// An isolation barrier to a sibling domain.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Barrier {
    /// The sibling domain's name.
    pub to: String,
    pub creepage_mm: Attr<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clearance_mm: Option<Attr<f64>>,
}

// ---------------------------------------------------------------------------------------------
// Regions

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Region {
    pub name: String,
    pub kind: RegionKind,
    /// Board coordinates in mm, KiCad's frame; absent for a floorplanner-assigned soft region.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub polygon: Option<Vec<[f64; 2]>>,
    /// Places the region relative to a part instead of the board: polygon coordinates are then
    /// in that part's frame (its centre and rotation), so the region moves with the part.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<String>,
    /// For a soft region: the block it holds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block: Option<String>,
    /// Copper layers by KiCad name (`F.Cu`, `In1.Cu`); empty means all.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub layers: Vec<String>,
    pub rules: Vec<RegionRule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<Provenance>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RegionKind {
    Polygon,
    Soft,
    IsolationBarrier,
    ProtectedZone,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RegionRule {
    /// The nets the rule applies to: a class, one net, or all nets.
    pub applies_to: Selector,
    /// The rule is about these nets' vias, not their tracks.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub vias: bool,
    pub mode: RuleMode,
    /// For `prefer` and `avoid`: in (0, 1].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weight: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Selector {
    Class(String),
    Net(String),
    AllNets,
}

/// What a region rule does on the region's layers. The router's meanings, so the constraint
/// compiler passes them through unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RuleMode {
    /// The nets' copper may not leave the region (on these layers).
    Must,
    /// Copper outside the region costs extra, by `weight`.
    Prefer,
    /// Copper inside the region costs extra, by `weight`.
    Avoid,
    /// No copper of these nets inside the region.
    Never,
}

// ---------------------------------------------------------------------------------------------
// Protection

/// An ordered pad sequence for a connector net: energy reaches each pad in turn, protection
/// first, with no branch before it.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProtectionPath {
    pub net: String,
    /// Pads as `REF.PIN`, connector first. The protection device's pad comes before any branch.
    pub sequence: Vec<String>,
    /// No stub may leave the path before the last pad of `sequence`.
    #[serde(default = "yes")]
    pub no_stub: bool,
    pub provenance: Provenance,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EnergySource {
    pub net: String,
    pub kind: EnergyKind,
    /// The `protected_zone` region bounding copper upstream of the first disconnect.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protected_zone: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EnergyKind {
    Battery,
    SupplyInput,
    CapacitorBank,
}
