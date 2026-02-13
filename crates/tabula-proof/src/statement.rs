//! The ApplyBatch public inputs statement.

use tabula_core::state::Digest;
use tabula_core::tx::ProgramBudgets;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_statement_construction() {
        let stmt = ApplyBatchStatement {
            old_state_root: [0u8; 32],
            new_state_root: [1u8; 32],
            program_root: [2u8; 32],
            applied_tx_digest: [3u8; 32],
            static_table_root: [4u8; 32],
            budgets: ProgramBudgets {
                max_ops: 1000,
                max_slots: 256,
                max_accesses: 500,
            },
        };
        assert_ne!(stmt.old_state_root, stmt.new_state_root);
        assert_eq!(stmt.budgets.max_ops, 1000);
    }
}
