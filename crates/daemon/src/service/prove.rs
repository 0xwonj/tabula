//! STARK proving pipeline for daemon-side proof generation.
//!
//! Mirrors the E2E test pipeline: execute → witness → traces → prove → verify.
//! Returns a serializable [`StarkProofSummary`] (the actual p3 proof is not serializable).
//!
//! Gated behind the `stark` feature — see `mod.rs`.

use std::collections::BTreeMap;

use p3_baby_bear::BabyBear;
use p3_field::PrimeField32;
use tabula_artifact::{ChipSummary, StarkProofSummary};
use tabula_commitment::{BabyBearCodec, ColumnState, HybridVC, NativeDigest, PoseidonHasher};
use tabula_core::mock::InMemoryStaticTables;
use tabula_core::traits::ValueCodec;
use tabula_core::{Batch, ColId, RowKey, TableId, TableSchema};
use tabula_driver::RegisteredProgram;
use tabula_proof::chips::TabulaAir;
use tabula_proof::stark;
use tabula_proof::trace::build_trace_map;
use tabula_proof::witness::WitnessGenerator;

use super::error::{ServiceError, ServiceResult};
use super::execute::ExecutedBatch;
use crate::protocol::error::ErrorCode;

/// Hybrid VC threshold for SSMC vs SMT selection.
const VC_THRESHOLD: usize = 1024;

/// Generate a STARK proof for an executed batch and return a serializable summary.
pub fn prove_batch(
    executed: &ExecutedBatch,
    registered: &RegisteredProgram,
) -> ServiceResult<StarkProofSummary> {
    // 1. Reconstruct core types from executed batch.
    let schemas_by_id: BTreeMap<TableId, TableSchema> = registered
        .table_schemas
        .iter()
        .cloned()
        .map(|s| (s.id, s))
        .collect();

    let execution_result = tabula_core::ExecutionResult {
        read_set_old: executed.inner.read_set.clone(),
        write_set_final: executed.inner.write_set.clone(),
        events: executed.inner.events.clone(),
        emitted: executed.inner.emitted.clone(),
        tx_outcomes: executed.inner.tx_outcomes.clone(),
    };

    let batch = Batch {
        transactions: executed
            .batch_file
            .transactions
            .iter()
            .map(|t| {
                t.to_transaction()
                    .map_err(|e| ServiceError::internal(ErrorCode::InternalError, e.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()?,
    };

    // 2. Build old_column_states from state file + schemas.
    let old_column_states = build_old_column_states(&schemas_by_id, &executed.inner.state_before)
        .map_err(|e| {
        ServiceError::internal(ErrorCode::InternalError, format!("column state build: {e}"))
    })?;

    // 3. Generate witness.
    let vc = HybridVC::new(PoseidonHasher::new(), VC_THRESHOLD);
    let wg = WitnessGenerator::new(vc);
    let witness = wg
        .generate(&execution_result, &schemas_by_id, &old_column_states)
        .map_err(|e| {
            ServiceError::internal(ErrorCode::InternalError, format!("witness gen: {e}"))
        })?;

    // 4. Build trace map (all chip traces + public values).
    let traces = build_trace_map::<PoseidonHasher, 3>(
        &witness,
        &registered.program,
        &batch,
        &execution_result,
        &schemas_by_id,
        &InMemoryStaticTables::new(),
        PoseidonHasher::new(),
    )
    .map_err(|e| ServiceError::internal(ErrorCode::InternalError, format!("trace build: {e}")))?;

    // 5. Prove (timed).
    let prove_start = std::time::Instant::now();
    let proof = stark::prove::<TabulaAir>(&stark::default_config(), &traces);
    let prove_time_ms = prove_start.elapsed().as_millis() as u64;

    // 6. Verify (timed).
    let verify_start = std::time::Instant::now();
    let verified = stark::verify::<TabulaAir>(&proof).is_ok();
    let verify_time_ms = verify_start.elapsed().as_millis() as u64;

    // 8. Assemble summary.
    let chips: Vec<ChipSummary> = proof
        .chip_proofs
        .iter()
        .map(|entry| ChipSummary {
            name: entry.chip_name.to_string(),
            trace_height: entry.trace_height,
        })
        .collect();

    Ok(StarkProofSummary {
        scheme: "stark_v1".to_string(),
        verified,
        chip_count: chips.len(),
        chips,
        old_state_root: digest_to_hex_vec(&witness.old_state_root),
        new_state_root: digest_to_hex_vec(&witness.new_state_root),
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

/// Build `old_column_states` from state file cells and schemas.
///
/// Enumerates ALL schema columns (not just those with data) to ensure
/// empty columns get proper commitments.
fn build_old_column_states(
    schemas: &BTreeMap<TableId, TableSchema>,
    state_file: &tabula_artifact::StateFile,
) -> Result<BTreeMap<(TableId, ColId), ColumnState<PoseidonHasher>>, String> {
    let codec = BabyBearCodec;
    let vc = HybridVC::new(PoseidonHasher::new(), VC_THRESHOLD);

    // Group state cells by (table, col).
    type EncodedEntries = BTreeMap<(TableId, ColId), Vec<(RowKey, Vec<BabyBear>)>>;
    let mut entries_by_col: EncodedEntries = BTreeMap::new();

    for cell in &state_file.cells {
        if let Some(value) = &cell.value {
            let encoded = codec.encode(value).map_err(|e| {
                format!(
                    "encode cell ({},{},{}): {e}",
                    cell.table, cell.col, cell.row
                )
            })?;
            entries_by_col
                .entry((TableId(cell.table), ColId(cell.col)))
                .or_default()
                .push((RowKey(cell.row), encoded));
        }
    }

    let mut result = BTreeMap::new();
    for schema in schemas.values() {
        for col_def in &schema.columns {
            let mut entries = entries_by_col
                .remove(&(schema.id, col_def.id))
                .unwrap_or_default();
            entries.sort_by_key(|(row, _)| *row);
            let (state, _com) = vc
                .commit_column(schema.id, col_def.id, entries)
                .map_err(|e| e.to_string())?;
            result.insert((schema.id, col_def.id), state);
        }
    }

    Ok(result)
}

/// Convert a NativeDigest (8 BabyBear elements) to hex strings.
fn digest_to_hex_vec(digest: &NativeDigest) -> Vec<String> {
    digest
        .0
        .iter()
        .map(|fe| format!("{:08x}", fe.as_canonical_u32()))
        .collect()
}
