//! STARK proving pipeline for daemon-side proof generation.
//!
//! Delegates to [`tabula_runtime`]'s proving pipeline for witness generation,
//! trace building, and proving. Assembles the result into a serializable
//! [`StarkProofSummary`].
//!
//! Gated behind the `stark` feature — see `mod.rs`.

use std::collections::BTreeMap;

use tabula_artifact::StarkProofSummary;
use tabula_core::{TableId, TableSchema};
use tabula_driver::RegisteredProgram;
use tabula_machine::TabulaMachine;
use tabula_runtime::ProofSummary;
use tabula_runtime::prove as rt_prove;

use super::error::{ServiceError, ServiceResult};
use super::execute::ExecutedBatch;
use crate::protocol::error::ErrorCode;

/// Generate a STARK proof for an executed batch and return a serializable summary.
pub fn prove_batch(
    executed: &ExecutedBatch,
    registered: &RegisteredProgram,
) -> ServiceResult<StarkProofSummary> {
    let schemas_by_id: BTreeMap<TableId, TableSchema> = registered
        .table_schemas
        .iter()
        .cloned()
        .map(|s| (s.id, s))
        .collect();

    // 1. Build old column states.
    let old_column_states =
        rt_prove::build_old_column_states(&schemas_by_id, &executed.inner.state_before)
            .map_err(map_runtime_error)?;

    // 2. Reconstruct core types.
    let batch_result = rt_prove::to_batch_result(&executed.inner);
    let batch = rt_prove::convert_batch(&executed.batch_file).map_err(map_runtime_error)?;

    // 3. Generate witness.
    let witness = rt_prove::generate_witness(&batch_result, &schemas_by_id, &old_column_states)
        .map_err(map_runtime_error)?;

    // 4. Build machine from schemas.
    let col_configs = derive_column_configs(&registered.table_schemas);
    let machine =
        TabulaMachine::new(&col_configs).map_err(|e| map_error(format!("machine: {e}")))?;

    // 5. Build traces.
    let traces = rt_prove::build_traces(
        &machine,
        &witness,
        &registered.program,
        &batch,
        &batch_result,
        &schemas_by_id,
    )
    .map_err(map_runtime_error)?;

    // 6. Extract proof metadata.
    let column_identities = rt_prove::extract_column_identities(&witness);
    let statement = rt_prove::extract_statement(&witness);

    // 7. Prove.
    let prove_start = std::time::Instant::now();
    let proof = machine
        .prove(traces, &column_identities, statement)
        .map_err(|e| map_error(format!("proving: {e}")))?;
    let prove_time_ms = prove_start.elapsed().as_millis() as u64;

    // 8. Verify.
    let verify_start = std::time::Instant::now();
    let verified = machine.verify(&proof).is_ok();
    let verify_time_ms = verify_start.elapsed().as_millis() as u64;

    // 9. Build summary.
    let summary = ProofSummary::from_proof(&proof);
    Ok(StarkProofSummary {
        scheme: "stark_v1".to_string(),
        verified,
        chip_count: summary.chip_count,
        chips: summary.chips,
        old_state_root: rt_prove::digest_to_hex(&witness.old_state_root),
        new_state_root: rt_prove::digest_to_hex(&witness.new_state_root),
        prove_time_ms,
        verify_time_ms,
        statement_hash: String::new(),
        program_hash: String::new(),
        batch_hash: String::new(),
    })
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

// ── Helpers ──────────────────────────────────────────────────────────────────

fn derive_column_configs(schemas: &[TableSchema]) -> Vec<tabula_machine::ColumnSetupConfig> {
    use tabula_commitment::scheme_tags;
    schemas
        .iter()
        .flat_map(|s| {
            s.columns
                .iter()
                .map(move |c| tabula_machine::ColumnSetupConfig {
                    table_id: s.id,
                    col_id: c.id,
                    scheme_tag: scheme_tags::SSMC,
                    receives_commitment: true,
                })
        })
        .collect()
}

fn map_runtime_error(e: tabula_runtime::RuntimeError) -> ServiceError {
    ServiceError::internal(ErrorCode::InternalError, e.to_string())
}

fn map_error(detail: String) -> ServiceError {
    ServiceError::internal(ErrorCode::InternalError, detail)
}
