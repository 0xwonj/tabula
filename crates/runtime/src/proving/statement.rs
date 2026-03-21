use p3_field::PrimeField32;
use tabula_artifact::{State, Statement, TransactionBatch};
use tabula_commitment::NativeDigest;
use tabula_machine::PublicStatement;

use crate::error::RuntimeError;
use crate::program::ResolvedProgram;

/// Build the canonical execution statement from execution artifacts and AIR public values.
pub fn build_execution_statement(
    resolved_program: &ResolvedProgram,
    state: &State,
    batch: &TransactionBatch,
    state_after: &State,
    air_statement: &PublicStatement,
) -> Result<Statement, RuntimeError> {
    Ok(Statement {
        program_hash: resolved_program.binding().program_hash().to_string(),
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
        metadata_hash: resolved_program.binding().metadata_hash().to_string(),
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
