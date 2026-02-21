use std::collections::{BTreeMap, BTreeSet};

use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;

use tabula_commitment::BabyBearCodec;
use tabula_core::error::TabulaError;
use tabula_core::traits::{StaticTableProvider, ValueCodec};
use tabula_core::{
    Batch, CellKey, ColId, ExecutionEvent, ExecutionResult, OpKind, RowKey, TableId, TableSchema,
    TxOutcome, Value, zero_value,
};
use tabula_ir::{Instruction, Program, RowExpr, ValueExpr};

use crate::air::chips::execution::trace::{CmpOp, InstructionRecord, Opcode};
use crate::air::chips::execution::{
    HASH_INSTRUCTION_DOMAIN_TAG, HASH_INSTRUCTION_INPUT_COUNT, MAX_SLOTS,
};
use crate::air::chips::poseidon::constants::poseidon2_permutation;
use crate::air::chips::static_table::trace::StaticTableRow;

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

        let (records, static_rows) = lower_tx_body::<W>(
            &tx_def.body,
            &tx.params,
            tx_index,
            &tx_events,
            &type_map,
            static_tables,
            empty_columns,
            &codec,
        )?;

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

/// Resolve a `RowExpr` to a concrete `RowKey`.
fn resolve_row(
    expr: &RowExpr,
    slots: &[Option<Value>],
    params: &[Value],
) -> Result<RowKey, TabulaError> {
    match expr {
        RowExpr::Literal(rk) => Ok(*rk),
        RowExpr::Slot(s) => {
            let v = slots
                .get(*s as usize)
                .and_then(|o| o.as_ref())
                .ok_or_else(|| TabulaError::SlotOutOfBounds {
                    index: *s,
                    max: slots.len().saturating_sub(1) as u16,
                })?;
            match v {
                Value::U64(n) => Ok(RowKey(*n)),
                _ => Err(TabulaError::TypeMismatch {
                    expected: "U64",
                    actual: v.type_name(),
                }),
            }
        }
        RowExpr::Param(p) => {
            let v = params
                .get(*p as usize)
                .ok_or(TabulaError::ParamOutOfBounds {
                    index: *p,
                    max: params.len().saturating_sub(1) as u16,
                })?;
            match v {
                Value::U64(n) => Ok(RowKey(*n)),
                _ => Err(TabulaError::TypeMismatch {
                    expected: "U64",
                    actual: v.type_name(),
                }),
            }
        }
    }
}

/// Resolve a `ValueExpr` to a concrete `Value`.
fn resolve_val(
    expr: &ValueExpr,
    slots: &[Option<Value>],
    params: &[Value],
) -> Result<Value, TabulaError> {
    match expr {
        ValueExpr::Literal(v) => Ok(*v),
        ValueExpr::Slot(s) => {
            slots
                .get(*s as usize)
                .and_then(|o| *o)
                .ok_or(TabulaError::SlotOutOfBounds {
                    index: *s,
                    max: slots.len().saturating_sub(1) as u16,
                })
        }
        ValueExpr::Param(p) => {
            params
                .get(*p as usize)
                .copied()
                .ok_or(TabulaError::ParamOutOfBounds {
                    index: *p,
                    max: params.len().saturating_sub(1) as u16,
                })
        }
    }
}

/// Get slot index from a `ValueExpr`, searching existing slots for a matching value.
///
/// For `Param` and `Literal` operands, searches all populated slots for one whose
/// encoded value matches. Param/Literal values must be pre-materialized into slots
/// (via synthetic Add records at the start of the tx body) before any instruction
/// that references them.
fn resolve_slot_idx(
    expr: &ValueExpr,
    encoded: &[BabyBear],
    is_null: bool,
    slot_fes: &[Vec<BabyBear>],
    slot_nulls: &[bool],
    max_slot: usize,
) -> Result<Option<usize>, TabulaError> {
    match expr {
        ValueExpr::Slot(s) => Ok(Some(*s as usize)),
        ValueExpr::Param(_) | ValueExpr::Literal(_) => {
            let found =
                (0..max_slot).find(|&s| slot_fes[s] == encoded && slot_nulls[s] == is_null);
            if let Some(idx) = found {
                return Ok(Some(idx));
            }
            Err(TabulaError::ConsistencyError(format!(
                "no slot contains the required operand value (param/literal); \
                 the current AIR requires all operands to come from slots"
            )))
        }
    }
}

/// Encode a `Value` and pad to exactly `W` field elements.
fn encode_padded<const W: usize>(
    value: &Value,
    codec: &BabyBearCodec,
) -> Result<Vec<BabyBear>, TabulaError> {
    let mut fes = codec.encode(value)?;
    fes.resize(W, BabyBear::ZERO);
    Ok(fes)
}

fn map_ir_cmp_op(op: &tabula_ir::CmpOp) -> CmpOp {
    match op {
        tabula_ir::CmpOp::Eq => CmpOp::Eq,
        tabula_ir::CmpOp::Ne => CmpOp::Ne,
        tabula_ir::CmpOp::Lt => CmpOp::Lt,
        tabula_ir::CmpOp::Lte => CmpOp::Lte,
        tabula_ir::CmpOp::Gt => CmpOp::Gt,
        tabula_ir::CmpOp::Gte => CmpOp::Gte,
    }
}

/// Default InstructionRecord with zero/empty fields.
fn empty_record<const W: usize>(opcode: Opcode, tx_index: u32) -> InstructionRecord {
    InstructionRecord {
        opcode,
        tx_index,
        written_slots: vec![],
        src1_val: vec![BabyBear::ZERO; W],
        src2_val: vec![BabyBear::ZERO; W],
        cond_val: false,
        src1_slot_idx: None,
        src2_slot_idx: None,
        cond_slot_idx: None,
        access_t: None,
        access_c: None,
        access_r: None,
        access_val: None,
        access_is_null: None,
        dst_val: vec![],
        dst_is_null: false,
        dst2_val: vec![],
        dst2_is_null: false,
        hash_perm_input: None,
        hash_perm_output: None,
        is_empty_col: false,
    }
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
        Instruction::DivMod { dst_q, dst_r, .. } => {
            Some((*dst_q as usize).max(*dst_r as usize))
        }
        Instruction::Write { .. } | Instruction::Assert { .. } | Instruction::Emit { .. } => None,
    }
}

/// Collect unique `(param_index, Value)` pairs that appear as value operands
/// in the instruction list. These need pre-materialized slots for AIR linkage.
fn collect_param_operands(instructions: &[Instruction], params: &[Value]) -> Vec<(u16, Value)> {
    let mut seen = BTreeSet::new();
    let mut result = Vec::new();
    let mut add = |ve: &ValueExpr| {
        if let ValueExpr::Param(p) = ve {
            if seen.insert(*p) {
                if let Some(&val) = params.get(*p as usize) {
                    result.push((*p, val));
                }
            }
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

/// Lower one transaction's IR body into instruction records.
#[allow(clippy::too_many_arguments)]
fn lower_tx_body<const W: usize>(
    instructions: &[Instruction],
    params: &[Value],
    tx_index: u32,
    tx_events: &[&ExecutionEvent],
    type_map: &BTreeMap<(TableId, ColId), tabula_core::ValueType>,
    static_tables: &dyn StaticTableProvider,
    empty_columns: &BTreeSet<(TableId, ColId)>,
    codec: &BabyBearCodec,
) -> Result<(Vec<InstructionRecord>, Vec<StaticTableRow>), TabulaError> {
    let mut records = Vec::with_capacity(instructions.len());
    let mut static_rows = Vec::new();

    // Slot state (Value-level for resolution).
    let mut slots: Vec<Option<Value>> = vec![None; MAX_SLOTS];
    // Encoded slot state (BabyBear FEs for trace).
    let mut slot_fes: Vec<Vec<BabyBear>> = vec![vec![BabyBear::ZERO; W]; MAX_SLOTS];
    // Slot null flags.
    let mut slot_nulls: Vec<bool> = vec![false; MAX_SLOTS];
    // Next available slot (for tracking how many are in use).
    let mut max_slot: usize = 0;

    // Effect ordinal counter (increments on Read/Write).
    let mut effect_ordinal: u32 = 0;

    // --- Phase 0: Pre-materialize param values into dedicated slots ---
    //
    // The DSL compiler may use alias binding for params, producing ValueExpr::Param(p)
    // references in instructions without allocating SSA slots. The AIR requires all
    // operand values to reside in slots (operand-to-slot linkage constraint). We solve
    // this by generating synthetic Add(param + 0) records that establish param values in
    // slots before any real instruction. The operand selector for src1 is self-referential
    // (points to the destination slot), which the AIR accepts because slot values in the
    // trace reflect the post-write state.
    let param_operands = collect_param_operands(instructions, params);
    if !param_operands.is_empty() {
        // Find highest slot index used by instruction destinations to avoid conflicts.
        let ir_max = instructions
            .iter()
            .filter_map(|i| max_dst_slot(i))
            .max()
            .map(|m| m + 1)
            .unwrap_or(0);
        max_slot = max_slot.max(ir_max);

        // Reserve a zero-valued slot (never written) for the src2 operand of synthetic loads.
        let zero_slot = max_slot;
        max_slot += 1;

        for (_p, val) in &param_operands {
            let enc = encode_padded::<W>(val, codec)?;
            // Skip if this value is already in a slot (dedup by encoded value).
            if (0..max_slot).any(|s| slot_fes[s] == enc && !slot_nulls[s]) {
                continue;
            }
            let slot = max_slot;
            if slot >= MAX_SLOTS {
                return Err(TabulaError::ConsistencyError(format!(
                    "cannot pre-materialize param: slot {} >= MAX_SLOTS ({})",
                    slot, MAX_SLOTS
                )));
            }
            slots[slot] = Some(*val);
            slot_fes[slot] = enc.clone();
            slot_nulls[slot] = false;
            max_slot = slot + 1;

            // Synthetic Add record: slot = param_val + 0.
            let mut rec = empty_record::<W>(Opcode::Add, tx_index);
            rec.written_slots = vec![slot];
            rec.src1_val = enc.clone();
            rec.src2_val = vec![BabyBear::ZERO; W];
            rec.src1_slot_idx = Some(slot); // self-referential: src1 reads from the slot we write
            rec.src2_slot_idx = Some(zero_slot);
            rec.dst_val = enc;
            rec.dst_is_null = false;
            records.push(rec);
        }
    }

    for (instr_idx, instr) in instructions.iter().enumerate() {
        match instr {
            Instruction::Read {
                dst_val,
                dst_is_null: _,
                table,
                col,
                row,
            } => {
                let row_key = resolve_row(row, &slots, params)?;

                // Find matching event.
                let event = find_event(tx_events, tx_index, effect_ordinal, instr_idx)?;
                effect_ordinal += 1;

                let vtype = *type_map.get(&(*table, *col)).ok_or_else(|| {
                    TabulaError::ConsistencyError(format!(
                        "missing schema type for ({:?}, {:?})",
                        table, col
                    ))
                })?;

                let encoded = if event.val_is_null {
                    encode_padded::<W>(&zero_value(vtype), codec)?
                } else {
                    encode_padded::<W>(&event.value, codec)?
                };

                let slot = *dst_val as usize;
                if slot >= MAX_SLOTS {
                    return Err(TabulaError::ConsistencyError(format!(
                        "slot {} >= MAX_SLOTS at instruction {}",
                        slot, instr_idx
                    )));
                }

                // Update slot state.
                slots[slot] = if event.val_is_null {
                    Some(zero_value(vtype))
                } else {
                    Some(event.value)
                };
                slot_fes[slot] = encoded.clone();
                slot_nulls[slot] = event.val_is_null;
                if slot >= max_slot {
                    max_slot = slot + 1;
                }

                let is_empty = empty_columns.contains(&(*table, *col));

                let mut rec = empty_record::<W>(Opcode::Read, tx_index);
                rec.written_slots = vec![slot];
                rec.access_t = Some(table.0);
                rec.access_c = Some(col.0);
                rec.access_r = Some(row_key.0);
                rec.access_val = Some(encoded.clone());
                rec.access_is_null = Some(event.val_is_null);
                rec.dst_val = encoded;
                rec.dst_is_null = event.val_is_null;
                rec.is_empty_col = is_empty;
                records.push(rec);
            }

            Instruction::Write {
                table,
                col,
                row,
                src_val,
                src_is_null: _,
            } => {
                let row_key = resolve_row(row, &slots, params)?;
                let value = resolve_val(src_val, &slots, params)?;
                let value_encoded = encode_padded::<W>(&value, codec)?;

                // Find matching event.
                let event = find_event(tx_events, tx_index, effect_ordinal, instr_idx)?;
                effect_ordinal += 1;

                // Slot linkage: src1 = the value being written.
                let src1_idx = resolve_slot_idx(
                    src_val,
                    &value_encoded,
                    event.val_is_null,
                    &slot_fes,
                    &slot_nulls,
                    max_slot,
                )?;

                let mut rec = empty_record::<W>(Opcode::Write, tx_index);
                rec.src1_val = value_encoded.clone();
                rec.src1_slot_idx = src1_idx;
                rec.access_t = Some(table.0);
                rec.access_c = Some(col.0);
                rec.access_r = Some(row_key.0);
                rec.access_val = Some(value_encoded);
                rec.access_is_null = Some(event.val_is_null);
                records.push(rec);
            }

            Instruction::Arith { dst, op, lhs, rhs } => {
                let lhs_val = resolve_val(lhs, &slots, params)?;
                let rhs_val = resolve_val(rhs, &slots, params)?;
                let result = op.apply(&lhs_val, &rhs_val)?;

                let lhs_enc = encode_padded::<W>(&lhs_val, codec)?;
                let rhs_enc = encode_padded::<W>(&rhs_val, codec)?;
                let dst_enc = encode_padded::<W>(&result, codec)?;

                let opcode = match op {
                    tabula_ir::ArithOp::Add => Opcode::Add,
                    tabula_ir::ArithOp::Sub => Opcode::Sub,
                    tabula_ir::ArithOp::Mul => Opcode::Mul,
                };

                let slot = *dst as usize;
                let src1_idx = resolve_slot_idx(
                    lhs,
                    &lhs_enc,
                    false,
                    &slot_fes,
                    &slot_nulls,
                    max_slot,
                )?;
                let src2_idx = resolve_slot_idx(
                    rhs,
                    &rhs_enc,
                    false,
                    &slot_fes,
                    &slot_nulls,
                    max_slot,
                )?;

                update_slot(
                    &mut slots,
                    &mut slot_fes,
                    &mut slot_nulls,
                    &mut max_slot,
                    slot,
                    result,
                    dst_enc.clone(),
                    false,
                )?;

                let mut rec = empty_record::<W>(opcode, tx_index);
                rec.written_slots = vec![slot];
                rec.src1_val = lhs_enc;
                rec.src2_val = rhs_enc;
                rec.src1_slot_idx = src1_idx;
                rec.src2_slot_idx = src2_idx;
                rec.dst_val = dst_enc;
                rec.dst_is_null = false;
                records.push(rec);
            }

            Instruction::DivMod {
                dst_q,
                dst_r,
                lhs,
                rhs,
            } => {
                let lhs_val = resolve_val(lhs, &slots, params)?;
                let rhs_val = resolve_val(rhs, &slots, params)?;
                let (q, r) = lhs_val.checked_divmod(&rhs_val)?;

                let lhs_enc = encode_padded::<W>(&lhs_val, codec)?;
                let rhs_enc = encode_padded::<W>(&rhs_val, codec)?;
                let q_enc = encode_padded::<W>(&q, codec)?;
                let r_enc = encode_padded::<W>(&r, codec)?;

                let q_slot = *dst_q as usize;
                let r_slot = *dst_r as usize;
                let src1_idx = resolve_slot_idx(
                    lhs,
                    &lhs_enc,
                    false,
                    &slot_fes,
                    &slot_nulls,
                    max_slot,
                )?;
                let src2_idx = resolve_slot_idx(
                    rhs,
                    &rhs_enc,
                    false,
                    &slot_fes,
                    &slot_nulls,
                    max_slot,
                )?;

                update_slot(
                    &mut slots,
                    &mut slot_fes,
                    &mut slot_nulls,
                    &mut max_slot,
                    q_slot,
                    q,
                    q_enc.clone(),
                    false,
                )?;
                update_slot(
                    &mut slots,
                    &mut slot_fes,
                    &mut slot_nulls,
                    &mut max_slot,
                    r_slot,
                    r,
                    r_enc.clone(),
                    false,
                )?;

                let mut rec = empty_record::<W>(Opcode::DivMod, tx_index);
                rec.written_slots = vec![q_slot, r_slot];
                rec.src1_val = lhs_enc;
                rec.src2_val = rhs_enc;
                rec.src1_slot_idx = src1_idx;
                rec.src2_slot_idx = src2_idx;
                rec.dst_val = q_enc;
                rec.dst_is_null = false;
                rec.dst2_val = r_enc;
                rec.dst2_is_null = false;
                records.push(rec);
            }

            Instruction::Cmp { dst, op, lhs, rhs } => {
                let lhs_val = resolve_val(lhs, &slots, params)?;
                let rhs_val = resolve_val(rhs, &slots, params)?;
                let result = op.apply(&lhs_val, &rhs_val)?;

                let lhs_enc = encode_padded::<W>(&lhs_val, codec)?;
                let rhs_enc = encode_padded::<W>(&rhs_val, codec)?;
                let dst_enc = encode_padded::<W>(&result, codec)?;

                let slot = *dst as usize;
                let src1_idx = resolve_slot_idx(
                    lhs,
                    &lhs_enc,
                    false,
                    &slot_fes,
                    &slot_nulls,
                    max_slot,
                )?;
                let src2_idx = resolve_slot_idx(
                    rhs,
                    &rhs_enc,
                    false,
                    &slot_fes,
                    &slot_nulls,
                    max_slot,
                )?;

                update_slot(
                    &mut slots,
                    &mut slot_fes,
                    &mut slot_nulls,
                    &mut max_slot,
                    slot,
                    result,
                    dst_enc.clone(),
                    false,
                )?;

                let mut rec = empty_record::<W>(Opcode::Cmp(map_ir_cmp_op(op)), tx_index);
                rec.written_slots = vec![slot];
                rec.src1_val = lhs_enc;
                rec.src2_val = rhs_enc;
                rec.src1_slot_idx = src1_idx;
                rec.src2_slot_idx = src2_idx;
                rec.dst_val = dst_enc;
                rec.dst_is_null = false;
                records.push(rec);
            }

            Instruction::Not { dst, src } => {
                let src_val = resolve_val(src, &slots, params)?;
                let result = match src_val {
                    Value::Bool(b) => Value::Bool(!b),
                    _ => {
                        return Err(TabulaError::TypeMismatch {
                            expected: "Bool",
                            actual: src_val.type_name(),
                        });
                    }
                };

                let src_enc = encode_padded::<W>(&src_val, codec)?;
                let dst_enc = encode_padded::<W>(&result, codec)?;

                let slot = *dst as usize;
                let src1_idx = resolve_slot_idx(
                    src,
                    &src_enc,
                    false,
                    &slot_fes,
                    &slot_nulls,
                    max_slot,
                )?;

                update_slot(
                    &mut slots,
                    &mut slot_fes,
                    &mut slot_nulls,
                    &mut max_slot,
                    slot,
                    result,
                    dst_enc.clone(),
                    false,
                )?;

                let mut rec = empty_record::<W>(Opcode::Not, tx_index);
                rec.written_slots = vec![slot];
                rec.src1_val = src_enc;
                rec.src1_slot_idx = src1_idx;
                rec.dst_val = dst_enc;
                rec.dst_is_null = false;
                records.push(rec);
            }

            Instruction::And { dst, lhs, rhs } => {
                let lhs_val = resolve_val(lhs, &slots, params)?;
                let rhs_val = resolve_val(rhs, &slots, params)?;
                let result = match (lhs_val, rhs_val) {
                    (Value::Bool(a), Value::Bool(b)) => Value::Bool(a && b),
                    _ => {
                        return Err(TabulaError::TypeMismatch {
                            expected: "Bool",
                            actual: lhs_val.type_name(),
                        });
                    }
                };

                let lhs_enc = encode_padded::<W>(&lhs_val, codec)?;
                let rhs_enc = encode_padded::<W>(&rhs_val, codec)?;
                let dst_enc = encode_padded::<W>(&result, codec)?;

                let slot = *dst as usize;
                let src1_idx = resolve_slot_idx(
                    lhs,
                    &lhs_enc,
                    false,
                    &slot_fes,
                    &slot_nulls,
                    max_slot,
                )?;
                let src2_idx = resolve_slot_idx(
                    rhs,
                    &rhs_enc,
                    false,
                    &slot_fes,
                    &slot_nulls,
                    max_slot,
                )?;

                update_slot(
                    &mut slots,
                    &mut slot_fes,
                    &mut slot_nulls,
                    &mut max_slot,
                    slot,
                    result,
                    dst_enc.clone(),
                    false,
                )?;

                let mut rec = empty_record::<W>(Opcode::And, tx_index);
                rec.written_slots = vec![slot];
                rec.src1_val = lhs_enc;
                rec.src2_val = rhs_enc;
                rec.src1_slot_idx = src1_idx;
                rec.src2_slot_idx = src2_idx;
                rec.dst_val = dst_enc;
                rec.dst_is_null = false;
                records.push(rec);
            }

            Instruction::Or { dst, lhs, rhs } => {
                let lhs_val = resolve_val(lhs, &slots, params)?;
                let rhs_val = resolve_val(rhs, &slots, params)?;
                let result = match (lhs_val, rhs_val) {
                    (Value::Bool(a), Value::Bool(b)) => Value::Bool(a || b),
                    _ => {
                        return Err(TabulaError::TypeMismatch {
                            expected: "Bool",
                            actual: lhs_val.type_name(),
                        });
                    }
                };

                let lhs_enc = encode_padded::<W>(&lhs_val, codec)?;
                let rhs_enc = encode_padded::<W>(&rhs_val, codec)?;
                let dst_enc = encode_padded::<W>(&result, codec)?;

                let slot = *dst as usize;
                let src1_idx = resolve_slot_idx(
                    lhs,
                    &lhs_enc,
                    false,
                    &slot_fes,
                    &slot_nulls,
                    max_slot,
                )?;
                let src2_idx = resolve_slot_idx(
                    rhs,
                    &rhs_enc,
                    false,
                    &slot_fes,
                    &slot_nulls,
                    max_slot,
                )?;

                update_slot(
                    &mut slots,
                    &mut slot_fes,
                    &mut slot_nulls,
                    &mut max_slot,
                    slot,
                    result,
                    dst_enc.clone(),
                    false,
                )?;

                let mut rec = empty_record::<W>(Opcode::Or, tx_index);
                rec.written_slots = vec![slot];
                rec.src1_val = lhs_enc;
                rec.src2_val = rhs_enc;
                rec.src1_slot_idx = src1_idx;
                rec.src2_slot_idx = src2_idx;
                rec.dst_val = dst_enc;
                rec.dst_is_null = false;
                records.push(rec);
            }

            Instruction::Assert { cond } => {
                let cond_val = resolve_val(cond, &slots, params)?;
                let cond_enc = encode_padded::<W>(&cond_val, codec)?;

                // Assert uses src1 for the condition.
                let src1_idx = resolve_slot_idx(
                    cond,
                    &cond_enc,
                    false,
                    &slot_fes,
                    &slot_nulls,
                    max_slot,
                )?;

                let mut rec = empty_record::<W>(Opcode::Assert, tx_index);
                rec.src1_val = cond_enc;
                rec.src1_slot_idx = src1_idx;
                records.push(rec);
            }

            Instruction::Select {
                dst,
                cond,
                if_true,
                if_false,
            } => {
                let cond_val = resolve_val(cond, &slots, params)?;
                let cond_bool = match cond_val {
                    Value::Bool(b) => b,
                    _ => {
                        return Err(TabulaError::TypeMismatch {
                            expected: "Bool",
                            actual: cond_val.type_name(),
                        });
                    }
                };
                let t_val = resolve_val(if_true, &slots, params)?;
                let f_val = resolve_val(if_false, &slots, params)?;
                let result = if cond_bool { t_val } else { f_val };

                let t_enc = encode_padded::<W>(&t_val, codec)?;
                let f_enc = encode_padded::<W>(&f_val, codec)?;
                let dst_enc = encode_padded::<W>(&result, codec)?;

                let slot = *dst as usize;
                let src1_idx = resolve_slot_idx(
                    if_true,
                    &t_enc,
                    false,
                    &slot_fes,
                    &slot_nulls,
                    max_slot,
                )?;
                let src2_idx = resolve_slot_idx(
                    if_false,
                    &f_enc,
                    false,
                    &slot_fes,
                    &slot_nulls,
                    max_slot,
                )?;
                let cond_idx = resolve_slot_idx(
                    cond,
                    &encode_padded::<W>(&cond_val, codec)?,
                    false,
                    &slot_fes,
                    &slot_nulls,
                    max_slot,
                )?;

                update_slot(
                    &mut slots,
                    &mut slot_fes,
                    &mut slot_nulls,
                    &mut max_slot,
                    slot,
                    result,
                    dst_enc.clone(),
                    false,
                )?;

                let mut rec = empty_record::<W>(Opcode::Select, tx_index);
                rec.written_slots = vec![slot];
                rec.src1_val = t_enc;
                rec.src2_val = f_enc;
                rec.cond_val = cond_bool;
                rec.src1_slot_idx = src1_idx;
                rec.src2_slot_idx = src2_idx;
                rec.cond_slot_idx = cond_idx;
                rec.dst_val = dst_enc;
                rec.dst_is_null = false;
                records.push(rec);
            }

            Instruction::Hash { dst, inputs } => {
                if inputs.len() != HASH_INSTRUCTION_INPUT_COUNT as usize {
                    return Err(TabulaError::ConsistencyError(format!(
                        "Hash instruction requires {} inputs, got {}",
                        HASH_INSTRUCTION_INPUT_COUNT,
                        inputs.len()
                    )));
                }

                let v0 = resolve_val(&inputs[0], &slots, params)?;
                let v1 = resolve_val(&inputs[1], &slots, params)?;
                let v0_enc = encode_padded::<W>(&v0, codec)?;
                let v1_enc = encode_padded::<W>(&v1, codec)?;

                // Build Poseidon permutation input.
                let mut perm_input = [BabyBear::ZERO; 16];
                perm_input[0] = BabyBear::new(HASH_INSTRUCTION_DOMAIN_TAG);
                perm_input[1] = BabyBear::new(HASH_INSTRUCTION_INPUT_COUNT);
                for (j, v) in v0_enc.iter().enumerate().take(W) {
                    perm_input[2 + j] = *v;
                }
                for (j, v) in v1_enc.iter().enumerate().take(W) {
                    perm_input[2 + W + j] = *v;
                }

                let (_rounds, perm_output) = poseidon2_permutation(perm_input);
                let digest: [BabyBear; 8] = core::array::from_fn(|i| perm_output[i]);

                // Hash output is 8 FE (Bytes32 width). Pad to W.
                let mut dst_enc = digest.to_vec();
                dst_enc.resize(W, BabyBear::ZERO);

                let slot = *dst as usize;
                let src1_idx = resolve_slot_idx(
                    &inputs[0],
                    &v0_enc,
                    false,
                    &slot_fes,
                    &slot_nulls,
                    max_slot,
                )?;
                let src2_idx = resolve_slot_idx(
                    &inputs[1],
                    &v1_enc,
                    false,
                    &slot_fes,
                    &slot_nulls,
                    max_slot,
                )?;

                // For Hash, the result value is Bytes32 — store the digest as 8 FE in slot.
                let result_fes = digest.to_vec();
                let mut slot_enc = result_fes.clone();
                slot_enc.resize(W, BabyBear::ZERO);

                // Can't produce a proper Value for Bytes32 from FE; store None in Value slot.
                slots[slot] = None;
                slot_fes[slot] = slot_enc;
                slot_nulls[slot] = false;
                if slot >= max_slot {
                    max_slot = slot + 1;
                }

                let mut rec = empty_record::<W>(Opcode::Hash, tx_index);
                rec.written_slots = vec![slot];
                rec.src1_val = v0_enc;
                rec.src2_val = v1_enc;
                rec.src1_slot_idx = src1_idx;
                rec.src2_slot_idx = src2_idx;
                rec.dst_val = dst_enc;
                rec.dst_is_null = false;
                rec.hash_perm_input = Some(perm_input);
                rec.hash_perm_output = Some(digest);
                records.push(rec);
            }

            Instruction::Lookup {
                dst,
                static_table,
                col,
                row,
            } => {
                let row_key = resolve_row(row, &slots, params)?;
                let value = static_tables.lookup(*static_table, row_key, *col)?;
                let dst_enc = encode_padded::<W>(&value, codec)?;

                let slot = *dst as usize;
                update_slot(
                    &mut slots,
                    &mut slot_fes,
                    &mut slot_nulls,
                    &mut max_slot,
                    slot,
                    value,
                    dst_enc.clone(),
                    false,
                )?;

                let mut rec = empty_record::<W>(Opcode::Lookup, tx_index);
                rec.written_slots = vec![slot];
                rec.access_t = Some(static_table.0);
                rec.access_c = Some(col.0);
                rec.access_r = Some(row_key.0);
                rec.access_val = Some(dst_enc.clone());
                rec.access_is_null = Some(false);
                rec.dst_val = dst_enc;
                rec.dst_is_null = false;
                records.push(rec);

                static_rows.push(StaticTableRow {
                    table_id: static_table.0,
                    col_id: col.0,
                    row_key: row_key.0,
                    value: codec.encode(&value)?,
                    lookup_mult: 1,
                });
            }

            Instruction::Emit { .. } => {
                // Out-of-protocol; skip.
            }
        }
    }

    Ok((records, static_rows))
}

/// Find the event matching the given tx_index and effect ordinal.
fn find_event<'a>(
    tx_events: &[&'a ExecutionEvent],
    tx_index: u32,
    effect_ordinal: u32,
    instr_idx: usize,
) -> Result<&'a ExecutionEvent, TabulaError> {
    tx_events
        .iter()
        .find(|e| e.tx_index == tx_index && e.effect_ordinal_in_tx == effect_ordinal)
        .copied()
        .ok_or_else(|| {
            TabulaError::ConsistencyError(format!(
                "no event found for tx={} effect_ordinal={} at instruction {}",
                tx_index, effect_ordinal, instr_idx
            ))
        })
}

/// Update a slot with a new value.
#[allow(clippy::too_many_arguments)]
fn update_slot(
    slots: &mut [Option<Value>],
    slot_fes: &mut [Vec<BabyBear>],
    slot_nulls: &mut [bool],
    max_slot: &mut usize,
    slot: usize,
    value: Value,
    encoded: Vec<BabyBear>,
    is_null: bool,
) -> Result<(), TabulaError> {
    if slot >= MAX_SLOTS {
        return Err(TabulaError::ConsistencyError(format!(
            "slot {} >= MAX_SLOTS ({})",
            slot, MAX_SLOTS
        )));
    }
    slots[slot] = Some(value);
    slot_fes[slot] = encoded;
    slot_nulls[slot] = is_null;
    if slot >= *max_slot {
        *max_slot = slot + 1;
    }
    Ok(())
}
