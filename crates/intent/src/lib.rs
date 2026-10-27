//! Circuit Composer's intent schema: the sidecar file beside a KiCad project that holds what
//! KiCad cannot express. These types are the one definition; the JSON Schema is generated from
//! them (`ccomp schema`), so the router, the tools and the agent's validator agree.

pub mod import;
pub mod model;
pub mod netlist;
pub mod presets;
pub mod validate;

use std::path::Path;

pub use model::*;

/// Reads an intent file. Unknown fields are refused, so a typo never passes silently.
pub fn load(path: &Path) -> Result<IntentFile, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

/// The JSON Schema of the intent file.
pub fn json_schema() -> serde_json::Value {
    serde_json::to_value(schemars::schema_for!(IntentFile)).expect("schema serialises")
}
