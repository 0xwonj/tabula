//! Sequential nonce policy.

use crate::error::TabulaError;
use crate::traits::NoncePolicy;

/// Nonce policy: `tx_nonce == current_nonce`, next = `current + 1`.
#[derive(Debug, Clone, Copy)]
pub struct SequentialNonce;

impl NoncePolicy for SequentialNonce {
    fn validate(
        &self,
        sender: &[u8; 32],
        tx_nonce: u64,
        current_nonce: u64,
    ) -> Result<(), TabulaError> {
        if tx_nonce == current_nonce {
            Ok(())
        } else {
            Err(TabulaError::InvalidNonce {
                sender: *sender,
                expected: current_nonce,
                actual: tx_nonce,
            })
        }
    }

    fn next_nonce(&self, _sender: &[u8; 32], current_nonce: u64) -> u64 {
        current_nonce + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequential_nonce_validates() {
        let n = SequentialNonce;
        let sender = [0u8; 32];
        assert!(n.validate(&sender, 0, 0).is_ok());
        assert!(n.validate(&sender, 1, 0).is_err());
        assert_eq!(n.next_nonce(&sender, 0), 1);
    }
}
