//! Core trace types for chip-agnostic proof pipelines.
//!
//! Defines [`TraceMap`], [`TraceContributor`], [`TraceGenerator`], and supporting types.
//! The actual trace building logic lives in downstream crates.
//!
//! The [`column_commitment`] module defines the pluggable per-column commitment
//! interface and proof plan types used by the shard architecture.

pub mod column_commitment;
pub mod contributor;
pub mod dyn_chip;
pub mod generator;
pub mod trace_map;

pub use column_commitment::{BusConsumer, ColumnCommitment, ColumnPlan, EncodingWidth, ProofPlan};
pub use contributor::{TraceContributor, TracePhase, WitnessKey, WitnessStore, witness_labels};
pub use dyn_chip::DynChip;
pub use generator::TraceGenerator;
pub use trace_map::{TraceEntry, TraceMap};
