use crate::error::SdkError;
use crate::types::Proof;
use std::sync::Arc;
use tabula_contract::PublicStatement;

/// Prepared verification handle for one `(artifact, environment)` pair.
#[derive(Clone)]
pub struct Verifier {
    prepared: Arc<tabula_runtime::PreparedVerifier>,
}

impl Verifier {
    pub(crate) fn new(program: &super::Program) -> Result<Self, SdkError> {
        Ok(Self {
            prepared: program
                .sdk()
                .prepare_prepared_verifier(program.artifact())?,
        })
    }

    /// Verifies a proof against an externally supplied expected public statement.
    pub fn verify_public_statement(
        &self,
        proof: &Proof,
        expected_public_statement: &PublicStatement,
    ) -> Result<(), SdkError> {
        self.prepared
            .verify(&proof.proof, expected_public_statement)
            .map_err(tabula_runtime::RuntimeError::from)
            .map_err(SdkError::from)?;
        Ok(())
    }
}

impl std::fmt::Debug for Verifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Verifier")
            .field("binding", self.prepared.binding())
            .finish_non_exhaustive()
    }
}
