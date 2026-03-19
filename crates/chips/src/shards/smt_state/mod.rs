//! Per-column SMT state shard chip.
//!
//! Verifies row-level sparse Merkle openings and updates for one committed
//! column, then binds the resulting column roots to the shared MetaShard via
//! the CommitmentVerification bus.

pub mod air;
/// Column layout and sizing constants for the SMT state shard.
pub mod columns;
pub mod trace;

pub use air::SmtStateShardChip;
pub use columns::{SmtStateShardCols, smt_state_shard_width};
pub use trace::{SMT_STATE_WITNESS_LABEL, SmtStatePathWitness, SmtStateWitness};
