use std::collections::BTreeMap;

use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;

use tabula_commitment::BabyBearCodec;
use tabula_core::error::TabulaError;
use tabula_core::traits::ValueCodec;
use tabula_core::{CellKey, ExecutionResult, OpKind, TableId, TableSchema, zero_value};

use crate::air::chips::execution::trace::InstructionRecord;
use crate::air::chips::execution::{MAX_SLOTS, Opcode};

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
            return Err(TabulaError::ConsistencyError(format!(
                "execution events must be time-ordered: event {idx} has time {} after {}",
                event.time, prev_time
            )));
        }
        last_time = Some(event.time);

        let value_type = *value_types
            .get(&(event.key.table, event.key.col))
            .ok_or_else(|| {
                TabulaError::ConsistencyError(format!(
                    "missing schema type for event key ({:?}, {:?})",
                    event.key.table, event.key.col
                ))
            })?;

        let encoded = if event.val_is_null {
            codec.encode(&zero_value(value_type))?
        } else {
            if !event.value.matches_type(value_type) {
                return Err(TabulaError::ConsistencyError(format!(
                    "event value type mismatch at event {} for key ({:?},{:?},{}): expected {:?}, got {}",
                    idx,
                    event.key.table,
                    event.key.col,
                    event.key.row.0,
                    value_type,
                    event.value.type_name()
                )));
            }
            codec.encode(&event.value)?
        };
        if encoded.len() != W {
            return Err(TabulaError::ConsistencyError(format!(
                "lower_execution_records width mismatch at event {}: expected {}, got {}",
                idx,
                W,
                encoded.len()
            )));
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
                    return Err(TabulaError::ConsistencyError(format!(
                        "execution lowering exhausted {} slots at event {} (full instruction witness required)",
                        MAX_SLOTS, idx
                    )));
                };

                slot_vals[slot] = encoded.clone();
                slot_nulls[slot] = event.val_is_null;
                slot_by_key.insert(event.key, slot);

                records.push(InstructionRecord {
                    opcode: Opcode::Read,
                    tx_index: event.tx_index,
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
                        TabulaError::ConsistencyError(format!(
                            "cannot lower write event at tx={} effect={} key=({:?},{:?},{}): no matching source slot",
                            event.tx_index,
                            event.effect_ordinal_in_tx,
                            event.key.table,
                            event.key.col,
                            event.key.row.0
                        ))
                    })?;

                slot_by_key.insert(event.key, slot);

                records.push(InstructionRecord {
                    opcode: Opcode::Write,
                    tx_index: event.tx_index,
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
                });
            }
        }
    }

    Ok(records)
}
