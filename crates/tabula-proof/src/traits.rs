//! Prover and Verifier trait definitions.

use tabula_core::error::TabulaError;

use crate::statement::ApplyBatchStatement;

/// A proof object (opaque bytes for now).
pub type Proof = Vec<u8>;

/// Generates proofs for the ApplyBatch statement.
pub trait Prover: Send + Sync {
    /// Generate a proof for the given statement.
    fn prove(&self, statement: &ApplyBatchStatement) -> Result<Proof, TabulaError>;
}

/// Verifies proofs for the ApplyBatch statement.
pub trait Verifier: Send + Sync {
    /// Verify a proof against the given statement.
    fn verify(&self, statement: &ApplyBatchStatement, proof: &Proof) -> Result<bool, TabulaError>;
}
