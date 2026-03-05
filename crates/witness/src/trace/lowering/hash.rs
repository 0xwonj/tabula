use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;

use tabula_core::error::TabulaError;
use tabula_ir::ValueExpr;

use tabula_chips::execution::trace::Opcode;
use tabula_chips::execution::{HASH_INSTRUCTION_DOMAIN_TAG, HASH_INSTRUCTION_INPUT_COUNT};
use tabula_chips::poseidon::constants::poseidon2_permutation;

use super::context::LoweringContext;

pub(super) fn lower_hash<const W: usize>(
    ctx: &mut LoweringContext<'_, W>,
    dst: u16,
    inputs: &[ValueExpr],
) -> Result<(), TabulaError> {
    if inputs.len() != HASH_INSTRUCTION_INPUT_COUNT as usize {
        return Err(TabulaError::ProofError {
            phase: "trace_lowering",
            detail: format!(
                "Hash instruction requires {} inputs, got {}",
                HASH_INSTRUCTION_INPUT_COUNT,
                inputs.len()
            ),
        });
    }

    let v0 = ctx.resolve_val(&inputs[0])?;
    let v1 = ctx.resolve_val(&inputs[1])?;
    let v0_enc = ctx.encode_padded(&v0)?;
    let v1_enc = ctx.encode_padded(&v1)?;

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

    let slot = dst as usize;
    let exclude = [slot];
    let src1_idx = ctx.resolve_slot_idx(&inputs[0], &v0_enc, false, &exclude)?;
    let src2_idx = ctx.resolve_slot_idx(&inputs[1], &v1_enc, false, &exclude)?;

    // For Hash, the result value is Bytes32 — store the digest as 8 FE in slot.
    let result_fes = digest.to_vec();
    let mut slot_enc = result_fes;
    slot_enc.resize(W, BabyBear::ZERO);

    // Can't produce a proper Value for Bytes32 from FE; store None in Value slot.
    ctx.slots[slot] = None;
    ctx.slot_fes[slot] = slot_enc;
    ctx.slot_nulls[slot] = false;
    ctx.slot_initialized[slot] = true;
    if slot >= ctx.max_slot {
        ctx.max_slot = slot + 1;
    }

    let mut rec = ctx.empty_record(Opcode::Hash);
    rec.written_slots = vec![slot];
    rec.src1_val = v0_enc;
    rec.src2_val = v1_enc;
    rec.src1_slot_idx = src1_idx;
    rec.src2_slot_idx = src2_idx;
    rec.dst_val = dst_enc;
    rec.dst_is_null = false;
    rec.hash_perm_input = Some(perm_input);
    rec.hash_perm_output = Some(digest);
    ctx.push_record(rec);

    Ok(())
}
