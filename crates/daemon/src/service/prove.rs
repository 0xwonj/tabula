//! STARK proving pipeline for daemon-side proof generation.
//!
//! Uses a prepared [`tabula_runtime::TabulaRuntime`] so proving reuses the
//! runtime's machine and program-scoped setup instead of rebuilding them
//! for each run.

use tabula_artifact::Statement;
use tabula_runtime::{ProofSummary, ProveInput, TabulaRuntime};

use super::error::{ServiceError, ServiceResult};
use super::execute::ExecutedBatch;
use super::types::{ChipSummary, StarkProofSummary};
use crate::protocol::error::ErrorCode;

/// Generate a STARK proof for an executed batch and return a serializable summary.
pub fn prove_batch(
    executed: &ExecutedBatch,
    runtime: &TabulaRuntime,
) -> ServiceResult<(StarkProofSummary, Statement)> {
    let prove_start = std::time::Instant::now();
    let prove_result = runtime
        .prove(&ProveInput {
            state: &executed.inner.state_before,
            batch: &executed.transaction_batch,
            executed: &executed.inner,
        })
        .map_err(|e| map_runtime_error(&e))?;
    let prove_time_ms = prove_start.elapsed().as_millis() as u64;

    let verify_start = std::time::Instant::now();
    let verified = runtime
        .verify(&prove_result.proof, &prove_result.statement)
        .is_ok();
    let verify_time_ms = verify_start.elapsed().as_millis() as u64;

    let summary = summary_from_proof(
        &prove_result.summary,
        &prove_result.statement,
        verified,
        prove_time_ms,
        verify_time_ms,
    );

    Ok((summary, prove_result.statement))
}

/// Return a mock STARK proof summary for UI display when proof generation fails.
pub fn mock_stark_summary() -> StarkProofSummary {
    let zero_root: Vec<String> = vec!["00000000".to_string(); 8];
    StarkProofSummary {
        scheme: "stark_v2 (mock)".to_string(),
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
    statement: &Statement,
    verified: bool,
    prove_time_ms: u64,
    verify_time_ms: u64,
) -> StarkProofSummary {
    StarkProofSummary {
        scheme: "stark_v2".to_string(),
        verified,
        chip_count: summary.chip_count,
        chips: summary
            .chips
            .iter()
            .map(|chip| ChipSummary {
                name: chip.name.clone(),
                trace_height: chip.trace_height,
            })
            .collect(),
        old_state_root: statement.old_state_root.clone(),
        new_state_root: statement.new_state_root.clone(),
        prove_time_ms,
        verify_time_ms,
        statement_hash: statement.statement_hash(),
        program_hash: statement.program_hash.clone(),
        batch_hash: statement.batch_hash.clone(),
    }
}

fn map_runtime_error(e: &tabula_runtime::RuntimeError) -> ServiceError {
    ServiceError::internal(ErrorCode::InternalError, e.to_string())
}
