//! Witness lowering for the Precompile opcode.
//!
//! Reads stored precompile I/O from execution results, then constructs a
//! Poseidon I/O commitment and writes the digest into the destination slot.

use p3_field::PrimeCharacteristicRing;
use p3_koala_bear::KoalaBear;

use tabula_core::error::TabulaError;
use tabula_ir::{PrecompileId, ValueExpr};

use tabula_chips::execution::PRECOMPILE_DOMAIN_TAG;
use tabula_chips::execution::trace::Opcode;
use tabula_chips::poseidon::constants::poseidon2_permutation;

use super::context::LoweringContext;

pub(super) fn lower_precompile<const W: usize>(
    ctx: &mut LoweringContext<'_, W>,
    id: PrecompileId,
    dst_slots: &[u16],
    inputs: &[ValueExpr],
) -> Result<(), TabulaError> {
    // 1. Read stored I/O from execution results.
    let io = ctx
        .precompile_ios
        .get(ctx.precompile_idx)
        .ok_or_else(|| TabulaError::ProofError {
            phase: "trace_lowering",
            detail: format!(
                "precompile instruction encountered but no stored I/O at index {}",
                ctx.precompile_idx
            ),
        })?;
    ctx.precompile_idx += 1;

    // Resolve input values and encode to field elements.
    let mut input_vals = Vec::with_capacity(inputs.len());
    let mut input_fes = Vec::new();
    for inp in inputs {
        let val = ctx.resolve_val(inp)?;
        let enc = ctx.encode_padded(&val)?;
        input_fes.extend_from_slice(&enc);
        input_vals.push(val);
    }

    // Use stored output values instead of re-executing.
    let output_vals = &io.outputs;
    let mut output_fes = Vec::new();
    for val in output_vals {
        let enc = ctx.encode_padded(val)?;
        output_fes.extend_from_slice(&enc);
    }

    // 3. Build Poseidon permutation input.
    //    Layout: [DOMAIN_TAG, precompile_id, n_inputs, input_fes..., output_fes..., 0-padding]
    //    Total: 16 field elements (Poseidon state width).
    let mut perm_input = [KoalaBear::ZERO; 16];
    perm_input[0] = KoalaBear::new(PRECOMPILE_DOMAIN_TAG);
    perm_input[1] = KoalaBear::new(id.0 as u32);
    perm_input[2] = KoalaBear::new(inputs.len() as u32);

    let mut offset = 3;
    for fe in &input_fes {
        if offset < 16 {
            perm_input[offset] = *fe;
            offset += 1;
        }
    }
    for fe in &output_fes {
        if offset < 16 {
            perm_input[offset] = *fe;
            offset += 1;
        }
    }
    // Remaining positions stay zero (padding).

    // 4. Compute Poseidon permutation.
    let (_rounds, perm_output) = poseidon2_permutation(perm_input);
    let digest: [KoalaBear; 8] = core::array::from_fn(|i| perm_output[i]);

    // 5. The first dst_slot gets hash_perm_output[0..W] (the I/O commitment).
    //    The AIR constrains slot_written_count = 1 for Precompile, so only
    //    the first dst_slot is written at the proof level.
    let first_slot = *dst_slots.first().ok_or_else(|| TabulaError::ProofError {
        phase: "trace_lowering",
        detail: "Precompile instruction has no dst_slots".into(),
    })? as usize;

    let mut dst_enc = digest[..W].to_vec();
    dst_enc.resize(W, KoalaBear::ZERO);

    // Resolve src operand slots for linkage (inputs may reference slots).
    let exclude = [first_slot];
    let src1_idx = if !inputs.is_empty() {
        let v0_enc = ctx.encode_padded(&input_vals[0])?;
        ctx.resolve_slot_idx(&inputs[0], &v0_enc, false, &exclude)?
    } else {
        None
    };
    let src2_idx = if inputs.len() >= 2 {
        let v1_enc = ctx.encode_padded(&input_vals[1])?;
        ctx.resolve_slot_idx(&inputs[1], &v1_enc, false, &exclude)?
    } else {
        None
    };

    // The slot value at the proof level is the I/O commitment digest.
    // (The executor stores actual values; the proof stores the commitment.)
    ctx.slots[first_slot] = None;
    ctx.slot_fes[first_slot] = dst_enc.clone();
    ctx.slot_nulls[first_slot] = false;
    ctx.slot_initialized[first_slot] = true;
    if first_slot >= ctx.max_slot {
        ctx.max_slot = first_slot + 1;
    }

    // 6. Build instruction record.
    let mut rec = ctx.empty_record(Opcode::Precompile);
    rec.written_slots = vec![first_slot];
    rec.precompile_id = Some(id.0);
    rec.hash_perm_input = Some(perm_input);
    rec.hash_perm_output = Some(digest);
    rec.writes.push((first_slot, dst_enc, false));

    // Populate operand values for linkage constraints.
    if !input_vals.is_empty() {
        rec.src1_val = ctx.encode_padded(&input_vals[0])?;
    }
    if input_vals.len() >= 2 {
        rec.src2_val = ctx.encode_padded(&input_vals[1])?;
    }
    rec.src1_slot_idx = src1_idx;
    rec.src2_slot_idx = src2_idx;

    ctx.push_record(rec);

    Ok(())
}
