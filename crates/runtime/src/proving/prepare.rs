//! Canonical batch proof-input preparation for runtime-owned proving.

use std::collections::{BTreeMap, BTreeSet};

use p3_koala_bear::KoalaBear;

use tabula_artifact::{StateSnapshot, TransactionBatch};
use tabula_commitment::{KoalaBearCodec, PoseidonHasher};
use tabula_core::traits::ValueCodec;
use tabula_core::{Batch, BatchResult, ColId, InMemoryStaticTables, RowKey, TableId};
use tabula_witness::ExecutionInputPreparer;
use tabula_witness::trace::builtin::PropertyReadRecord;
use tabula_witness::trace::builtin::lowering::LoweringOutput;

use crate::columns::{BatchProofInput, ColumnTransitionInput};
use crate::error::RuntimeError;
use crate::execute::ExecutedBatch;
use crate::program::RuntimeProgram;

type EncodedColumnEntries = BTreeMap<(TableId, ColId), Vec<(RowKey, Vec<KoalaBear>)>>;

pub(crate) struct PreparedBatchProofArtifacts {
    pub(crate) proof_input: BatchProofInput,
    pub(crate) lowering: LoweringOutput,
}

/// Prepare the canonical batch proof input plus shared lowering artifacts.
#[tracing::instrument(skip_all, fields(col_count))]
pub(crate) fn prepare_batch_proof_input(
    runtime_program: &RuntimeProgram,
    state_file: &StateSnapshot,
    batch: &Batch,
    batch_result: &BatchResult,
) -> Result<PreparedBatchProofArtifacts, RuntimeError> {
    let mut old_entries_by_col = encode_old_entries_by_column(runtime_program, state_file)?;
    let empty_columns: BTreeSet<(TableId, ColId)> = old_entries_by_col
        .iter()
        .filter_map(|(key, entries)| entries.is_empty().then_some(*key))
        .collect();

    let lowering = tabula_witness::trace::builtin::lowering::lower_program_batch::<3>(
        runtime_program.program(),
        batch,
        batch_result,
        runtime_program.schemas_by_id(),
        &InMemoryStaticTables::new(),
        &empty_columns,
    )
    .map_err(RuntimeError::TraceBuild)?;
    let property_reads = lowering.property_read_records();

    let preparer = ExecutionInputPreparer::new(PoseidonHasher::new());
    let mut prepared = preparer
        .prepare_execution_inputs(
            batch_result,
            runtime_program.schemas_by_id(),
            runtime_program.column_plans().keys(),
        )
        .map_err(|e| RuntimeError::WitnessGeneration {
            detail: e.to_string(),
        })?;

    let mut columns = Vec::with_capacity(runtime_program.column_plans().len());
    for (&(table, col), plan) in runtime_program.column_plans() {
        let value_type = prepared
            .type_map
            .get(&(table, col))
            .copied()
            .ok_or_else(|| RuntimeError::ValidationFailed {
                detail: format!(
                    "missing value type for planned column ({}, {})",
                    table.0, col.0
                ),
            })?;
        let old_entries = old_entries_by_col.remove(&(table, col)).ok_or_else(|| {
            RuntimeError::ValidationFailed {
                detail: format!(
                    "missing encoded old entries for planned column ({}, {})",
                    table.0, col.0
                ),
            }
        })?;
        let init_rows = prepared
            .init_rows_by_col
            .remove(&(table, col))
            .unwrap_or_default();
        let access_rows = prepared
            .access_rows_by_col
            .remove(&(table, col))
            .unwrap_or_default();
        let writes = prepared
            .writes_by_col
            .remove(&(table, col))
            .unwrap_or_default();
        let is_touched = prepared.touched.contains(&(table, col));
        let reads = property_reads
            .get(&(table, col))
            .map_or(&[] as &[PropertyReadRecord], Vec::as_slice);
        let Some(backend) = runtime_program.transition_backends().get(&(table, col)) else {
            return Err(RuntimeError::ValidationFailed {
                detail: format!(
                    "missing column transition backend for table {} col {}",
                    table.0, col.0
                ),
            });
        };
        let proof_input = backend
            .build_proof_input(
                ColumnTransitionInput {
                    table,
                    col,
                    value_type,
                    old_entries,
                    init_rows,
                    access_rows,
                    writes,
                    is_touched,
                },
                reads,
            )
            .map_err(|e| RuntimeError::WitnessGeneration {
                detail: format!(
                    "column ({}, {}) transition backend '{}': {e}",
                    plan.table_id.0,
                    plan.col_id.0,
                    backend.name(),
                ),
            })?;
        columns.push(proof_input);
    }

    tracing::Span::current().record("col_count", columns.len());

    let metas: Vec<_> = columns.iter().map(|column| column.meta.clone()).collect();
    let (old_state_root, new_state_root) = preparer.compute_state_roots_from_metas(&metas);

    Ok(PreparedBatchProofArtifacts {
        proof_input: BatchProofInput {
            columns,
            old_state_root,
            new_state_root,
        },
        lowering,
    })
}

fn encode_old_entries_by_column(
    runtime_program: &RuntimeProgram,
    state_file: &StateSnapshot,
) -> Result<EncodedColumnEntries, RuntimeError> {
    let codec = KoalaBearCodec;

    let mut entries_by_col: EncodedColumnEntries = BTreeMap::new();
    for cell in &state_file.cells {
        if let Some(value) = &cell.value {
            let encoded = codec.encode(value).map_err(|e| RuntimeError::ColumnState {
                detail: format!(
                    "encode cell ({},{},{}): {e}",
                    cell.table, cell.col, cell.row
                ),
            })?;
            entries_by_col
                .entry((TableId(cell.table), ColId(cell.col)))
                .or_default()
                .push((RowKey(cell.row), encoded));
        }
    }

    for schema in runtime_program.schemas_by_id().values() {
        for col_def in &schema.columns {
            if !runtime_program
                .column_plans()
                .contains_key(&(schema.id, col_def.id))
            {
                return Err(RuntimeError::ValidationFailed {
                    detail: format!(
                        "missing column plan for table {} col {}",
                        schema.id.0, col_def.id.0,
                    ),
                });
            }
            let entries = entries_by_col.entry((schema.id, col_def.id)).or_default();
            entries.sort_by_key(|(row, _)| *row);
        }
    }

    Ok(entries_by_col)
}

/// Convert a `TransactionBatch` into a `Batch`.
pub(crate) fn convert_batch(batch_file: &TransactionBatch) -> Result<Batch, RuntimeError> {
    let transactions = batch_file
        .transactions
        .iter()
        .map(|t| t.to_transaction().map_err(RuntimeError::InvalidBatch))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Batch { transactions })
}

/// Convert `ExecutedBatch` fields into a `BatchResult`.
pub(crate) fn to_batch_result(executed: &ExecutedBatch) -> BatchResult {
    BatchResult {
        read_set_old: executed.read_set.clone(),
        write_set_final: executed.write_set.clone(),
        txs: executed.txs.clone(),
    }
}
