//! Receipt hashing, building, and verification.

use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use sha2::{Digest, Sha256};

use tabula_artifact::ProgramArtifact;
use tabula_artifact::StateFile;
use tabula_core::ExecutionConsistencyStatus;
use tabula_driver::RegisteredProgram;

use super::error::{ServiceError, ServiceResult};
use super::types::ExecutionReceipt;
use crate::protocol::error::ErrorCode;

const RECEIPT_VERSION: u32 = 1;
const RECEIPT_SCHEME: &str = "execution_receipt_v1";
const JSON_HASH_DOMAIN: &[u8] = b"tabula.orchestrator.json_hash.v1";
const STATEMENT_HASH_DOMAIN: &[u8] = b"tabula.orchestrator.statement_hash.v1";

/// Pre-computed hashes for a statement.
#[derive(Debug, Clone)]
pub struct StatementComponents {
    pub program_hash: String,
    pub state_hash: String,
    pub batch_hash: String,
    pub state_after_hash: String,
    pub metadata_hash: String,
}

impl StatementComponents {
    /// Compute statement hash from components.
    pub fn statement_hash(&self) -> String {
        statement_hash(
            &self.program_hash,
            &self.state_hash,
            &self.batch_hash,
            &self.state_after_hash,
            &self.metadata_hash,
        )
    }
}

/// Verification result from receipt comparison.
#[derive(Debug, Clone)]
pub struct ReceiptVerification {
    pub verified: bool,
    pub message: String,
}

/// Build statement components from execution artifacts.
pub fn statement_components(
    artifact: &RegisteredProgram,
    state: &StateFile,
    batch: &tabula_artifact::BatchFile,
    state_after: &StateFile,
) -> ServiceResult<StatementComponents> {
    let program_file = ProgramArtifact {
        table_schemas: artifact.table_schemas.clone(),
        tx_types: artifact.tx_types.clone(),
        contract_metadata: Some(artifact.metadata_envelope.clone()),
    };

    Ok(StatementComponents {
        program_hash: hash_json("program", &program_file)?,
        state_hash: hash_json("state", state)?,
        batch_hash: hash_json("batch", batch)?,
        state_after_hash: hash_json("state_after", state_after)?,
        metadata_hash: bytes_to_hex(&artifact.metadata_envelope.canonical_hash()),
    })
}

/// Build an execution receipt from statement components and execution metadata.
pub fn build_receipt(
    components: &StatementComponents,
    tx_count: usize,
    emitted_count: usize,
    consistency: &ExecutionConsistencyStatus,
) -> ExecutionReceipt {
    ExecutionReceipt {
        version: RECEIPT_VERSION,
        scheme: RECEIPT_SCHEME.to_string(),
        statement_hash: components.statement_hash(),
        program_hash: components.program_hash.clone(),
        state_hash: components.state_hash.clone(),
        batch_hash: components.batch_hash.clone(),
        state_after_hash: components.state_after_hash.clone(),
        metadata_hash: components.metadata_hash.clone(),
        generated_at_ms: now_ms(),
        tx_count,
        emitted_count,
        consistency: consistency.clone(),
    }
}

/// Verify a receipt against expected statement components.
pub fn verify_receipt(
    proof: &ExecutionReceipt,
    components: &StatementComponents,
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

    let recomputed = statement_hash(
        &proof.program_hash,
        &proof.state_hash,
        &proof.batch_hash,
        &proof.state_after_hash,
        &proof.metadata_hash,
    );
    if recomputed != proof.statement_hash {
        return ReceiptVerification {
            verified: false,
            message: "receipt statement hash mismatch".to_string(),
        };
    }

    if proof.program_hash != components.program_hash
        || proof.state_hash != components.state_hash
        || proof.batch_hash != components.batch_hash
        || proof.state_after_hash != components.state_after_hash
        || proof.metadata_hash != components.metadata_hash
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

pub fn hash_json<T: Serialize>(label: &str, value: &T) -> ServiceResult<String> {
    let bytes = serde_json::to_vec(value).map_err(|e| {
        ServiceError::internal(
            ErrorCode::InternalError,
            format!("failed to serialize {label} for hashing: {e}"),
        )
    })?;

    let mut hasher = Sha256::new();
    hasher.update(JSON_HASH_DOMAIN);
    hasher.update(label.as_bytes());
    hasher.update([0u8]);
    hasher.update(&bytes);
    Ok(bytes_to_hex(&hasher.finalize()))
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
        let _ = write!(out, "{:02x}", b);
    }
    out
}

fn statement_hash(
    program_hash: &str,
    state_hash: &str,
    batch_hash: &str,
    state_after_hash: &str,
    metadata_hash: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(STATEMENT_HASH_DOMAIN);
    hash_part(&mut hasher, b"program_hash", program_hash);
    hash_part(&mut hasher, b"state_hash", state_hash);
    hash_part(&mut hasher, b"batch_hash", batch_hash);
    hash_part(&mut hasher, b"state_after_hash", state_after_hash);
    hash_part(&mut hasher, b"metadata_hash", metadata_hash);
    bytes_to_hex(&hasher.finalize())
}

fn hash_part(hasher: &mut Sha256, label: &[u8], value: &str) {
    hasher.update(label);
    hasher.update([0u8]);
    hasher.update(value.as_bytes());
    hasher.update([0xffu8]);
}
