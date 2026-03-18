//! Trace assembly infrastructure for Tabula witness pipelines.
//!
//! Generic orchestration APIs live at the root.
//! Builtin lowering helpers live under [`builtin`].

mod builder;
pub mod builtin;
mod lowering;
mod memory;
pub mod orchestration;
pub mod partition;
mod smt;
pub mod validation;

// Re-export core trace types from tabula-stark.
pub use tabula_stark::trace::{
    TraceContributor, TraceEntry, TraceGenerator, TraceMap, TracePhase, WitnessKey, WitnessStore,
    witness_labels,
};

pub use orchestration::build_all_traces;
pub use partition::{PartitionedStores, partition_by_tier};
pub use validation::debug_validate_trace_map;
