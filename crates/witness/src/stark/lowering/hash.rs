use p3_koala_bear::KoalaBear;

use tabula_commitment::NativeDigest;
use tabula_core::error::TabulaError;
use tabula_ir::ValueExpr;
use tabula_types::bytes32_typed;

use tabula_chips::execution::trace::Opcode;
use tabula_chips::ir_hash::IrHashCall;

use super::context::LoweringContext;

pub(super) fn lower_hash<const W: usize>(
    ctx: &mut LoweringContext<'_, W>,
    dst: u16,
    inputs: &[ValueExpr],
) -> Result<(), TabulaError> {
    let typed_inputs = inputs
        .iter()
        .map(|expr| ctx.resolve_val(expr))
        .collect::<Result<Vec<_>, _>>()?;
    let portable_inputs = typed_inputs
        .iter()
        .map(|value| ctx.type_runtimes.encode_typed(value))
        .collect::<Result<Vec<_>, _>>()?;

    let instruction_index = ctx.records.len() as u32;
    let call = IrHashCall::from_inputs(ctx.tx_index, instruction_index, &portable_inputs)?;
    let digest_bytes =
        NativeDigest(core::array::from_fn(|idx| KoalaBear::new(call.digest[idx]))).to_bytes();
    let digest_typed = bytes32_typed(digest_bytes);

    let slot = dst as usize;
    let digest_prefix: Vec<KoalaBear> = call
        .digest
        .iter()
        .take(W)
        .map(|value| KoalaBear::new(*value))
        .collect();

    ctx.slots[slot] = Some(digest_typed.clone());
    ctx.slot_fes[slot] = digest_prefix.clone();
    ctx.slot_nulls[slot] = false;
    ctx.slot_initialized[slot] = true;
    if slot >= ctx.max_slot {
        ctx.max_slot = slot + 1;
    }

    let mut rec = ctx.empty_record(Opcode::Hash);
    rec.written_slots = vec![slot];
    rec.writes.push((slot, digest_prefix, false));
    rec.instruction_index = Some(instruction_index);
    rec.hash_digest = Some(core::array::from_fn(|idx| KoalaBear::new(call.digest[idx])));
    ctx.push_record(rec);
    ctx.push_ir_hash_call(call);

    Ok(())
}
