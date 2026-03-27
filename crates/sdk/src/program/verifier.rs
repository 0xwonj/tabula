use super::Program;
use crate::error::SdkError;
use crate::types::Proof;

/// Prepared verification handle for one `(artifact, environment)` pair.
#[derive(Clone)]
pub struct Verifier {
    program: Program,
}

impl Verifier {
    pub(crate) fn new(program: Program) -> Self {
        Self { program }
    }

    /// Prepares verifier-side caches for the verifier artifact.
    pub fn warm(&self) -> Result<(), SdkError> {
        let _ = self
            .program
            .sdk()
            .prepare_verifier(self.program.artifact())?;
        Ok(())
    }

    /// Verifies a proof against the verifier artifact.
    pub fn verify(&self, proof: &Proof) -> Result<(), SdkError> {
        self.program
            .sdk()
            .prepare_verifier(self.program.artifact())?
            .verify(&proof.proof, &proof.statement)?;
        Ok(())
    }
}

impl std::fmt::Debug for Verifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Verifier")
            .field("artifact_digest", &self.program.artifact().digest())
            .finish_non_exhaustive()
    }
}
