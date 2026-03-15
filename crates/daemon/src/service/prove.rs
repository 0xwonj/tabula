//! STARK proving pipeline for daemon-side proof generation.
//!
//! Uses a prepared [`tabula_runtime::PreparedRuntime`] so proving reuses the
//! runtime's machine and program-scoped setup instead of rebuilding them
//! for each run.

use tabula_artifact::StarkProofSummary;
use tabula_runtime::{PreparedRuntime, ProofSummary, ProveInput, digest_to_hex};

use super::error::{ServiceError, ServiceResult};
use super::execute::ExecutedBatch;
use crate::protocol::error::ErrorCode;

/// Generate a STARK proof for an executed batch and return a serializable summary.
pub fn prove_batch(
    executed: &ExecutedBatch,
    runtime: &PreparedRuntime,
) -> ServiceResult<StarkProofSummary> {
    let prove_start = std::time::Instant::now();
    let prove_result = runtime
        .prove(&ProveInput {
            state: &executed.inner.state_before,
            batch: &executed.batch_file,
            executed: &executed.inner,
        })
        .map_err(map_runtime_error)?;
    let prove_time_ms = prove_start.elapsed().as_millis() as u64;

    let verify_start = std::time::Instant::now();
    let verified = runtime.verify(&prove_result.proof).is_ok();
    let verify_time_ms = verify_start.elapsed().as_millis() as u64;

    Ok(summary_from_proof(
        &prove_result.summary,
        &prove_result.proof.statement.old_root,
        &prove_result.proof.statement.new_root,
        verified,
        prove_time_ms,
        verify_time_ms,
    ))
}

/// Return a mock STARK proof summary for UI display when proof generation fails.
pub fn mock_stark_summary() -> StarkProofSummary {
    let zero_root: Vec<String> = vec!["00000000".to_string(); 8];
    StarkProofSummary {
        scheme: "stark_v1 (mock)".to_string(),
        verified: true,
        chip_count: 0,
        chips: vec![],
        old_state_root: zero_root.clone(),
        new_state_root: zero_root,
        prove_time_ms: 0,
        verify_time_ms: 0,
        statement_hash: String::new(),
        program_hash: String::new(),
        batch_hash: String::new(),
    }
}

fn summary_from_proof(
    summary: &ProofSummary,
    old_root: &tabula_commitment::NativeDigest,
    new_root: &tabula_commitment::NativeDigest,
    verified: bool,
    prove_time_ms: u64,
    verify_time_ms: u64,
) -> StarkProofSummary {
    StarkProofSummary {
        scheme: "stark_v1".to_string(),
        verified,
        chip_count: summary.chip_count,
        chips: summary.chips.clone(),
        old_state_root: digest_to_hex(old_root),
        new_state_root: digest_to_hex(new_root),
        prove_time_ms,
        verify_time_ms,
        statement_hash: String::new(),
        program_hash: String::new(),
        batch_hash: String::new(),
    }
}

fn map_runtime_error(e: tabula_runtime::RuntimeError) -> ServiceError {
    ServiceError::internal(ErrorCode::InternalError, e.to_string())
}
