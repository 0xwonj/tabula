//! Mock prover and verifier for pipeline testing.

use tabula_core::error::TabulaError;

use crate::statement::ApplyBatchStatement;
use crate::traits::{Proof, Prover, Verifier};

/// Accept-all prover: returns an empty proof.
#[derive(Debug, Clone)]
pub struct MockProver;

impl Prover for MockProver {
    fn prove(&self, _statement: &ApplyBatchStatement) -> Result<Proof, TabulaError> {
        Ok(vec![0xDE, 0xAD]) // Dummy proof bytes
    }
}

/// Accept-all verifier: always returns true.
#[derive(Debug, Clone)]
pub struct MockVerifier;

impl Verifier for MockVerifier {
    fn verify(
        &self,
        _statement: &ApplyBatchStatement,
        _proof: &Proof,
    ) -> Result<bool, TabulaError> {
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_prover_verifier() {
        let stmt = ApplyBatchStatement {
            old_state_root: [0u8; 32],
            new_state_root: [1u8; 32],
            program_root: [2u8; 32],
            batch_digest: [3u8; 32],
        };

        let prover = MockProver;
        let proof = prover.prove(&stmt).unwrap();
        assert!(!proof.is_empty());

        let verifier = MockVerifier;
        assert!(verifier.verify(&stmt, &proof).unwrap());
    }
}
