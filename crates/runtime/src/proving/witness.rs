use std::collections::BTreeMap;

use tabula_artifact::{StateSnapshot, TransactionBatch};
use tabula_commitment::{ColumnState, KoalaBearCodec, PoseidonHasher};
use tabula_core::traits::ValueCodec;
use tabula_core::{Batch, BatchResult, ColId, RowKey, TableId, TableSchema};
use tabula_machine::{ColumnIdentity, ProofTraces, PublicStatement};
use tabula_witness::{BatchWitness, WitnessGenerator};

use crate::error::RuntimeError;
use crate::execute::ExecutedBatch;
use crate::program::RuntimeProgram;

type EncodedColumnEntries =
    BTreeMap<(TableId, ColId), Vec<(RowKey, Vec<p3_koala_bear::KoalaBear>)>>;

/// Build `old_column_states` from state file cells and schemas.
///
/// Enumerates ALL schema columns (not just those with data) to ensure
/// empty columns get proper commitments.
#[tracing::instrument(skip_all, fields(col_count))]
pub fn build_old_column_states(
    runtime_program: &RuntimeProgram,
    state_file: &StateSnapshot,
) -> Result<BTreeMap<(TableId, ColId), ColumnState<PoseidonHasher>>, RuntimeError> {
    let codec = KoalaBearCodec;
    let hasher = PoseidonHasher::new();

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

    let mut result = BTreeMap::new();
    for schema in runtime_program.schemas_by_id().values() {
        for col_def in &schema.columns {
            let Some(column_plan) = runtime_program.column_plans().get(&(schema.id, col_def.id))
            else {
                return Err(RuntimeError::ValidationFailed {
                    detail: format!(
                        "missing column plan for table {} col {}",
                        schema.id.0, col_def.id.0,
                    ),
                });
            };
            let mut entries = entries_by_col
                .remove(&(schema.id, col_def.id))
                .unwrap_or_default();
            entries.sort_by_key(|(row, _)| *row);
            let (state, _com) = ColumnState::commit(
                &hasher,
                schema.id,
                col_def.id,
                entries,
                column_plan.scheme_id.raw(),
            )
            .map_err(|e| RuntimeError::ColumnState {
                detail: e.to_string(),
            })?;
            result.insert((schema.id, col_def.id), state);
        }
    }

    tracing::Span::current().record("col_count", result.len());
    Ok(result)
}

/// Generate a `BatchWitness` from execution results and column states.
#[tracing::instrument(skip_all)]
pub fn generate_witness(
    batch_result: &BatchResult,
    schemas: &BTreeMap<TableId, TableSchema>,
    old_column_states: &BTreeMap<(TableId, ColId), ColumnState<PoseidonHasher>>,
) -> Result<BatchWitness<PoseidonHasher>, RuntimeError> {
    let hasher = PoseidonHasher::new();
    let wg = WitnessGenerator::new(hasher);
    wg.generate(batch_result, schemas, old_column_states)
        .map_err(|e| RuntimeError::WitnessGeneration {
            detail: e.to_string(),
        })
}

/// Extract column identities for the columns that actually appear in proof traces.
pub fn extract_column_identities(
    witness: &BatchWitness<PoseidonHasher>,
    traces: &ProofTraces,
) -> Result<Vec<ColumnIdentity>, RuntimeError> {
    let identities_by_column: BTreeMap<(TableId, ColId), ColumnIdentity> = witness
        .columns
        .iter()
        .map(|col| {
            (
                (col.table, col.col),
                ColumnIdentity {
                    table_id: col.table.0,
                    col_id: col.col.0,
                    com_old: col.meta.com_old.0,
                    com_new: col.meta.com_new.0,
                },
            )
        })
        .collect();

    traces
        .columns
        .iter()
        .map(|((table_id, col_id), _)| {
            identities_by_column
                .get(&(*table_id, *col_id))
                .copied()
                .ok_or_else(|| RuntimeError::ValidationFailed {
                    detail: format!(
                        "missing column identity for traced table {} col {}",
                        table_id.0, col_id.0,
                    ),
                })
        })
        .collect()
}

/// Build the public statement from witness state roots.
pub fn extract_statement(witness: &BatchWitness<PoseidonHasher>) -> PublicStatement {
    PublicStatement {
        old_root: witness.old_state_root,
        new_root: witness.new_state_root,
    }
}

/// Convert a `TransactionBatch` into a `Batch`.
pub fn convert_batch(batch_file: &TransactionBatch) -> Result<Batch, RuntimeError> {
    let transactions = batch_file
        .transactions
        .iter()
        .map(|t| t.to_transaction().map_err(RuntimeError::InvalidBatch))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Batch { transactions })
}

/// Convert `ExecutedBatch` fields into a `BatchResult`.
pub fn to_batch_result(executed: &ExecutedBatch) -> BatchResult {
    BatchResult {
        read_set_old: executed.read_set.clone(),
        write_set_final: executed.write_set.clone(),
        txs: executed.txs.clone(),
    }
}
