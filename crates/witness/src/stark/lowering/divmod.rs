use tabula_core::error::TabulaError;
use tabula_ir::ValueExpr;

use tabula_chips::execution::trace::Opcode;

use super::context::LoweringContext;

pub(super) fn lower_divmod<const W: usize>(
    ctx: &mut LoweringContext<'_, W>,
    dst_q: u16,
    dst_r: u16,
    lhs: &ValueExpr,
    rhs: &ValueExpr,
) -> Result<(), TabulaError> {
    let lhs_val = ctx.resolve_val(lhs)?;
    let rhs_val = ctx.resolve_val(rhs)?;
    let (q, r) = lhs_val.checked_divmod(&rhs_val)?;

    let lhs_enc = ctx.encode_padded(&lhs_val)?;
    let rhs_enc = ctx.encode_padded(&rhs_val)?;
    let q_enc = ctx.encode_padded(&q)?;
    let r_enc = ctx.encode_padded(&r)?;

    let q_slot = dst_q as usize;
    let r_slot = dst_r as usize;
    let exclude = [q_slot, r_slot];
    let src1_idx = ctx.resolve_slot_idx(lhs, &lhs_enc, false, &exclude)?;
    let src2_idx = ctx.resolve_slot_idx(rhs, &rhs_enc, false, &exclude)?;

    ctx.update_slot(q_slot, q, q_enc.clone(), false)?;
    ctx.update_slot(r_slot, r, r_enc.clone(), false)?;

    let mut rec = ctx.empty_record(Opcode::DivMod);
    rec.written_slots = vec![q_slot, r_slot];
    rec.src1_val = lhs_enc;
    rec.src2_val = rhs_enc;
    rec.src1_slot_idx = src1_idx;
    rec.src2_slot_idx = src2_idx;
    rec.writes.push((dst_q as usize, q_enc, false));
    rec.writes.push((dst_r as usize, r_enc, false));
    ctx.push_record(rec);

    Ok(())
}
