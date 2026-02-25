use tabula_core::error::TabulaError;
use tabula_ir::ValueExpr;

use crate::chips::execution::trace::Opcode;

use super::context::LoweringContext;

pub(super) fn lower_arith<const W: usize>(
    ctx: &mut LoweringContext<'_, W>,
    dst: u16,
    op: &tabula_ir::ArithOp,
    lhs: &ValueExpr,
    rhs: &ValueExpr,
) -> Result<(), TabulaError> {
    let lhs_val = ctx.resolve_val(lhs)?;
    let rhs_val = ctx.resolve_val(rhs)?;
    let result = op.apply(&lhs_val, &rhs_val)?;

    let lhs_enc = ctx.encode_padded(&lhs_val)?;
    let rhs_enc = ctx.encode_padded(&rhs_val)?;
    let dst_enc = ctx.encode_padded(&result)?;

    let opcode = match op {
        tabula_ir::ArithOp::Add => Opcode::Add,
        tabula_ir::ArithOp::Sub => Opcode::Sub,
        tabula_ir::ArithOp::Mul => Opcode::Mul,
    };

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
