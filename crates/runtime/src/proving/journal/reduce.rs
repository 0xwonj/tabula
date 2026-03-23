use std::collections::BTreeMap;

use rayon::prelude::*;

use tabula_artifact::{TransactionBatch, normalize_state};
use tabula_chips::static_table::trace::StaticTableRow;
use tabula_core::Batch;
use tabula_types::TypeRuntimeRegistry;
use tabula_witness::stark::{LoweringOutput, TxLoweringOutput};

use crate::error::RuntimeError;
use crate::policy::validate_proof_state_surface;

use super::state::{
    build_column_plan_index, build_column_profile_map, build_precompile_plan_index,
    collect_empty_columns, collect_old_entries_by_slot, reduce_init_cells, reduce_writes,
};
use super::tx::build_tx_proof_shard;
use super::types::{
    JournalInput, PreparedBatchPlanContext, ProofColumnSlot, ProofJournal, TxProofProjectionContext,
};

/// Convert an artifact `TransactionBatch` into a core `Batch`.
pub(crate) fn convert_batch(
    batch_file: &TransactionBatch,
    type_runtimes: &TypeRuntimeRegistry,
) -> Result<Batch, RuntimeError> {
    let transactions = batch_file
        .transactions
        .iter()
        .map(|t| {
            t.to_transaction(type_runtimes)
                .map_err(RuntimeError::InvalidBatch)
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Batch { transactions })
}

/// Build the canonical runtime-owned prepared proof journal from execution truth.
pub(crate) fn build_proof_journal(input: JournalInput<'_>) -> Result<ProofJournal, RuntimeError> {
    let normalized_state = normalize_state(input.state).map_err(RuntimeError::InvalidState)?;
    validate_proof_state_surface(input.resolved_program, &normalized_state)?;

    let column_slots = input.resolved_program.proof_plan().column_slots();
    let precompile_slots = input.resolved_program.proof_plan().precompile_slots();
    let column_index = build_column_plan_index(column_slots)?;
    let precompile_index = build_precompile_plan_index(precompile_slots)?;
    let column_profiles = build_column_profile_map(input.resolved_program, column_slots)?;
    let plan_ctx = PreparedBatchPlanContext {
        column_slots,
        column_index: &column_index,
        column_profiles: &column_profiles,
    };
    let old_entries_by_slot =
        collect_old_entries_by_slot(&plan_ctx, input.resolved_program, &normalized_state)?;
    if old_entries_by_slot.len() != column_slots.len() {
        return Err(RuntimeError::ValidationFailed {
            detail: format!(
                "old-entry slot count {} does not match proof slot count {}",
                old_entries_by_slot.len(),
                column_slots.len(),
            ),
        });
    }
    let empty_columns = collect_empty_columns(column_slots, &old_entries_by_slot);

    let mut columns = column_slots
        .iter()
        .zip(old_entries_by_slot)
        .map(|(slot, old_entries)| {
            let profile = column_profiles
                .get(&(slot.table, slot.col))
                .ok_or_else(|| RuntimeError::ValidationFailed {
                    detail: format!(
                        "missing sealed type/encoding profile for proof slot ({}, {})",
                        slot.table.0, slot.col.0,
                    ),
                })?;
            Ok(ProofColumnSlot {
                table: slot.table,
                col: slot.col,
                type_id: profile.type_id,
                encoding_profile_id: profile.encoding_profile_id,
                old_entries,
                init_cells: Vec::new(),
                access_events: Vec::new(),
                writes: Vec::new(),
                property_reads: Vec::new(),
            })
        })
        .collect::<Result<Vec<_>, RuntimeError>>()?;

    reduce_init_cells(
        &mut columns,
        &column_index,
        input.execution_journal,
        input.resolved_program.type_runtimes(),
    )?;
    reduce_writes(&mut columns, &column_index, input.execution_journal)?;

    let shards = input
        .execution_journal
        .txs
        .par_iter()
        .filter_map(|record| match record {
            tabula_executor::TxExecutionOutcome::Success(success) => Some(success),
            tabula_executor::TxExecutionOutcome::Failed(_) => None,
        })
        .map(|success| {
            build_tx_proof_shard(
                &TxProofProjectionContext {
                    resolved_program: input.resolved_program,
                    batch: input.batch,
                    column_profiles: &column_profiles,
                    column_index: &column_index,
                    precompile_index: &precompile_index,
                    precompile_slots,
                    static_tables: input.static_tables,
                    empty_columns: &empty_columns,
                },
                success,
            )
        })
        .collect::<Result<Vec<_>, RuntimeError>>()?;

    let mut ordered_shards = shards;
    ordered_shards.sort_by_key(|shard| shard.tx_index);

    let lowering = merge_lowering_outputs(ordered_shards.iter().map(|shard| &shard.lowering));
    let mut precompile_calls_by_slot = vec![Vec::new(); precompile_slots.len()];
    let mut precompile_transcript_calls = Vec::new();

    for shard in ordered_shards {
        for (slot_idx, events) in shard.access_events_by_slot.into_iter().enumerate() {
            columns[slot_idx].access_events.extend(events);
        }
        for (slot_idx, claims) in shard.property_reads_by_slot.into_iter().enumerate() {
            columns[slot_idx].property_reads.extend(claims);
        }
        for (slot_idx, calls) in shard.precompile_calls_by_slot.into_iter().enumerate() {
            precompile_calls_by_slot[slot_idx].extend(calls);
        }
        precompile_transcript_calls.extend(shard.precompile_transcript_calls);
    }

    for column in &mut columns {
        column.init_cells.sort_by_key(|cell| cell.key.row);
        column.writes.sort_by_key(|write| write.row);
    }
    precompile_transcript_calls.sort_by_key(|call| {
        (
            call.header.tx_index,
            call.header.instruction_index,
            call.header.precompile_id,
        )
    });

    Ok(ProofJournal {
        lowering,
        columns,
        precompile_calls_by_slot,
        precompile_transcript_calls,
    })
}

fn merge_lowering_outputs<'a>(
    outputs: impl IntoIterator<Item = &'a TxLoweringOutput>,
) -> LoweringOutput {
    let mut instruction_records = Vec::new();
    let mut static_rows: BTreeMap<(u32, u16, u64), StaticTableRow> = BTreeMap::new();
    let mut ir_hash_calls = Vec::new();

    for output in outputs {
        instruction_records.extend(output.instruction_records.iter().cloned());
        ir_hash_calls.extend(output.ir_hash_calls.iter().cloned());
        for row in &output.static_table_rows {
            let key = (row.table_id, row.col_id, row.row_key);
            static_rows
                .entry(key)
                .and_modify(|existing| existing.lookup_mult += row.lookup_mult)
                .or_insert_with(|| row.clone());
        }
    }

    LoweringOutput {
        instruction_records,
        static_table_rows: static_rows.into_values().collect(),
        ir_hash_calls,
    }
}
