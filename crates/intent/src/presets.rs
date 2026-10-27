//! Net class presets and the block-type library list.
//!
//! A class is a preset over net attributes, never the other way round: it names the attributes a
//! net of that class must state explicitly, and supplies soft defaults for the ones it may leave
//! out. A default never satisfies a requirement, so a class lists as required only what it has
//! no sensible default for.

use crate::model::{Attr, ClaimKind, Level, NetIntent, Noise, Provenance, Strength};

/// The attributes a class can require, named as in the intent file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Aggressor,
    Victim,
    Bandwidth,
    Voltage,
    Current,
    FaultCurrent,
    Impedance,
    Topology,
    MatchGroup,
    Safety,
    MaxLength,
}

impl Field {
    pub fn name(self) -> &'static str {
        match self {
            Field::Aggressor => "aggressor",
            Field::Victim => "victim",
            Field::Bandwidth => "bandwidth",
            Field::Voltage => "voltage",
            Field::Current => "current",
            Field::FaultCurrent => "fault_current",
            Field::Impedance => "impedance",
            Field::Topology => "topology",
            Field::MatchGroup => "match_group",
            Field::Safety => "safety",
            Field::MaxLength => "max_length",
        }
    }

    pub fn present(self, n: &NetIntent) -> bool {
        match self {
            Field::Aggressor => n.aggressor.is_some(),
            Field::Victim => n.victim.is_some(),
            Field::Bandwidth => n.bandwidth.is_some(),
            Field::Voltage => n.voltage.is_some(),
            Field::Current => n.current.is_some(),
            Field::FaultCurrent => n.fault_current.is_some(),
            Field::Impedance => n.impedance.is_some(),
            Field::Topology => n.topology.is_some(),
            Field::MatchGroup => n.match_group.is_some(),
            Field::Safety => n.safety.is_some(),
            Field::MaxLength => n.max_length.is_some(),
        }
    }
}

pub struct NetClass {
    pub name: &'static str,
    pub summary: &'static str,
    pub required: &'static [Field],
    /// Soft defaults: (aggressor, victim).
    pub aggressor: Option<Level>,
    pub victim: Option<Level>,
}

use Field::*;


/// The presets. Names follow easy_pcb's phase-3 classes (`references/net-classes.md`), plus
/// `GROUND`.
pub const NET_CLASSES: &[NetClass] = &[
    NetClass { name: "POWER_HI", summary: "high-current rails, > 3 A: pours, via arrays", required: &[Voltage, Current, FaultCurrent], aggressor: Some(Level::Weak), victim: Some(Level::Weak) },
    NetClass { name: "POWER", summary: "logic rails: planes or wide traces", required: &[Voltage, Current], aggressor: Some(Level::Weak), victim: Some(Level::Weak) },
    NetClass { name: "GROUND", summary: "return nets: planes", required: &[Current], aggressor: Some(Level::None), victim: Some(Level::Weak) },
    NetClass { name: "SWITCH_HI", summary: "half-bridge phase nodes: short, wide, minimal loop", required: &[Voltage, Current], aggressor: Some(Level::Strong), victim: Some(Level::None) },
    NetClass { name: "SWITCH", summary: "buck/boost switch nodes: minimal area", required: &[Voltage, Current], aggressor: Some(Level::Strong), victim: Some(Level::None) },
    NetClass { name: "GATE", summary: "gate drive: short, paired with its return", required: &[Voltage, Current], aggressor: Some(Level::Strong), victim: Some(Level::Moderate) },
    NetClass { name: "ANALOG", summary: "sense, references, ADC inputs", required: &[Voltage, Bandwidth], aggressor: Some(Level::None), victim: Some(Level::Strong) },
    NetClass { name: "ANALOG_DIFF", summary: "differential sense, Kelvin pairs", required: &[Voltage, Bandwidth], aggressor: Some(Level::None), victim: Some(Level::Strong) },
    NetClass { name: "CLOCK", summary: "crystals and oscillators: very short, guarded", required: &[Voltage], aggressor: Some(Level::Moderate), victim: Some(Level::Strong) },
    NetClass { name: "DIFF_HS", summary: "USB, Ethernet, LVDS: impedance-controlled pairs", required: &[Voltage, Impedance], aggressor: Some(Level::Moderate), victim: Some(Level::Moderate) },
    NetClass { name: "SE_HS", summary: "fast single-ended: QSPI, SPI > 20 MHz", required: &[Voltage], aggressor: Some(Level::Moderate), victim: Some(Level::Moderate) },
    NetClass { name: "RF", summary: "antennas and RF front ends: 50 ohm, keepouts", required: &[Voltage, Impedance], aggressor: Some(Level::Moderate), victim: Some(Level::Strong) },
    NetClass { name: "HV", summary: "> 50 V or mains: creepage and clearance", required: &[Voltage, Safety], aggressor: Some(Level::Moderate), victim: Some(Level::Weak) },
    NetClass { name: "SIGNAL", summary: "everything else", required: &[Voltage], aggressor: Some(Level::Weak), victim: Some(Level::Weak) },
];

pub fn net_class(name: &str) -> Option<&'static NetClass> {
    NET_CLASSES.iter().find(|c| c.name == name)
}

/// A net with its class's soft defaults filled in wherever it is silent. Explicit values win.
pub fn resolve(net: &NetIntent) -> NetIntent {
    let mut out = net.clone();
    let Some(class) = net.class.as_deref().and_then(net_class) else { return out };
    let preset = |level: Level| Attr {
        value: Noise { level, edge_ns: None },
        strength: Strength::Soft,
        weight: Some(0.5),
        provenance: Some(Provenance {
            source: format!("preset:{}", class.name),
            locator: None,
            kind: ClaimKind::Preset,
            date: None,
            note: None,
        }),
    };
    if out.aggressor.is_none() {
        out.aggressor = class.aggressor.map(preset);
    }
    if out.victim.is_none() {
        out.victim = class.victim.map(preset);
    }
    out
}

pub struct BlockType {
    pub name: &'static str,
    pub summary: &'static str,
    /// Whether a generator exists yet; without one the block is placed from a package layout or
    /// by hand.
    pub generator: bool,
}

/// The block library's types. Generators come milestone by milestone; a type is listed once
/// the schema needs to name it.
pub const BLOCK_TYPES: &[BlockType] = &[
    BlockType { name: "buck", summary: "synchronous or async step-down converter", generator: false },
    BlockType { name: "boost", summary: "step-up converter", generator: false },
    BlockType { name: "ldo", summary: "linear regulator with its capacitors", generator: false },
    BlockType { name: "half_bridge", summary: "two switches, driver and bulk/decoupling", generator: false },
    BlockType { name: "crystal", summary: "crystal, load capacitors and series resistor", generator: false },
    BlockType { name: "connector_protection", summary: "connector with its TVS/ESD/fuse in path order", generator: false },
    BlockType { name: "mcu", summary: "microcontroller with its decoupling", generator: false },
    BlockType { name: "memory", summary: "external memory on a bus (QSPI flash, PSRAM)", generator: false },
    BlockType { name: "level_shifter", summary: "logic-level translation", generator: false },
    BlockType { name: "led_chain", summary: "addressable LEDs with decoupling", generator: false },
    BlockType { name: "indicator", summary: "LED and its resistor", generator: false },
    BlockType { name: "button", summary: "pushbutton and its resistor", generator: false },
    BlockType { name: "connector", summary: "header or connector with no protection", generator: false },
    BlockType { name: "test_point", summary: "test points", generator: false },
];

pub fn block_type(name: &str) -> Option<&'static BlockType> {
    BLOCK_TYPES.iter().find(|b| b.name == name)
}
