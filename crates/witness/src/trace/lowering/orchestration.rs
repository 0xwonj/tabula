//! Batch-level lowering orchestration.
//!
//! Public entry points for lowering execution results to AIR trace records.

use std::collections::{BTreeMap, BTreeSet};

use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;

use tabula_commitment::BabyBearCodec;
use tabula_core::error::TabulaError;
use tabula_core::traits::{StaticTableProvider, ValueCodec};
use tabula_core::{
    Batch, CellKey, ColId, ExecutionResult, OpKind, TableId, TableSchema, TxOutcome, zero_value,
};
use tabula_ir::Program;

use tabula_chips::execution::MAX_SLOTS;
use tabula_chips::execution::trace::{InstructionRecord, Opcode};
use tabula_chips::static_table::trace::StaticTableRow;

use super::context::LoweringContext;
use super::{build_type_map, lower_tx_body};

// ── Access-level lowering (event-based, no IR) ──────────────────────────────

/// Lower access-level execution events to `ExecutionChip` instruction records.
///
/// This lowering is intentionally fail-closed:
/// - it only supports `Read`/`Write` access events
/// - each `Write` must be explainable by an existing slot value
/// - slot pressure above `MAX_SLOTS` hard-fails
///
/// For richer opcode coverage (arith/hash/lookup), a full instruction witness
/// source is still required.
pub fn lower_execution_records<const W: usize>(
    result: &ExecutionResult,
    schemas: &BTreeMap<TableId, TableSchema>,
) -> Result<Vec<InstructionRecord>, TabulaError> {
    let mut value_types = BTreeMap::new();
    for (&table_id, schema) in schemas {
        for col in &schema.columns {
            value_types.insert((table_id, col.id), col.value_type);
        }
    }

    let codec = BabyBearCodec;
    let mut records = Vec::with_capacity(result.events.len());
    let mut slot_vals = vec![vec![BabyBear::ZERO; W]; MAX_SLOTS];
    let mut slot_nulls = [false; MAX_SLOTS];
    let mut slot_by_key: BTreeMap<CellKey, usize> = BTreeMap::new();
    let mut next_slot = 0usize;
    let mut last_time: Option<u64> = None;

    for (idx, event) in result.events.iter().enumerate() {
        if let Some(prev_time) = last_time
            && event.time < prev_time
        {
            return Err(TabulaError::ProofError {
                phase: "trace_lowering",
                detail: format!(
                    "execution events must be time-ordered: event {idx} has time {} after {}",
                    event.time, prev_time
                ),
            });
        }
        last_time = Some(event.time);

        let value_type = *value_types
            .get(&(event.key.table, event.key.col))
            .ok_or_else(|| TabulaError::ProofError {
                phase: "trace_lowering",
                detail: format!(
                    "missing schema type for event key ({:?}, {:?})",
                    event.key.table, event.key.col
                ),
            })?;

        let encoded = if event.val_is_null {
            codec.encode(&zero_value(value_type))?
        } else {
            if !event.value.matches_type(value_type) {
                return Err(TabulaError::ProofError {
                    phase: "trace_lowering",
                    detail: format!(
                        "event value type mismatch at event {} for key ({:?},{:?},{}): expected {:?}, got {}",
                        idx,
                        event.key.table,
                        event.key.col,
                        event.key.row.0,
                        value_type,
                        event.value.type_name()
                    ),
                });
            }
            codec.encode(&event.value)?
        };
        if encoded.len() != W {
            return Err(TabulaError::ProofError {
                phase: "trace_lowering",
                detail: format!(
                    "lower_execution_records width mismatch at event {}: expected {}, got {}",
                    idx,
                    W,
                    encoded.len()
                ),
            });
        }

        match event.op {
            OpKind::Read => {
                let slot = if let Some(&existing) = slot_by_key.get(&event.key) {
                    existing
                } else if next_slot < MAX_SLOTS {
                    let assigned = next_slot;
                    next_slot += 1;
                    assigned
                } else {
                    return Err(TabulaError::ProofError {
                        phase: "trace_lowering",
                        detail: format!(
                            "execution lowering exhausted {MAX_SLOTS} slots at event {idx} (full instruction witness required)"
                        ),
                    });
                };

                slot_vals[slot] = encoded.clone();
                slot_nulls[slot] = event.val_is_null;
                slot_by_key.insert(event.key, slot);

                records.push(InstructionRecord {
                    opcode: Opcode::Read,
                    tx_index: event.tx_index,
                    effect_ordinal_in_tx: event.effect_ordinal_in_tx,
                    written_slots: vec![slot],
                    src1_val: vec![BabyBear::ZERO; W],
                    src2_val: vec![BabyBear::ZERO; W],
                    cond_val: false,
                    src1_slot_idx: None,
                    src2_slot_idx: None,
                    cond_slot_idx: None,
                    access_t: Some(event.key.table.0),
                    access_c: Some(event.key.col.0),
                    access_r: Some(event.key.row.0),
                    access_val: Some(encoded.clone()),
                    access_is_null: Some(event.val_is_null),
                    dst_val: encoded,
                    dst_is_null: event.val_is_null,
                    dst2_val: vec![],
                    dst2_is_null: false,
                    hash_perm_input: None,
                    hash_perm_output: None,
                    is_empty_col: false,
                    precompile_id: None,
                    property_query_type: None,
                    property_result_val: vec![],
                    property_result_key: vec![],
                    property_result_is_null: false,
                });
            }
            OpKind::Write => {
                let slot = slot_by_key
                    .get(&event.key)
                    .copied()
                    .filter(|&s| slot_vals[s] == encoded && slot_nulls[s] == event.val_is_null)
                    .or_else(|| {
                        (0..MAX_SLOTS)
                            .find(|&s| slot_vals[s] == encoded && slot_nulls[s] == event.val_is_null)
                    })
                    .ok_or_else(|| {
                        TabulaError::ProofError { phase: "trace_lowering", detail: format!(
                            "cannot lower write event at tx={} effect={} key=({:?},{:?},{}): no matching source slot",
                            event.tx_index,
                            event.effect_ordinal_in_tx,
                            event.key.table,
                            event.key.col,
                            event.key.row.0
                        ) }
                    })?;

                slot_by_key.insert(event.key, slot);

                records.push(InstructionRecord {
                    opcode: Opcode::Write,
                    tx_index: event.tx_index,
                    effect_ordinal_in_tx: event.effect_ordinal_in_tx,
                    written_slots: vec![],
                    src1_val: encoded.clone(),
                    src2_val: vec![BabyBear::ZERO; W],
                    cond_val: false,
                    src1_slot_idx: Some(slot),
                    src2_slot_idx: None,
                    cond_slot_idx: None,
                    access_t: Some(event.key.table.0),
                    access_c: Some(event.key.col.0),
                    access_r: Some(event.key.row.0),
                    access_val: Some(encoded),
                    access_is_null: Some(event.val_is_null),
                    dst_val: vec![],
                    dst_is_null: false,
                    dst2_val: vec![],
                    dst2_is_null: false,
                    hash_perm_input: None,
                    hash_perm_output: None,
                    is_empty_col: false,
                    precompile_id: None,
                    property_query_type: None,
                    property_result_val: vec![],
                    property_result_key: vec![],
                    property_result_is_null: false,
                });
            }
        }
    }

    Ok(records)
}

// ── Full IR-based lowering ──────────────────────────────────────────────────

/// Output of full program lowering.
#[derive(Debug, Clone)]
pub struct LoweringOutput {
    /// Instruction records for all opcodes across all successful txs.
    pub instruction_records: Vec<InstructionRecord>,
    /// Static table rows accumulated from Lookup instructions.
    pub static_table_rows: Vec<StaticTableRow>,
}

/// Lower a full batch execution from IR programs.
///
/// Walks each successful tx's IR body, producing `InstructionRecord`s
/// for ALL opcodes and collecting `StaticTableRow` entries from Lookups.
///
/// **Limitation**: All `ValueExpr` operands that require slot linkage
/// (src1/src2/cond) must reference either a `Slot(s)` or a value already
/// present in a slot. `Param`/`Literal` operands will search existing
/// slots for a matching value; if none is found, an error is returned.
#[allow(clippy::too_many_arguments)]
pub fn lower_program_batch<const W: usize>(
    program: &Program,
    batch: &Batch,
    execution_result: &ExecutionResult,
    schemas: &BTreeMap<TableId, TableSchema>,
    static_tables: &dyn StaticTableProvider,
    empty_columns: &BTreeSet<(TableId, ColId)>,
    precompile_executor: Option<&super::context::PrecompileExecuteFn>,
    property_reader: Option<&super::context::PropertyReadFn>,
) -> Result<LoweringOutput, TabulaError> {
    let type_map = build_type_map(schemas);
    let codec = BabyBearCodec;

    // Pre-index events by tx_index.
    let mut events_by_tx: BTreeMap<u32, Vec<&tabula_core::AccessEvent>> = BTreeMap::new();
    for event in &execution_result.events {
        events_by_tx.entry(event.tx_index).or_default().push(event);
    }

    let mut all_records = Vec::new();
    let mut all_static_rows: BTreeMap<(u32, u16, u64), StaticTableRow> = BTreeMap::new();

    for (tx_idx, tx) in batch.transactions.iter().enumerate() {
        let tx_index = tx_idx as u32;

        // Skip failed txs.
        match &execution_result.tx_outcomes[tx_idx] {
            TxOutcome::Success => {}
            TxOutcome::Failed { .. } => continue,
        }

        let tx_def = program.resolve(tx.tx_type)?;
        let tx_events = events_by_tx.get(&tx_index).cloned().unwrap_or_default();

        let mut ctx = LoweringContext::<W>::new(
            tx_index,
            &tx_events,
            &type_map,
            static_tables,
            empty_columns,
            &tx.params,
            &codec,
            tx_def.body.len(),
            precompile_executor,
            property_reader,
        );

        lower_tx_body(&mut ctx, &tx_def.body)?;

        let (records, static_rows) = ctx.into_output();
        all_records.extend(records);

        // Merge static table rows (accumulate multiplicities).
        for row in static_rows {
            let key = (row.table_id, row.col_id, row.row_key);
            all_static_rows
                .entry(key)
                .and_modify(|existing| existing.lookup_mult += row.lookup_mult)
                .or_insert(row);
        }
    }

    Ok(LoweringOutput {
        instruction_records: all_records,
        static_table_rows: all_static_rows.into_values().collect(),
    })
}
