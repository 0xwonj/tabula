use tabula_core::error::TabulaError;
use tabula_ir::ValueExpr;

use tabula_chips::execution::trace::{CmpOp, Opcode};

use super::context::LoweringContext;

pub(super) fn lower_cmp<const W: usize>(
    ctx: &mut LoweringContext<'_, W>,
    dst: u16,
    op: tabula_ir::CmpOp,
    lhs: &ValueExpr,
    rhs: &ValueExpr,
) -> Result<(), TabulaError> {
    let lhs_val = ctx.resolve_val(lhs)?;
    let rhs_val = ctx.resolve_val(rhs)?;
    let result = op.apply(&lhs_val, &rhs_val)?;

    let lhs_enc = ctx.encode_padded(&lhs_val)?;
    let rhs_enc = ctx.encode_padded(&rhs_val)?;
    let dst_enc = ctx.encode_padded(&result)?;

    let slot = dst as usize;
    let exclude = [slot];
    let src1_idx = ctx.resolve_slot_idx(lhs, &lhs_enc, false, &exclude)?;
    let src2_idx = ctx.resolve_slot_idx(rhs, &rhs_enc, false, &exclude)?;

    ctx.update_slot(slot, result, dst_enc.clone(), false)?;

    let mut rec = ctx.empty_record(Opcode::Cmp(map_ir_cmp_op(op)));
    rec.written_slots = vec![slot];
    rec.src1_val = lhs_enc;
    rec.src2_val = rhs_enc;
    rec.src1_slot_idx = src1_idx;
    rec.src2_slot_idx = src2_idx;
    rec.writes.push((dst as usize, dst_enc, false));
    ctx.push_record(rec);

    Ok(())
}

fn map_ir_cmp_op(op: tabula_ir::CmpOp) -> CmpOp {
    match op {
        tabula_ir::CmpOp::Eq => CmpOp::Eq,
        tabula_ir::CmpOp::Ne => CmpOp::Ne,
        tabula_ir::CmpOp::Lt => CmpOp::Lt,
        tabula_ir::CmpOp::Lte => CmpOp::Lte,
        tabula_ir::CmpOp::Gt => CmpOp::Gt,
        tabula_ir::CmpOp::Gte => CmpOp::Gte,
    }
}
