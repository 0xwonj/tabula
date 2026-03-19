//! Witness pipeline and trace builder for the Tabula proof system.
//!
//! Transforms executor output and runtime-owned proof inputs into canonical
//! chip traces (`TraceMap`) for STARK proving.

pub mod prepare;
pub mod trace;
mod witness;

// Convenience re-exports.
pub use prepare::{ExecutionInputPreparer, PreparedExecutionInputs};
pub use trace::builtin::{
    AllTraceInputs, BuiltinTraceBuilder, BuiltinTraceContext, BuiltinWitnessInputs,
};
pub use witness::{
    AccessRow, InitRow, LiteralCell, ProgramInfo, TemplateId, proof_column_commitment,
};
