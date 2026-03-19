//! Canonical batch proof-input preparation for runtime-owned proving.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use p3_koala_bear::KoalaBear;
use rayon::prelude::*;

use tabula_artifact::{StateSnapshot, TransactionBatch};
use tabula_commitment::{KoalaBearCodec, PoseidonHasher};
use tabula_core::traits::ValueCodec;
use tabula_core::{Batch, BatchResult, ColId, InMemoryStaticTables, RowKey, TableId};
use tabula_witness::BatchInputPreparer;
use tabula_witness::trace::builtin::PropertyReadRecord;
use tabula_witness::trace::builtin::lowering::LoweringOutput;

use crate::columns::{BatchProofInput, ColumnTransitionBackend, ColumnTransitionInput};
use crate::error::RuntimeError;
use crate::execute::ExecutedBatch;
use crate::program::RuntimeProgram;

type EncodedColumnEntries = BTreeMap<(TableId, ColId), Vec<(RowKey, Vec<KoalaBear>)>>;

struct PlannedColumnProofInput {
    table: TableId,
    col: ColId,
    value_type: tabula_core::ValueType,
    old_entries: Vec<(RowKey, Vec<KoalaBear>)>,
    init_rows: Vec<tabula_witness::InitRow>,
    access_rows: Vec<tabula_witness::AccessRow>,
    writes: tabula_witness::prepare::EncodedColumnWrites,
    is_touched: bool,
    property_reads: Vec<PropertyReadRecord>,
    backend: Arc<dyn ColumnTransitionBackend>,
}

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

    let preparer = BatchInputPreparer::new(PoseidonHasher::new());
    let mut prepared = preparer
        .prepare_execution_inputs(
            batch_result,
            runtime_program.schemas_by_id(),
            runtime_program.column_plans().keys(),
        )
        .map_err(|e| RuntimeError::WitnessGeneration {
            detail: e.to_string(),
        })?;

    let planned_inputs = build_planned_column_inputs(
        runtime_program,
        &property_reads,
        &mut old_entries_by_col,
        &mut prepared,
    )?;

    let columns: Vec<_> = planned_inputs
        .into_par_iter()
        .map(|input| {
            let backend_name = input.backend.name().to_string();
            input
                .backend
                .build_proof_input(
                    ColumnTransitionInput {
                        table: input.table,
                        col: input.col,
                        value_type: input.value_type,
                        old_entries: input.old_entries,
                        init_rows: input.init_rows,
                        access_rows: input.access_rows,
                        writes: input.writes,
                        is_touched: input.is_touched,
                    },
                    &input.property_reads,
                )
                .map_err(|e| RuntimeError::WitnessGeneration {
                    detail: format!(
                        "column ({}, {}) transition backend '{}': {e}",
                        input.table.0, input.col.0, backend_name,
                    ),
                })
        })
        .collect::<Result<Vec<_>, RuntimeError>>()?;

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

fn build_planned_column_inputs(
    runtime_program: &RuntimeProgram,
    property_reads: &BTreeMap<(TableId, ColId), Vec<PropertyReadRecord>>,
    old_entries_by_col: &mut EncodedColumnEntries,
    prepared: &mut tabula_witness::PreparedExecutionInputs,
) -> Result<Vec<PlannedColumnProofInput>, RuntimeError> {
    runtime_program
        .column_plans()
        .iter()
        .map(|(&(table, col), _plan)| {
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
            let backend = runtime_program
                .transition_backends()
                .get(&(table, col))
                .cloned()
                .ok_or_else(|| RuntimeError::ValidationFailed {
                    detail: format!(
                        "missing column transition backend for table {} col {}",
                        table.0, col.0
                    ),
                })?;

            Ok(PlannedColumnProofInput {
                table,
                col,
                value_type,
                old_entries,
                init_rows: prepared
                    .init_rows_by_col
                    .remove(&(table, col))
                    .unwrap_or_default(),
                access_rows: prepared
                    .access_rows_by_col
                    .remove(&(table, col))
                    .unwrap_or_default(),
                writes: prepared
                    .writes_by_col
                    .remove(&(table, col))
                    .unwrap_or_default(),
                is_touched: prepared.written_columns.contains(&(table, col)),
                property_reads: property_reads
                    .get(&(table, col))
                    .cloned()
                    .unwrap_or_default(),
                backend,
            })
        })
        .collect()
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

#[cfg(test)]
mod tests {
    use tabula_chips::shards::shared::{SHARED_COLUMN_WITNESS_LABEL, SharedColumnWitness};
    use tabula_chips::shards::smt_state::{SMT_STATE_WITNESS_LABEL, SmtStateWitness};
    use tabula_core::ColId;
    use tabula_testing::exec::compiled_program_from_source;
    use tabula_testing::fixtures::cases::{
        liquid_shielded_bump_runtime_case, peek_runtime_case, shielded_peek_runtime_case,
    };
    use tabula_testing::fixtures::state::empty_state;

    use crate::TabulaRuntime;
    use crate::proving::prepare_witness_artifacts;

    #[test]
    fn ssmc_read_only_column_keeps_commitment_and_untouched_meta() {
        let case = peek_runtime_case();
        let runtime = TabulaRuntime::builder(compiled_program_from_source(case.source))
            .build()
            .expect("build runtime");
        let state = case.state;
        let batch = case.batch;
        let executed = runtime.execute(&state, &batch).expect("execution succeeds");
        let artifacts =
            prepare_witness_artifacts(runtime.runtime_program(), &state, &batch, &executed)
                .expect("witness artifacts");

        let column = &artifacts.proof_input.columns[0];
        assert!(!column.meta.is_touched);
        assert_eq!(column.meta.com_new, column.meta.com_old);
        assert_eq!(column.meta.is_empty_new, column.meta.is_empty_old);

        let shared = column
            .witness_store
            .get::<SharedColumnWitness>(SHARED_COLUMN_WITNESS_LABEL)
            .expect("shared witness");
        let meta_row = shared.meta_row.as_ref().expect("meta row");
        assert!(!meta_row.is_touched);
        assert_eq!(meta_row.empty_read_count, 0);
    }

    #[test]
    fn empty_read_only_ssmc_column_preserves_empty_state_and_records_empty_read() {
        let case = peek_runtime_case();
        let runtime = TabulaRuntime::builder(compiled_program_from_source(case.source))
            .build()
            .expect("build runtime");
        let state = empty_state();
        let batch = case.batch;
        let executed = runtime.execute(&state, &batch).expect("execution succeeds");
        let artifacts =
            prepare_witness_artifacts(runtime.runtime_program(), &state, &batch, &executed)
                .expect("witness artifacts");

        let column = &artifacts.proof_input.columns[0];
        assert!(!column.meta.is_touched);
        assert!(column.meta.is_empty_old);
        assert!(column.meta.is_empty_new);
        assert_eq!(column.meta.com_new, column.meta.com_old);

        let shared = column
            .witness_store
            .get::<SharedColumnWitness>(SHARED_COLUMN_WITNESS_LABEL)
            .expect("shared witness");
        let meta_row = shared.meta_row.as_ref().expect("meta row");
        assert!(!meta_row.is_touched);
        assert_eq!(meta_row.empty_read_count, 1);
    }

    #[test]
    fn smt_read_only_column_uses_trivial_no_write_semantics() {
        let case = shielded_peek_runtime_case();
        let runtime = TabulaRuntime::builder(compiled_program_from_source(case.source))
            .build()
            .expect("build runtime");
        let state = case.state;
        let batch = case.batch;
        let executed = runtime.execute(&state, &batch).expect("execution succeeds");
        let artifacts =
            prepare_witness_artifacts(runtime.runtime_program(), &state, &batch, &executed)
                .expect("witness artifacts");

        let column = &artifacts.proof_input.columns[0];
        assert!(!column.meta.is_touched);
        assert_eq!(column.meta.com_new, column.meta.com_old);
        assert_eq!(column.meta.is_empty_new, column.meta.is_empty_old);

        let witness = column
            .witness_store
            .get::<SmtStateWitness<3>>(SMT_STATE_WITNESS_LABEL)
            .expect("smt state witness");
        assert!(!witness.column_is_touched);
        assert_eq!(witness.column_new_root, witness.column_old_root);
        assert_eq!(witness.paths.len(), 1);
    }

    #[test]
    fn mixed_read_only_and_write_columns_preserve_per_column_touched_semantics() {
        let case = liquid_shielded_bump_runtime_case();
        let runtime = TabulaRuntime::builder(compiled_program_from_source(case.source))
            .build()
            .expect("build runtime");
        let state = case.state;
        let batch = case.batch;
        let executed = runtime.execute(&state, &batch).expect("execution succeeds");
        let artifacts =
            prepare_witness_artifacts(runtime.runtime_program(), &state, &batch, &executed)
                .expect("witness artifacts");

        let liquid = artifacts
            .proof_input
            .columns
            .iter()
            .find(|column| column.col == ColId(0))
            .expect("liquid column");
        assert!(!liquid.meta.is_touched);
        assert_eq!(liquid.meta.com_new, liquid.meta.com_old);

        let shielded = artifacts
            .proof_input
            .columns
            .iter()
            .find(|column| column.col == ColId(1))
            .expect("shielded column");
        assert!(shielded.meta.is_touched);
        assert_ne!(shielded.meta.com_new, shielded.meta.com_old);
    }
}
