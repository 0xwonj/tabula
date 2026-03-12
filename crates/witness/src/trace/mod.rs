//! Trace builder orchestrator (M12 entry).
//!
//! Converts `BatchWitness` into canonical chip traces via one entrypoint,
//! enforcing a shared E-Trace/contract boundary.

mod builder;
mod lowering;
pub mod memory;
pub mod orchestration;
pub mod partition;
mod smt;
pub mod validation;

// Re-export core trace types from tabula-stark.
pub use tabula_stark::trace::{
    TraceContributor, TraceEntry, TraceGenerator, TraceMap, TracePhase, WitnessKey, WitnessStore,
    witness_labels,
};

pub use builder::{AllTraceInputs, TraceBuilder};
pub use lowering::{
    LoweringOutput, PrecompileExecuteFn, PropertyReadFn, lower_execution_records,
    lower_program_batch,
};
pub use memory::prepare_shard_witness;
pub use orchestration::build_all_traces;
pub use partition::{PartitionedStores, partition_by_tier};
pub use smt::build_smt_paths;
pub use validation::debug_validate_trace_map;
