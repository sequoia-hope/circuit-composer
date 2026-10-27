//! Block generators. Critical blocks are a small search, not an open routing problem: each
//! generator enumerates arrangements, scores them on the verifier's own metrics, and emits a
//! locked sub-layout in its anchor part's frame.

pub mod buck;
pub mod layout;
