use std::collections::{BTreeMap, BTreeSet};

use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;

use tabula_core::error::TabulaError;
use tabula_core::{ColId, TableId, TableSchema, Value};
use tabula_ir::{Instruction, ValueExpr};

use tabula_chips::execution::MAX_SLOTS;
use tabula_chips::execution::trace::Opcode;

mod access;
mod arith;
mod cmp;
mod context;
mod control;
mod divmod;
mod hash;
mod logic;
mod lookup;
pub mod orchestration;
mod precompile;
mod property_read;

use context::LoweringContext;

// ── Re-exports ──────────────────────────────────────────────────────────────

pub use context::{PrecompileExecuteFn, PropertyReadFn};
pub use orchestration::{LoweringOutput, lower_execution_records, lower_program_batch};

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
        Instruction::Precompile { dst_slots, .. } => dst_slots.iter().map(|s| *s as usize).max(),
        Instruction::PropertyRead {
            dst_val,
            dst_key,
            dst_is_null,
            ..
        } => Some(
            (*dst_val as usize)
                .max(*dst_key as usize)
                .max(*dst_is_null as usize),
        ),
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
            Instruction::Precompile { inputs, .. } => {
                for inp in inputs {
                    add(inp);
                }
            }
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
        .map_or(0, |m| m + 1);
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
            return Err(TabulaError::ProofError {
                phase: "trace_lowering",
                detail: format!(
                    "cannot pre-materialize param: slot {slot} >= MAX_SLOTS ({MAX_SLOTS})"
                ),
            });
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
                arith::lower_arith(ctx, *dst, *op, lhs, rhs)?;
            }

            Instruction::DivMod {
                dst_q,
                dst_r,
                lhs,
                rhs,
            } => divmod::lower_divmod(ctx, *dst_q, *dst_r, lhs, rhs)?,

            Instruction::Cmp { dst, op, lhs, rhs } => {
                cmp::lower_cmp(ctx, *dst, *op, lhs, rhs)?;
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

            Instruction::Precompile {
                id,
                dst_slots,
                inputs,
            } => precompile::lower_precompile(ctx, *id, dst_slots, inputs)?,

            Instruction::PropertyRead {
                dst_val,
                dst_key,
                dst_is_null,
                table,
                col,
                query,
            } => property_read::lower_property_read(
                ctx,
                *dst_val,
                *dst_key,
                *dst_is_null,
                *table,
                *col,
                query,
            )?,
        }
    }

    Ok(())
}
