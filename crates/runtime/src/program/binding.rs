use tabula_artifact::ProgramArtifact;
#[cfg(feature = "prove")]
use tabula_compiler::CompiledProgram;

use crate::error::RuntimeError;

/// Canonical artifact binding precomputed during runtime preparation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgramBinding {
    program_hash: String,
    metadata_hash: String,
}

impl ProgramBinding {
    #[cfg(feature = "prove")]
    pub(crate) fn from_compiled_program(
        compiled_program: &CompiledProgram,
    ) -> Result<Self, RuntimeError> {
        let program_hash = compiled_program
            .as_program_artifact()
            .canonical_digest()
            .map_err(|e| RuntimeError::ValidationFailed {
                detail: format!("failed to hash runtime program artifact: {e}"),
            })?;
        let metadata_hash = compiled_program.metadata_envelope().canonical_hash_hex();
        Ok(Self {
            program_hash,
            metadata_hash,
        })
    }

    pub(crate) fn from_program_artifact(
        program_artifact: &ProgramArtifact,
    ) -> Result<Self, RuntimeError> {
        let program_hash =
            program_artifact
                .canonical_digest()
                .map_err(|e| RuntimeError::ValidationFailed {
                    detail: format!("failed to hash runtime program artifact: {e}"),
                })?;
        let metadata_hash = program_artifact.contract_metadata.canonical_hash_hex();
        Ok(Self {
            program_hash,
            metadata_hash,
        })
    }

    /// Canonical digest of the sealed program artifact backing this runtime.
    pub fn program_hash(&self) -> &str {
        &self.program_hash
    }

    /// Canonical digest of the runtime contract metadata.
    pub fn metadata_hash(&self) -> &str {
        &self.metadata_hash
    }
}
