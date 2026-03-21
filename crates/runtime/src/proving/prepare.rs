//! Canonical runtime-owned proof-batch preparation.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use rayon::prelude::*;

use tabula_artifact::{State, TransactionBatch};
use tabula_chips::precompile_transcript::{
    PRECOMPILE_TRANSCRIPT_WITNESS_LABEL, PrecompileTranscriptCall, compute_precompile_call_header,
};
use tabula_commitment::{PoseidonHasher, compute_state_roots_from_metas};
use tabula_core::{Batch, BatchResult, ColId, InMemoryStaticTables, TableId, TxResult, zero_value};
use tabula_ir::Instruction;
use tabula_machine::{ColumnIdentity, PublicStatement};
use tabula_stark::trace::WitnessStore;
use tabula_witness::stark::{SharedStoreBuilder, SharedStoreContext};
use tabula_witness::{
    CommittedEntry, ExecutionInputPreparer, PreparedExecutionColumns, PropertyReadClaim,
};

use crate::error::RuntimeError;
use crate::execute::ExecutedBatch;
use crate::precompile_proofs::{
    PrecompileProofContext, PrecompileProofPreparer, ResolvedPrecompileCall,
};
use crate::program::ResolvedProgram;
use crate::proof_extensions::{ColumnProofContext, ColumnProofPreparer};
use crate::setup::materialize::ColumnProofRecipe;

struct PlannedColumnProofInput {
    table: TableId,
    col: ColId,
    context: ColumnProofContext,
    preparer: Arc<dyn ColumnProofPreparer>,
}

pub(crate) struct ColumnTraceInput {
    pub(crate) identity: ColumnIdentity,
    pub(crate) store: WitnessStore,
}

pub(crate) struct PrecompileProofRecipe {
    pub(crate) descriptor: tabula_artifact::PrecompileDescriptor,
    pub(crate) preparer: Arc<dyn PrecompileProofPreparer>,
}

/// Canonical runtime-owned proving session prepared from one executed batch.
pub(crate) struct PreparedProofBatch {
    pub(crate) air_statement: PublicStatement,
    pub(crate) shared_store: WitnessStore,
    pub(crate) columns: Vec<ColumnTraceInput>,
}

/// Prepare the full runtime-owned proof batch from one executed batch.
pub(crate) fn prepare_proof_batch(
    resolved_program: &ResolvedProgram,
    proof_recipes: &[ColumnProofRecipe],
    precompile_recipes: &[PrecompileProofRecipe],
    state: &State,
    batch_file: &TransactionBatch,
    executed: &ExecutedBatch,
) -> Result<PreparedProofBatch, RuntimeError> {
    let batch = convert_batch(batch_file)?;
    prepare_proof_batch_from_parts(
        resolved_program,
        proof_recipes,
        precompile_recipes,
        state,
        &batch,
        executed.batch_result(),
    )
}

fn prepare_proof_batch_from_parts(
    resolved_program: &ResolvedProgram,
    proof_recipes: &[ColumnProofRecipe],
    precompile_recipes: &[PrecompileProofRecipe],
    state_file: &State,
    batch: &Batch,
    batch_result: &BatchResult,
) -> Result<PreparedProofBatch, RuntimeError> {
    let old_entries_by_col = collect_old_entries_by_column(resolved_program, state_file)?;
    let empty_columns: BTreeSet<(TableId, ColId)> = old_entries_by_col
        .iter()
        .filter_map(|(key, entries)| entries.iter().all(|entry| entry.is_null).then_some(*key))
        .collect();

    let lowering = tabula_witness::stark::lower_program_batch::<3>(
        resolved_program.program(),
        batch,
        batch_result,
        resolved_program.schemas_by_id(),
        &InMemoryStaticTables::new(),
        &empty_columns,
    )
    .map_err(RuntimeError::TraceBuild)?;
    let property_reads = extract_property_read_claims(resolved_program, batch, batch_result)?;

    let preparer = ExecutionInputPreparer::new();
    let planned_columns: Vec<_> = proof_recipes
        .iter()
        .map(|slot| (slot.table, slot.col))
        .collect();
    let prepared = preparer
        .prepare_execution_inputs(
            batch_result,
            resolved_program.schemas_by_id(),
            planned_columns.iter(),
        )
        .map_err(|e| RuntimeError::WitnessGeneration {
            detail: e.to_string(),
        })?;

    let planned_inputs =
        build_planned_column_inputs(proof_recipes, &property_reads, old_entries_by_col, prepared)?;

    let prepared_columns = planned_inputs
        .into_par_iter()
        .map(|planned| {
            let table = planned.table;
            let col = planned.col;
            let preparer_name = planned.preparer.name().to_string();
            let preparer = planned.preparer;
            let proof = preparer
                .prepare_column(planned.context)
                .map_err(RuntimeError::from_extension_proof)
                .map_err(|e| match e {
                    RuntimeError::WitnessGeneration { detail } => RuntimeError::WitnessGeneration {
                        detail: format!(
                            "column ({}, {}) proof preparer '{}': {detail}",
                            table.0, col.0, preparer_name,
                        ),
                    },
                    other => other,
                })?;
            Ok((table, col, proof))
        })
        .collect::<Result<Vec<_>, RuntimeError>>()?;
    let prepared_precompiles = precompile_recipes
        .iter()
        .map(|recipe| {
            let calls = collect_precompile_calls(batch_result, recipe.descriptor.precompile_id)
                .map_err(|e| RuntimeError::WitnessGeneration {
                    detail: e.to_string(),
                })?;
            let context = PrecompileProofContext {
                descriptor: recipe.descriptor.clone(),
                calls,
                binding: resolved_program.binding().clone(),
            };
            recipe
                .preparer
                .prepare_precompile(context)
                .map_err(RuntimeError::from_extension_proof)
                .map_err(|e| match e {
                    RuntimeError::WitnessGeneration { detail } => RuntimeError::WitnessGeneration {
                        detail: format!(
                            "precompile 0x{:04x} proof preparer '{}': {detail}",
                            recipe.descriptor.precompile_id.0,
                            recipe.preparer.name(),
                        ),
                    },
                    other => other,
                })
        })
        .collect::<Result<Vec<_>, RuntimeError>>()?;

    let metas: Vec<_> = prepared_columns
        .iter()
        .map(|(_, _, proof)| proof.meta.clone())
        .collect();
    let hasher = PoseidonHasher::new();
    let (old_state_root, new_state_root) =
        compute_state_roots_from_metas(&hasher, &metas).map_err(RuntimeError::TraceBuild)?;
    let air_statement = PublicStatement {
        old_root: old_state_root,
        new_root: new_state_root,
    };

    let mut shared_store = SharedStoreBuilder::<PoseidonHasher, 3>::new(SharedStoreContext {
        column_metas: &metas,
        old_state_root: &air_statement.old_root,
        new_state_root: &air_statement.new_root,
    })
    .prepare_witness_store(&lowering, PoseidonHasher::new())
    .map_err(RuntimeError::TraceBuild)?;
    let transcript_calls = collect_all_precompile_transcript_calls(batch_result).map_err(|e| {
        RuntimeError::WitnessGeneration {
            detail: e.to_string(),
        }
    })?;
    if !transcript_calls.is_empty() {
        let mut transcript_store = WitnessStore::new();
        transcript_store.put(PRECOMPILE_TRANSCRIPT_WITNESS_LABEL, transcript_calls);
        shared_store
            .merge(transcript_store)
            .map_err(|detail| RuntimeError::WitnessGeneration { detail })?;
    }
    for prepared in prepared_precompiles {
        shared_store
            .merge(prepared.store)
            .map_err(|detail| RuntimeError::WitnessGeneration { detail })?;
    }

    let columns = prepared_columns
        .into_iter()
        .map(|(table, col, proof)| ColumnTraceInput {
            identity: ColumnIdentity {
                table_id: table.0,
                col_id: col.0,
                com_old: proof.meta.com_old.0,
                com_new: proof.meta.com_new.0,
            },
            store: proof.store,
        })
        .collect();

    Ok(PreparedProofBatch {
        air_statement,
        shared_store,
        columns,
    })
}

fn build_planned_column_inputs(
    proof_recipes: &[ColumnProofRecipe],
    property_reads: &BTreeMap<(TableId, ColId), Vec<PropertyReadClaim>>,
    mut old_entries_by_col: BTreeMap<(TableId, ColId), Vec<CommittedEntry>>,
    prepared: PreparedExecutionColumns,
) -> Result<Vec<PlannedColumnProofInput>, RuntimeError> {
    if prepared.columns.len() != proof_recipes.len() {
        return Err(RuntimeError::ValidationFailed {
            detail: format!(
                "prepared execution column count {} does not match proof slot count {}",
                prepared.columns.len(),
                proof_recipes.len()
            ),
        });
    }

    proof_recipes
        .iter()
        .zip(prepared.columns)
        .map(|(slot, column)| {
            let table = slot.table;
            let col = slot.col;
            if column.table != table || column.col != col {
                return Err(RuntimeError::ValidationFailed {
                    detail: format!(
                        "prepared execution column ({}, {}) does not match proof slot ({}, {})",
                        column.table.0, column.col.0, table.0, col.0
                    ),
                });
            }
            if column.value_type != slot.value_type {
                return Err(RuntimeError::ValidationFailed {
                    detail: format!(
                        "prepared execution column ({}, {}) has value type {:?} but proof slot expects {:?}",
                        table.0, col.0, column.value_type, slot.value_type
                    ),
                });
            }
            let old_entries = old_entries_by_col.remove(&(table, col)).ok_or_else(|| {
                RuntimeError::ValidationFailed {
                    detail: format!(
                        "missing old entries for planned column ({}, {})",
                        table.0, col.0
                    ),
                }
            })?;
            Ok(PlannedColumnProofInput {
                table,
                col,
                context: ColumnProofContext {
                    column,
                    old_entries,
                    property_reads: property_reads
                        .get(&(table, col))
                        .cloned()
                        .unwrap_or_default(),
                },
                preparer: Arc::clone(&slot.preparer),
            })
        })
        .collect()
}

fn extract_property_read_claims(
    resolved_program: &ResolvedProgram,
    batch: &Batch,
    batch_result: &BatchResult,
) -> Result<BTreeMap<(TableId, ColId), Vec<PropertyReadClaim>>, RuntimeError> {
    let mut result: BTreeMap<(TableId, ColId), Vec<PropertyReadClaim>> = BTreeMap::new();

    for (tx, tx_result) in batch.transactions.iter().zip(&batch_result.txs) {
        let def = resolved_program
            .program()
            .resolve(tx.tx_type)
            .map_err(RuntimeError::TraceBuild)?;
        let TxResult::Success { property_reads, .. } = tx_result else {
            continue;
        };
        let mut read_idx = 0usize;
        for instr in &def.body {
            let Instruction::PropertyRead {
                table, col, query, ..
            } = instr
            else {
                continue;
            };
            let stored =
                property_reads
                    .get(read_idx)
                    .ok_or_else(|| RuntimeError::ValidationFailed {
                        detail: format!(
                            "missing property-read result {} for tx type {}",
                            read_idx, tx.tx_type.0
                        ),
                    })?;
            read_idx += 1;
            result
                .entry((*table, *col))
                .or_default()
                .push(PropertyReadClaim {
                    query: query.clone(),
                    result: tabula_core::PropertyQueryResult {
                        value: stored.value,
                        key: stored.key,
                        is_null: stored.is_null,
                    },
                });
        }
    }

    Ok(result)
}

fn collect_old_entries_by_column(
    resolved_program: &ResolvedProgram,
    state_file: &State,
) -> Result<BTreeMap<(TableId, ColId), Vec<CommittedEntry>>, RuntimeError> {
    let mut entries_by_col: BTreeMap<(TableId, ColId), Vec<CommittedEntry>> = BTreeMap::new();
    for cell in &state_file.cells {
        let key = (TableId(cell.table), ColId(cell.col));
        let value_type = resolved_program
            .column_plans()
            .get(&key)
            .map(|plan| plan.value_type)
            .ok_or_else(|| RuntimeError::ValidationFailed {
                detail: format!(
                    "missing column plan for table {} col {}",
                    cell.table, cell.col
                ),
            })?;
        entries_by_col.entry(key).or_default().push(CommittedEntry {
            row: tabula_core::RowKey(cell.row),
            value: cell.value.unwrap_or_else(|| zero_value(value_type)),
            is_null: cell.value.is_none(),
        });
    }

    for schema in resolved_program.schemas_by_id().values() {
        for col_def in &schema.columns {
            if !resolved_program
                .column_plans()
                .contains_key(&(schema.id, col_def.id))
            {
                return Err(RuntimeError::ValidationFailed {
                    detail: format!(
                        "missing column plan for table {} col {}",
                        schema.id.0, col_def.id.0
                    ),
                });
            }
            entries_by_col
                .entry((schema.id, col_def.id))
                .or_default()
                .sort_by_key(|entry| entry.row);
        }
    }

    Ok(entries_by_col)
}

fn collect_precompile_calls(
    batch_result: &BatchResult,
    precompile_id: tabula_ir::PrecompileId,
) -> Result<Vec<ResolvedPrecompileCall>, tabula_core::error::TabulaError> {
    batch_result
        .txs
        .iter()
        .filter_map(|tx| match tx {
            TxResult::Success {
                precompile_events, ..
            } => Some(precompile_events.iter()),
            TxResult::Failed { .. } => None,
        })
        .flatten()
        .filter(|event| event.precompile_id == precompile_id.0)
        .map(|event| {
            Ok(ResolvedPrecompileCall {
                event: event.clone(),
                header: compute_precompile_call_header(event).map(|header| {
                    tabula_ext::backend::precompile::PrecompileCallHeader {
                        tx_index: header.tx_index,
                        instruction_index: header.instruction_index,
                        precompile_id: header.precompile_id,
                        input_count: header.input_count,
                        output_count: header.output_count,
                        event_digest: header.event_digest,
                    }
                })?,
            })
        })
        .collect()
}

fn collect_all_precompile_transcript_calls(
    batch_result: &BatchResult,
) -> Result<Vec<PrecompileTranscriptCall>, tabula_core::error::TabulaError> {
    let mut calls = batch_result
        .txs
        .iter()
        .filter_map(|tx| match tx {
            TxResult::Success {
                precompile_events, ..
            } => Some(precompile_events.iter()),
            TxResult::Failed { .. } => None,
        })
        .flatten()
        .map(PrecompileTranscriptCall::from_event)
        .collect::<Result<Vec<_>, _>>()?;
    calls.sort_by_key(|call| (call.header.tx_index, call.header.instruction_index));
    Ok(calls)
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
    use crate::proving::prepare_proof_batch;

    fn column_store(
        prepared: &PreparedProofBatch,
        col_id: ColId,
    ) -> &tabula_stark::trace::WitnessStore {
        prepared
            .columns
            .iter()
            .find_map(|column| (column.identity.col_id == col_id.0).then_some(&column.store))
            .expect("column store")
    }

    use super::PreparedProofBatch;

    #[test]
    fn ssmc_read_only_column_keeps_commitment_and_untouched_meta() {
        let case = peek_runtime_case();
        let runtime = TabulaRuntime::builder(compiled_program_from_source(case.source))
            .build()
            .expect("build runtime");
        let state = case.state;
        let batch = case.batch;
        let executed = runtime.execute(&state, &batch).expect("execution succeeds");
        let prepared = prepare_proof_batch(
            runtime.resolved_program(),
            runtime.proof_recipes(),
            runtime.precompile_recipes(),
            &state,
            &batch,
            &executed,
        )
        .expect("proof batch");

        let identity = prepared.columns[0].identity;
        assert_eq!(identity.com_new, identity.com_old);

        let shared = column_store(&prepared, ColId(0))
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
        let prepared = prepare_proof_batch(
            runtime.resolved_program(),
            runtime.proof_recipes(),
            runtime.precompile_recipes(),
            &state,
            &batch,
            &executed,
        )
        .expect("proof batch");

        let identity = prepared.columns[0].identity;
        assert_eq!(identity.com_new, identity.com_old);

        let shared = column_store(&prepared, ColId(0))
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
        let prepared = prepare_proof_batch(
            runtime.resolved_program(),
            runtime.proof_recipes(),
            runtime.precompile_recipes(),
            &state,
            &batch,
            &executed,
        )
        .expect("proof batch");

        let identity = prepared.columns[0].identity;
        assert_eq!(identity.com_new, identity.com_old);

        let witness = column_store(&prepared, ColId(0))
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
        let prepared = prepare_proof_batch(
            runtime.resolved_program(),
            runtime.proof_recipes(),
            runtime.precompile_recipes(),
            &state,
            &batch,
            &executed,
        )
        .expect("proof batch");

        let liquid = prepared
            .columns
            .iter()
            .map(|column| column.identity)
            .find(|identity| identity.col_id == ColId(0).0)
            .expect("liquid column");
        assert_eq!(liquid.com_new, liquid.com_old);

        let shielded = prepared
            .columns
            .iter()
            .map(|column| column.identity)
            .find(|identity| identity.col_id == ColId(1).0)
            .expect("shielded column");
        assert_ne!(shielded.com_new, shielded.com_old);
    }
}
