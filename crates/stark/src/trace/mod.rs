//! Core trace types for chip-agnostic proof pipelines.
//!
//! Defines [`TraceMap`], [`TraceContributor`], [`TraceGenerator`], and supporting types.
//! The actual trace building logic lives in downstream crates.

pub mod contributor;
pub mod dyn_chip;
pub mod generator;
pub mod trace_map;

pub use contributor::{TraceContributor, TracePhase, WitnessKey, WitnessStore, witness_labels};
pub use dyn_chip::DynChip;
pub use generator::TraceGenerator;
pub use trace_map::{TraceEntry, TraceMap};
