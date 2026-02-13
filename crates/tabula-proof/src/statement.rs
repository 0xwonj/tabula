//! The ApplyBatch public inputs statement.

use tabula_core::state::Digest;

/// Public inputs for the ApplyBatch proof.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyBatchStatement {
    /// The state root before batch execution.
    pub old_state_root: Digest,
    /// The state root after batch execution.
    pub new_state_root: Digest,
    /// Commitment to the program (set of tx type definitions).
    pub program_root: Digest,
    /// Commitment to the batch of transactions.
    pub batch_digest: Digest,
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
            batch_digest: [3u8; 32],
        };
        assert_ne!(stmt.old_state_root, stmt.new_state_root);
    }
}
