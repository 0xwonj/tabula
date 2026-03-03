//! Trace builder orchestrator (M12 entry).
//!
//! Converts `BatchWitness` into canonical chip traces via one entrypoint,
//! enforcing a shared E-Trace/contract boundary.

mod builder;
mod collectors;
mod generator;
mod lowering;
mod memory;
mod orchestration;
mod smt;
pub mod trace_map;
mod validation;

pub use builder::{AllTraceInputs, TraceBuilder, build_trace_map};
pub use generator::TraceGenerator;
pub use lowering::{LoweringOutput, lower_execution_records, lower_program_batch};
pub use smt::build_smt_paths;
pub use trace_map::TraceMap;
