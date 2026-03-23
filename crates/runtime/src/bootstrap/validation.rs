use std::collections::BTreeSet;

use tabula_artifact::Statement;
use tabula_compiler::SealedProgram;
use tabula_ir::PrecompileId;

use crate::error::RuntimeError;

/// Validate that the compiler-owned profile catalog still covers the schema surface.
pub(crate) fn validate_compiler_owned_profiles(
    compiled_program: &SealedProgram,
) -> Result<(), RuntimeError> {
    compiled_program
        .validate_column_profiles()
        .map_err(|detail| RuntimeError::ValidationFailed { detail })
}

/// Validate that every compiler-required precompile ID has an installed host backend.
pub(crate) fn validate_precompile_requirements(
    sealed_program: &SealedProgram,
    registered: &BTreeSet<PrecompileId>,
    subject: &str,
) -> Result<(), RuntimeError> {
    for descriptor in sealed_program.precompile_manifest() {
        if !registered.contains(&descriptor.precompile_id) {
            return Err(RuntimeError::ValidationFailed {
                detail: format!(
                    "program references precompile 0x{:04x} but no installed {} is registered",
                    descriptor.precompile_id.0, subject,
                ),
            });
        }
    }
    Ok(())
}

/// Validate that a statement matches the expected sealed artifact binding.
pub(crate) fn validate_statement_artifact_binding(
    program_hash: &str,
    metadata_hash: &str,
    expected_program_hash: &str,
    expected_metadata_hash: &str,
) -> Result<(), RuntimeError> {
    if program_hash != expected_program_hash {
        return Err(RuntimeError::ValidationFailed {
            detail: "execution statement program hash does not match the runtime artifact"
                .to_string(),
        });
    }
    if metadata_hash != expected_metadata_hash {
        return Err(RuntimeError::ValidationFailed {
            detail:
                "execution statement metadata hash does not match the runtime contract metadata"
                    .to_string(),
        });
    }
    Ok(())
}

/// Validate that a proof is bound to the expected execution statement and artifact.
pub(crate) fn validate_statement_binding(
    statement: &Statement,
    proof_statement_digest: &[u8; 32],
    expected_program_hash: &str,
    expected_metadata_hash: &str,
) -> Result<(), RuntimeError> {
    validate_statement_artifact_binding(
        &statement.program_hash,
        &statement.metadata_hash,
        expected_program_hash,
        expected_metadata_hash,
    )?;

    if proof_statement_digest != &statement.statement_hash_bytes() {
        return Err(RuntimeError::ValidationFailed {
            detail: "proof statement digest does not match expected execution statement"
                .to_string(),
        });
    }

    Ok(())
}
