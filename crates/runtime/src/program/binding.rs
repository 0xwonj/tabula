use tabula_artifact::Artifact;
#[cfg(feature = "prove")]
use tabula_compiler::SealedProgram;
pub use tabula_contract::ProgramBinding as Binding;

use crate::error::RuntimeError;

#[cfg(feature = "prove")]
pub(crate) fn binding_from_compiled_program(
    compiled_program: &SealedProgram,
) -> Result<Binding, RuntimeError> {
    let program_hash = compiled_program
        .as_artifact()
        .canonical_digest()
        .map_err(|e| RuntimeError::ValidationFailed {
            detail: format!("failed to hash runtime artifact: {e}"),
        })?;
    let metadata_hash = compiled_program.metadata_envelope().canonical_hash_hex();
    Ok(Binding::new(program_hash, metadata_hash))
}

pub(crate) fn binding_from_artifact(artifact: &Artifact) -> Result<Binding, RuntimeError> {
    let program_hash = artifact
        .canonical_digest()
        .map_err(|e| RuntimeError::ValidationFailed {
            detail: format!("failed to hash runtime artifact: {e}"),
        })?;
    let metadata_hash = artifact.contract_metadata.canonical_hash_hex();
    Ok(Binding::new(program_hash, metadata_hash))
}
