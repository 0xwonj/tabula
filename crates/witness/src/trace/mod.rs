//! Trace builder orchestrator (M12 entry).
//!
//! Converts `BatchWitness` into canonical chip traces via one entrypoint,
//! enforcing a shared E-Trace/contract boundary.

mod builder;
mod collectors;
mod lowering;
mod memory;
mod orchestration;
mod smt;
mod validation;

// Re-export core trace types from tabula-stark.
pub use tabula_stark::trace::{
    TraceContributor, TraceEntry, TraceGenerator, TraceMap, TracePhase, WitnessKey, WitnessStore,
    witness_labels,
};

pub use builder::{AllTraceInputs, TraceBuilder, build_trace_map};
pub use lowering::{LoweringOutput, lower_execution_records, lower_program_batch};
pub use smt::build_smt_paths;
