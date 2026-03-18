//! Witness pipeline and trace builder for the Tabula proof system.
//!
//! Transforms executor output (`BatchWitness`) into canonical chip traces
//! (`TraceMap`) for STARK proving.

pub mod trace;
pub mod witness;

// Convenience re-exports.
pub use trace::builtin::{AllTraceInputs, BuiltinTraceBuilder, BuiltinWitnessInputs};
pub use witness::{
    AccessPattern, AccessRow, BatchWitness, ColumnWitness, InitRow, KeyRoute, LiteralCell,
    ProgramInfo, TemplateId, WitnessGenerator, route_keys,
};
