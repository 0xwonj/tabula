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
mod types;
mod validation;

pub use builder::{
    AllTraceInputs, TraceBuilder, build_all_from_program, build_all_trace_bundle,
    build_all_trace_bundle_from_execution_result, build_trace_bundle,
    debug_validate_all_trace_bundle,
};
pub use lowering::{LoweringOutput, lower_execution_records, lower_program_batch};
pub use smt::build_smt_paths;
pub use types::{AllTraceBundle, ProofTraceBundle};
