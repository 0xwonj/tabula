use std::collections::{BTreeMap, BTreeSet};

use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;

use tabula_commitment::BabyBearCodec;
use tabula_core::error::TabulaError;
use tabula_core::traits::{StaticTableProvider, ValueCodec};
use tabula_core::{
    Batch, CellKey, ColId, ExecutionEvent, ExecutionResult, OpKind, TableId, TableSchema,
    TxOutcome, Value, zero_value,
};
use tabula_ir::{Instruction, Program, ValueExpr};

use crate::air::chips::execution::MAX_SLOTS;
use crate::air::chips::execution::trace::{InstructionRecord, Opcode};
use crate::air::chips::static_table::trace::StaticTableRow;

mod access;
mod arith;
mod cmp;
mod context;
mod control;
mod divmod;
mod hash;
mod logic;
mod lookup;

use context::LoweringContext;

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
pub fn lower_program_batch<const W: usize>(
    program: &Program,
    batch: &Batch,
    execution_result: &ExecutionResult,
    schemas: &BTreeMap<TableId, TableSchema>,
    static_tables: &dyn StaticTableProvider,
    empty_columns: &BTreeSet<(TableId, ColId)>,
) -> Result<LoweringOutput, TabulaError> {
    let type_map = build_type_map(schemas);
    let codec = BabyBearCodec;

    // Pre-index events by tx_index.
    let mut events_by_tx: BTreeMap<u32, Vec<&ExecutionEvent>> = BTreeMap::new();
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

// ── Internal helpers ────────────────────────────────────────────────────────

fn build_type_map(
    schemas: &BTreeMap<TableId, TableSchema>,
) -> BTreeMap<(TableId, ColId), tabula_core::ValueType> {
    let mut type_map = BTreeMap::new();
    for (&table_id, schema) in schemas {
        for col in &schema.columns {
            type_map.insert((table_id, col.id), col.value_type);
        }
    }
    type_map
}

/// Return the highest destination slot index used by an instruction, if any.
fn max_dst_slot(instr: &Instruction) -> Option<usize> {
    match instr {
        Instruction::Read {
            dst_val,
            dst_is_null,
            ..
        } => Some((*dst_val as usize).max(*dst_is_null as usize)),
        Instruction::Arith { dst, .. }
        | Instruction::Cmp { dst, .. }
        | Instruction::Not { dst, .. }
        | Instruction::And { dst, .. }
        | Instruction::Or { dst, .. }
        | Instruction::Select { dst, .. }
        | Instruction::Hash { dst, .. }
        | Instruction::Lookup { dst, .. } => Some(*dst as usize),
        Instruction::DivMod { dst_q, dst_r, .. } => Some((*dst_q as usize).max(*dst_r as usize)),
        Instruction::Write { .. } | Instruction::Assert { .. } | Instruction::Emit { .. } => None,
    }
}

/// Collect unique `(param_index, Value)` pairs that appear as value operands
/// in the instruction list.
fn collect_param_operands(instructions: &[Instruction], params: &[Value]) -> Vec<(u16, Value)> {
    let mut seen = BTreeSet::new();
    let mut result = Vec::new();
    let mut add = |ve: &ValueExpr| {
        if let ValueExpr::Param(p) = ve
            && seen.insert(*p)
            && let Some(&val) = params.get(*p as usize)
        {
            result.push((*p, val));
        }
    };
    for instr in instructions {
        match instr {
            Instruction::Arith { lhs, rhs, .. }
            | Instruction::DivMod { lhs, rhs, .. }
            | Instruction::Cmp { lhs, rhs, .. }
            | Instruction::And { lhs, rhs, .. }
            | Instruction::Or { lhs, rhs, .. } => {
                add(lhs);
                add(rhs);
            }
            Instruction::Not { src, .. } => add(src),
            Instruction::Assert { cond } => add(cond),
            Instruction::Select {
                cond,
                if_true,
                if_false,
                ..
            } => {
                add(cond);
                add(if_true);
                add(if_false);
            }
            Instruction::Hash { inputs, .. } => {
                for inp in inputs {
                    add(inp);
                }
            }
            Instruction::Write { src_val, .. } => add(src_val),
            _ => {}
        }
    }
    result
}

/// Pre-materialize param values into dedicated slots.
fn pre_materialize_params<const W: usize>(
    ctx: &mut LoweringContext<'_, W>,
    instructions: &[Instruction],
) -> Result<(), TabulaError> {
    let param_operands = collect_param_operands(instructions, ctx.params);
    if param_operands.is_empty() {
        return Ok(());
    }

    // Find highest slot index used by instruction destinations to avoid conflicts.
    let ir_max = instructions
        .iter()
        .filter_map(max_dst_slot)
        .max()
        .map(|m| m + 1)
        .unwrap_or(0);
    ctx.max_slot = ctx.max_slot.max(ir_max);

    // Reserve a zero-valued slot (never written) for the src2 operand of synthetic loads.
    let zero_slot = ctx.max_slot;
    ctx.slot_initialized[zero_slot] = true;
    ctx.max_slot += 1;

    let mut initialized_slots: Vec<usize> = vec![zero_slot];

    for (_p, val) in &param_operands {
        let enc = ctx.encode_padded(val)?;
        // Skip if this value is already in an explicitly initialized slot.
        if initialized_slots
            .iter()
            .any(|&s| ctx.slot_fes[s] == enc && !ctx.slot_nulls[s])
        {
            continue;
        }
        let slot = ctx.max_slot;
        if slot >= MAX_SLOTS {
            return Err(TabulaError::ConsistencyError(format!(
                "cannot pre-materialize param: slot {} >= MAX_SLOTS ({})",
                slot, MAX_SLOTS
            )));
        }
        ctx.slots[slot] = Some(*val);
        ctx.slot_fes[slot] = enc.clone();
        ctx.slot_nulls[slot] = false;
        ctx.slot_initialized[slot] = true;
        ctx.max_slot = slot + 1;
        initialized_slots.push(slot);

        // Synthetic Add record: slot = param_val + 0.
        let mut rec = ctx.empty_record(Opcode::Add);
        rec.written_slots = vec![slot];
        rec.src1_val = enc.clone();
        rec.src2_val = vec![BabyBear::ZERO; W];
        rec.src1_slot_idx = Some(slot); // self-referential: src1 reads from the slot we write
        rec.src2_slot_idx = Some(zero_slot);
        rec.dst_val = enc;
        rec.dst_is_null = false;
        ctx.push_record(rec);
    }

    Ok(())
}

/// Lower one transaction's IR body into instruction records.
fn lower_tx_body<const W: usize>(
    ctx: &mut LoweringContext<'_, W>,
    instructions: &[Instruction],
) -> Result<(), TabulaError> {
    pre_materialize_params(ctx, instructions)?;

    for (instr_idx, instr) in instructions.iter().enumerate() {
        match instr {
            Instruction::Read {
                dst_val,
                dst_is_null: _,
                table,
                col,
                row,
            } => access::lower_read(ctx, *dst_val, *table, *col, row, instr_idx)?,

            Instruction::Write {
                table,
                col,
                row,
                src_val,
                src_is_null: _,
            } => access::lower_write(ctx, *table, *col, row, src_val, instr_idx)?,

            Instruction::Arith { dst, op, lhs, rhs } => {
                arith::lower_arith(ctx, *dst, op, lhs, rhs)?;
            }

            Instruction::DivMod {
                dst_q,
                dst_r,
                lhs,
                rhs,
            } => divmod::lower_divmod(ctx, *dst_q, *dst_r, lhs, rhs)?,

            Instruction::Cmp { dst, op, lhs, rhs } => {
                cmp::lower_cmp(ctx, *dst, op, lhs, rhs)?;
            }

            Instruction::Not { dst, src } => logic::lower_not(ctx, *dst, src)?,

            Instruction::And { dst, lhs, rhs } => logic::lower_and(ctx, *dst, lhs, rhs)?,

            Instruction::Or { dst, lhs, rhs } => logic::lower_or(ctx, *dst, lhs, rhs)?,

            Instruction::Assert { cond } => control::lower_assert(ctx, cond)?,

            Instruction::Select {
                dst,
                cond,
                if_true,
                if_false,
            } => control::lower_select(ctx, *dst, cond, if_true, if_false)?,

            Instruction::Hash { dst, inputs } => hash::lower_hash(ctx, *dst, inputs)?,

            Instruction::Lookup {
                dst,
                static_table,
                col,
                row,
            } => lookup::lower_lookup(ctx, *dst, *static_table, *col, row)?,

            Instruction::Emit { .. } => {
                // Out-of-protocol; skip.
            }
        }
    }

    Ok(())
}
