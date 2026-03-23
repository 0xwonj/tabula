use tabula_artifact::State;
use tabula_core::{ColId, TableId};
use tabula_executor::ResolvedExecutionProgram;

use crate::error::RuntimeError;

/// Validate that every normalized state cell belongs to the declared execution state surface.
pub(crate) fn validate_execution_state_surface(
    program: &ResolvedExecutionProgram,
    state: &State,
) -> Result<(), RuntimeError> {
    validate_state_surface(
        state,
        |table, col| program.has_column(table, col),
        "execution",
    )
}

#[cfg(feature = "prove")]
use crate::program::ResolvedProofProgram;

/// Validate that every normalized state cell belongs to the declared proof state surface.
#[cfg(feature = "prove")]
pub(crate) fn validate_proof_state_surface(
    program: &ResolvedProofProgram,
    state: &State,
) -> Result<(), RuntimeError> {
    validate_state_surface(
        state,
        |table, col| program.column_backends().contains_key(&(table, col)),
        "proof",
    )
}

/// Validate that the normalized proving input state matches the executed pre-state.
#[cfg(feature = "prove")]
pub(crate) fn validate_prove_input_prestate(
    normalized_state: &State,
    executed_state_before: &State,
) -> Result<(), RuntimeError> {
    let normalized_digest = normalized_state
        .canonical_digest_bytes()
        .map_err(RuntimeError::InvalidState)?;
    let executed_digest = executed_state_before
        .canonical_digest_bytes()
        .map_err(RuntimeError::InvalidState)?;
    if normalized_digest != executed_digest {
        return Err(RuntimeError::ValidationFailed {
            detail: "prove input state does not match the executed batch pre-state".to_string(),
        });
    }
    Ok(())
}

fn validate_state_surface(
    state: &State,
    mut is_allowed: impl FnMut(TableId, ColId) -> bool,
    subject: &str,
) -> Result<(), RuntimeError> {
    for cell in &state.cells {
        let table = TableId(cell.table);
        let col = ColId(cell.col);
        if !is_allowed(table, col) {
            return Err(RuntimeError::ValidationFailed {
                detail: format!(
                    "{subject} state cell ({}, {}) is outside the declared program state surface",
                    cell.table, cell.col,
                ),
            });
        }
    }
    Ok(())
}
