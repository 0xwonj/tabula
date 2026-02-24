use tabula_core::error::TabulaError;
use tabula_core::{ColId, TableId, zero_value};
use tabula_ir::{RowExpr, ValueExpr};

use crate::air::chips::execution::MAX_SLOTS;
use crate::air::chips::execution::trace::Opcode;

use super::context::LoweringContext;

pub(super) fn lower_read<const W: usize>(
    ctx: &mut LoweringContext<'_, W>,
    dst_val: u16,
    table: TableId,
    col: ColId,
    row: &RowExpr,
    instr_idx: usize,
) -> Result<(), TabulaError> {
    let row_key = ctx.resolve_row(row)?;

    let event = ctx.find_event(instr_idx)?;
    ctx.effect_ordinal += 1;

    let vtype = *ctx
        .type_map
        .get(&(table, col))
        .ok_or_else(|| TabulaError::ProofError {
            phase: "trace_lowering",
            detail: format!("missing schema type for ({:?}, {:?})", table, col),
        })?;

    let encoded = if event.val_is_null {
        ctx.encode_padded(&zero_value(vtype))?
    } else {
        ctx.encode_padded(&event.value)?
    };

    let slot = dst_val as usize;
    if slot >= MAX_SLOTS {
        return Err(TabulaError::ProofError {
            phase: "trace_lowering",
            detail: format!("slot {} >= MAX_SLOTS at instruction {}", slot, instr_idx),
        });
    }

    // Update slot state.
    let slot_value = if event.val_is_null {
        zero_value(vtype)
    } else {
        event.value
    };
    ctx.slots[slot] = Some(slot_value);
    ctx.slot_fes[slot] = encoded.clone();
    ctx.slot_nulls[slot] = event.val_is_null;
    ctx.slot_initialized[slot] = true;
    if slot >= ctx.max_slot {
        ctx.max_slot = slot + 1;
    }

    let is_empty = ctx.empty_columns.contains(&(table, col));

    let mut rec = ctx.empty_record(Opcode::Read);
    rec.written_slots = vec![slot];
    rec.access_t = Some(table.0);
    rec.access_c = Some(col.0);
    rec.access_r = Some(row_key.0);
    rec.access_val = Some(encoded.clone());
    rec.access_is_null = Some(event.val_is_null);
    rec.dst_val = encoded;
    rec.dst_is_null = event.val_is_null;
    rec.is_empty_col = is_empty;
    ctx.push_record(rec);

    Ok(())
}

pub(super) fn lower_write<const W: usize>(
    ctx: &mut LoweringContext<'_, W>,
    table: TableId,
    col: ColId,
    row: &RowExpr,
    src_val: &ValueExpr,
    instr_idx: usize,
) -> Result<(), TabulaError> {
    let row_key = ctx.resolve_row(row)?;
    let value = ctx.resolve_val(src_val)?;
    let value_encoded = ctx.encode_padded(&value)?;

    let event = ctx.find_event(instr_idx)?;
    ctx.effect_ordinal += 1;

    let src1_idx = ctx.resolve_slot_idx(
        src_val,
        &value_encoded,
        event.val_is_null,
        &[], // Write has no written slot
    )?;

    let mut rec = ctx.empty_record(Opcode::Write);
    rec.src1_val = value_encoded.clone();
    rec.src1_slot_idx = src1_idx;
    rec.access_t = Some(table.0);
    rec.access_c = Some(col.0);
    rec.access_r = Some(row_key.0);
    rec.access_val = Some(value_encoded);
    rec.access_is_null = Some(event.val_is_null);
    ctx.push_record(rec);

    Ok(())
}
