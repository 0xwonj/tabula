//! Trace builder orchestrator (M12 entry).
//!
//! Converts `BatchWitness` into canonical chip traces via one entrypoint,
//! enforcing a shared E-Trace/contract boundary.

mod collectors;
mod lowering;
mod memory;
mod orchestration;
mod smt;
mod types;
mod validation;

pub use lowering::lower_execution_records;
pub use memory::build_trace_bundle;
pub use orchestration::{build_all_trace_bundle, build_all_trace_bundle_from_execution_result};
pub use types::{AllTraceBundle, ProofTraceBundle};
pub use validation::debug_validate_all_trace_bundle;
