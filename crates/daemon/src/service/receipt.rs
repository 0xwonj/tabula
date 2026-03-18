//! Receipt hashing, building, and verification.

use std::time::{SystemTime, UNIX_EPOCH};

use tabula_artifact::{ExecutionStatement, StateSnapshot, TransactionBatch};
use tabula_compiler::CompiledProgram;
use tabula_core::ExecutionConsistencyStatus;

use super::error::{ServiceError, ServiceResult};
use super::types::ExecutionReceipt;
use crate::protocol::error::ErrorCode;

const RECEIPT_VERSION: u32 = 2;
const RECEIPT_SCHEME: &str = "execution_receipt_v2";
/// Verification result from receipt comparison.
#[derive(Debug, Clone)]
pub struct ReceiptVerification {
    pub verified: bool,
    pub message: String,
}

/// Build a canonical execution statement from execution artifacts.
pub fn build_execution_statement(
    artifact: &CompiledProgram,
    state: &StateSnapshot,
    batch: &TransactionBatch,
    state_after: &StateSnapshot,
) -> ServiceResult<ExecutionStatement> {
    let program_artifact = artifact.as_program_artifact();

    Ok(ExecutionStatement {
        program_hash: program_artifact.canonical_digest().map_err(|e| {
            ServiceError::internal(
                ErrorCode::InternalError,
                format!("failed to hash program artifact: {e}"),
            )
        })?,
        state_hash: state.canonical_digest().map_err(|e| {
            ServiceError::internal(
                ErrorCode::InternalError,
                format!("failed to hash state artifact: {e}"),
            )
        })?,
        batch_hash: batch.canonical_digest().map_err(|e| {
            ServiceError::internal(
                ErrorCode::InternalError,
                format!("failed to hash batch artifact: {e}"),
            )
        })?,
        state_after_hash: state_after.canonical_digest().map_err(|e| {
            ServiceError::internal(
                ErrorCode::InternalError,
                format!("failed to hash post-state artifact: {e}"),
            )
        })?,
        metadata_hash: bytes_to_hex(&artifact.metadata_envelope().canonical_hash()),
        old_state_root: vec![],
        new_state_root: vec![],
    })
}

/// Build an execution receipt from a canonical execution statement.
pub fn build_receipt(
    statement: &ExecutionStatement,
    tx_count: usize,
    emitted_count: usize,
    consistency: &ExecutionConsistencyStatus,
) -> ExecutionReceipt {
    ExecutionReceipt {
        version: RECEIPT_VERSION,
        scheme: RECEIPT_SCHEME.to_string(),
        statement_hash: statement.statement_hash(),
        program_hash: statement.program_hash.clone(),
        state_hash: statement.state_hash.clone(),
        batch_hash: statement.batch_hash.clone(),
        state_after_hash: statement.state_after_hash.clone(),
        metadata_hash: statement.metadata_hash.clone(),
        generated_at_ms: now_ms(),
        tx_count,
        emitted_count,
        consistency: consistency.clone(),
    }
}

/// Verify a receipt against the expected execution statement.
pub fn verify_receipt(
    proof: &ExecutionReceipt,
    statement: &ExecutionStatement,
    expected_statement_hash: &str,
) -> ReceiptVerification {
    if proof.version != RECEIPT_VERSION || proof.scheme != RECEIPT_SCHEME {
        return ReceiptVerification {
            verified: false,
            message: format!(
                "unsupported receipt format: expected version={}, scheme={}, got version={}, scheme={}",
                RECEIPT_VERSION, RECEIPT_SCHEME, proof.version, proof.scheme
            ),
        };
    }

    let recomputed = ExecutionStatement {
        program_hash: proof.program_hash.clone(),
        state_hash: proof.state_hash.clone(),
        batch_hash: proof.batch_hash.clone(),
        state_after_hash: proof.state_after_hash.clone(),
        metadata_hash: proof.metadata_hash.clone(),
        old_state_root: statement.old_state_root.clone(),
        new_state_root: statement.new_state_root.clone(),
    }
    .statement_hash();
    if recomputed != proof.statement_hash {
        return ReceiptVerification {
            verified: false,
            message: "receipt statement hash mismatch".to_string(),
        };
    }

    if proof.program_hash != statement.program_hash
        || proof.state_hash != statement.state_hash
        || proof.batch_hash != statement.batch_hash
        || proof.state_after_hash != statement.state_after_hash
        || proof.metadata_hash != statement.metadata_hash
    {
        return ReceiptVerification {
            verified: false,
            message: "receipt components do not match run statement".to_string(),
        };
    }

    if proof.statement_hash != expected_statement_hash {
        return ReceiptVerification {
            verified: false,
            message: "receipt statement hash does not match run".to_string(),
        };
    }

    ReceiptVerification {
        verified: true,
        message: "receipt verified".to_string(),
    }
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn bytes_to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        use std::fmt::Write as _;
        let _ = write!(out, "{b:02x}");
    }
    out
}
