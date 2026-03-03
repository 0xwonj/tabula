//! Replay protection policy abstraction.

use crate::error::TabulaError;

/// Replay protection policy abstraction.
pub trait NoncePolicy: Send + Sync {
    /// Validate that a transaction's nonce is acceptable. Returns `Ok(())` on success.
    fn validate(
        &self,
        sender: &[u8; 32],
        tx_nonce: u64,
        current_nonce: u64,
    ) -> Result<(), TabulaError>;

    /// Compute the next nonce after a successful transaction.
    fn next_nonce(&self, sender: &[u8; 32], current_nonce: u64) -> u64;
}
