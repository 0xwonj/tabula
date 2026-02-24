use tabula_core::Value;
use tabula_core::error::TabulaError;
use tabula_ir::ValueExpr;

use crate::air::chips::execution::trace::Opcode;

use super::context::LoweringContext;

pub(super) fn lower_assert<const W: usize>(
    ctx: &mut LoweringContext<'_, W>,
    cond: &ValueExpr,
) -> Result<(), TabulaError> {
    let cond_val = ctx.resolve_val(cond)?;
    let cond_enc = ctx.encode_padded(&cond_val)?;

    let src1_idx = ctx.resolve_slot_idx(
        cond,
        &cond_enc,
        false,
        &[], // Assert has no written slot
    )?;

    let mut rec = ctx.empty_record(Opcode::Assert);
    rec.src1_val = cond_enc;
    rec.src1_slot_idx = src1_idx;
    ctx.push_record(rec);

    Ok(())
}

pub(super) fn lower_select<const W: usize>(
    ctx: &mut LoweringContext<'_, W>,
    dst: u16,
    cond: &ValueExpr,
    if_true: &ValueExpr,
    if_false: &ValueExpr,
) -> Result<(), TabulaError> {
    let cond_val = ctx.resolve_val(cond)?;
    let cond_bool = match cond_val {
        Value::Bool(b) => b,
        _ => {
            return Err(TabulaError::TypeMismatch {
                expected: "Bool",
                actual: cond_val.type_name(),
            });
        }
    };
    let t_val = ctx.resolve_val(if_true)?;
    let f_val = ctx.resolve_val(if_false)?;
    let result = if cond_bool { t_val } else { f_val };

    let t_enc = ctx.encode_padded(&t_val)?;
    let f_enc = ctx.encode_padded(&f_val)?;
    let dst_enc = ctx.encode_padded(&result)?;

    let slot = dst as usize;
    let exclude = [slot];
    let src1_idx = ctx.resolve_slot_idx(if_true, &t_enc, false, &exclude)?;
    let src2_idx = ctx.resolve_slot_idx(if_false, &f_enc, false, &exclude)?;
    let cond_idx = ctx.resolve_slot_idx(cond, &ctx.encode_padded(&cond_val)?, false, &exclude)?;

    ctx.update_slot(slot, result, dst_enc.clone(), false)?;

    let mut rec = ctx.empty_record(Opcode::Select);
    rec.written_slots = vec![slot];
    rec.src1_val = t_enc;
    rec.src2_val = f_enc;
    rec.cond_val = cond_bool;
    rec.src1_slot_idx = src1_idx;
    rec.src2_slot_idx = src2_idx;
    rec.cond_slot_idx = cond_idx;
    rec.dst_val = dst_enc;
    rec.dst_is_null = false;
    ctx.push_record(rec);

    Ok(())
}
