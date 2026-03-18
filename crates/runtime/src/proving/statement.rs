use p3_field::PrimeField32;
use tabula_artifact::{ExecutionStatement, StateSnapshot, TransactionBatch};
use tabula_commitment::NativeDigest;
use tabula_machine::PublicStatement;

use crate::error::RuntimeError;
use crate::program::RuntimeProgram;

/// Build the canonical execution statement from execution artifacts and AIR public values.
pub fn build_execution_statement(
    runtime_program: &RuntimeProgram,
    state: &StateSnapshot,
    batch: &TransactionBatch,
    state_after: &StateSnapshot,
    air_statement: &PublicStatement,
) -> Result<ExecutionStatement, RuntimeError> {
    Ok(ExecutionStatement {
        program_hash: runtime_program.binding().program_hash().to_string(),
        state_hash: state
            .canonical_digest()
            .map_err(|e| RuntimeError::StatementBuild {
                detail: format!("failed to hash state artifact: {e}"),
            })?,
        batch_hash: batch
            .canonical_digest()
            .map_err(|e| RuntimeError::StatementBuild {
                detail: format!("failed to hash batch artifact: {e}"),
            })?,
        state_after_hash: state_after.canonical_digest().map_err(|e| {
            RuntimeError::StatementBuild {
                detail: format!("failed to hash post-state artifact: {e}"),
            }
        })?,
        metadata_hash: runtime_program.binding().metadata_hash().to_string(),
        old_state_root: digest_to_hex(&air_statement.old_root),
        new_state_root: digest_to_hex(&air_statement.new_root),
    })
}

/// Convert a `NativeDigest` (8 KoalaBear elements) to hex strings.
pub fn digest_to_hex(digest: &NativeDigest) -> Vec<String> {
    digest
        .0
        .iter()
        .map(|fe| format!("{:08x}", fe.as_canonical_u32()))
        .collect()
}
