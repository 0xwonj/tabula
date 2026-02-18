//! The ApplyBatch public inputs statement.

use tabula_core::{Digest, ProgramBudgets};

/// Public inputs for the ApplyBatch proof.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyBatchStatement {
    /// The state root before batch execution.
    pub old_state_root: Digest,
    /// The state root after batch execution.
    pub new_state_root: Digest,
    /// Commitment to the program (set of tx type definitions).
    pub program_root: Digest,
    /// Commitment to the batch of applied transactions.
    pub applied_tx_digest: Digest,
    /// Commitment to the static lookup tables.
    pub static_table_root: Digest,
    /// Program resource budgets (DoS prevention).
    pub budgets: ProgramBudgets,
}
