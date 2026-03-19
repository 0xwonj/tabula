//! Builtin witness-lowering and trace-input assembly helpers.

/// Builtin IR lowering helpers.
pub mod lowering {
    pub use super::lowering_impl::{LoweringOutput, lower_execution_records, lower_program_batch};
}

/// Builtin per-column memory witness helpers.
pub mod memory {
    pub use super::memory_impl::{
        prepare_memory_shard_rows_from_parts, prepare_meta_shard_row_from_parts,
        prepare_ssmc_column_witness_from_parts,
    };
}

/// Builtin SMT witness helpers.
pub mod smt {
    pub use super::smt_impl::build_smt_paths;
}

pub use super::builder::{
    AllTraceInputs, BuiltinTraceBuilder, BuiltinTraceContext, BuiltinWitnessInputs,
};
pub use tabula_chips::shards::property::trace::PropertyReadRecord;

use super::lowering as lowering_impl;
use super::memory as memory_impl;
use super::smt as smt_impl;
