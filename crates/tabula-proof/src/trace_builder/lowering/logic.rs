use tabula_core::Value;
use tabula_core::error::TabulaError;
use tabula_ir::ValueExpr;

use crate::air::chips::execution::trace::Opcode;

use super::context::LoweringContext;

pub(super) fn lower_not<const W: usize>(
    ctx: &mut LoweringContext<'_, W>,
    dst: u16,
    src: &ValueExpr,
) -> Result<(), TabulaError> {
    let src_val = ctx.resolve_val(src)?;
    let result = match src_val {
        Value::Bool(b) => Value::Bool(!b),
        _ => {
            return Err(TabulaError::TypeMismatch {
                expected: "Bool",
                actual: src_val.type_name(),
            });
        }
    };

    let src_enc = ctx.encode_padded(&src_val)?;
    let dst_enc = ctx.encode_padded(&result)?;

    let slot = dst as usize;
    let src1_idx = ctx.resolve_slot_idx(src, &src_enc, false, &[slot])?;

    ctx.update_slot(slot, result, dst_enc.clone(), false)?;

    let mut rec = ctx.empty_record(Opcode::Not);
    rec.written_slots = vec![slot];
    rec.src1_val = src_enc;
    rec.src1_slot_idx = src1_idx;
    rec.dst_val = dst_enc;
    rec.dst_is_null = false;
    ctx.push_record(rec);

    Ok(())
}

pub(super) fn lower_and<const W: usize>(
    ctx: &mut LoweringContext<'_, W>,
    dst: u16,
    lhs: &ValueExpr,
    rhs: &ValueExpr,
) -> Result<(), TabulaError> {
    lower_bool_binop(ctx, dst, lhs, rhs, |a, b| a && b, Opcode::And)
}

pub(super) fn lower_or<const W: usize>(
    ctx: &mut LoweringContext<'_, W>,
    dst: u16,
    lhs: &ValueExpr,
    rhs: &ValueExpr,
) -> Result<(), TabulaError> {
    lower_bool_binop(ctx, dst, lhs, rhs, |a, b| a || b, Opcode::Or)
}

fn lower_bool_binop<const W: usize>(
    ctx: &mut LoweringContext<'_, W>,
    dst: u16,
    lhs: &ValueExpr,
    rhs: &ValueExpr,
    op: fn(bool, bool) -> bool,
    opcode: Opcode,
) -> Result<(), TabulaError> {
    let lhs_val = ctx.resolve_val(lhs)?;
    let rhs_val = ctx.resolve_val(rhs)?;
    let result = match (lhs_val, rhs_val) {
        (Value::Bool(a), Value::Bool(b)) => Value::Bool(op(a, b)),
        _ => {
            return Err(TabulaError::TypeMismatch {
                expected: "Bool",
                actual: lhs_val.type_name(),
            });
        }
    };

    let lhs_enc = ctx.encode_padded(&lhs_val)?;
    let rhs_enc = ctx.encode_padded(&rhs_val)?;
    let dst_enc = ctx.encode_padded(&result)?;

    let slot = dst as usize;
    let exclude = [slot];
    let src1_idx = ctx.resolve_slot_idx(lhs, &lhs_enc, false, &exclude)?;
    let src2_idx = ctx.resolve_slot_idx(rhs, &rhs_enc, false, &exclude)?;

    ctx.update_slot(slot, result, dst_enc.clone(), false)?;

    let mut rec = ctx.empty_record(opcode);
    rec.written_slots = vec![slot];
    rec.src1_val = lhs_enc;
    rec.src2_val = rhs_enc;
    rec.src1_slot_idx = src1_idx;
    rec.src2_slot_idx = src2_idx;
    rec.dst_val = dst_enc;
    rec.dst_is_null = false;
    ctx.push_record(rec);

    Ok(())
}
