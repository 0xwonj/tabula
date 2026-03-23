//! Witness lowering for the Precompile opcode.
//!
//! Binds each IR call site to a unique typed precompile call, proves actual
//! program-visible outputs in the execution lane, and relays the canonical
//! transcript digest through the execution row.

use tabula_chips::execution::trace::Opcode;
use tabula_chips::precompile_transcript::compute_precompile_call_header;
use tabula_core::PrecompileEvent;
use tabula_core::error::TabulaError;
use tabula_ir::{PrecompileId, ValueExpr};

use super::context::LoweringContext;

pub(super) fn lower_precompile<const W: usize>(
    ctx: &mut LoweringContext<'_, W>,
    instr_idx: usize,
    id: PrecompileId,
    dst_slots: &[u16],
    inputs: &[ValueExpr],
) -> Result<(), TabulaError> {
    let call = ctx.precompile_event(instr_idx)?;
    if call.precompile_id != id {
        return Err(TabulaError::ProofError {
            phase: "trace_lowering",
            detail: format!(
                "precompile event id 0x{:04x} does not match instruction id 0x{:04x} at tx={} instruction {}",
                call.precompile_id.0, id.0, ctx.tx_index, instr_idx,
            ),
        });
    }
    let signature = ctx.precompile_signature(id)?;

    let input_vals = inputs
        .iter()
        .map(|expr| ctx.resolve_val(expr))
        .collect::<Result<Vec<_>, _>>()?;
    if input_vals != call.inputs {
        return Err(TabulaError::ProofError {
            phase: "trace_lowering",
            detail: format!(
                "precompile inputs for tx={} instruction {} do not match stored event",
                ctx.tx_index, instr_idx,
            ),
        });
    }
    if call.outputs.len() != dst_slots.len() {
        return Err(TabulaError::ProofError {
            phase: "trace_lowering",
            detail: format!(
                "precompile event for tx={} instruction {} reports {} outputs but IR declares {} dst_slots",
                ctx.tx_index,
                instr_idx,
                call.outputs.len(),
                dst_slots.len(),
            ),
        });
    }

    let portable_event = PrecompileEvent {
        tx_index: ctx.tx_index as usize,
        instruction_index: call.instruction_index,
        precompile_id: call.precompile_id.0,
        inputs: call
            .inputs
            .iter()
            .map(|value| ctx.type_runtimes.encode_typed(value))
            .collect::<Result<Vec<_>, _>>()?,
        outputs: call
            .outputs
            .iter()
            .map(|value| ctx.type_runtimes.encode_typed(value))
            .collect::<Result<Vec<_>, _>>()?,
    };
    let header = compute_precompile_call_header(
        &portable_event,
        id.0,
        signature,
        ctx.type_runtimes,
        ctx.encoding_runtimes,
    )?;

    let exclude: Vec<usize> = dst_slots.iter().map(|slot| *slot as usize).collect();
    let src1_idx = if !inputs.is_empty() {
        let enc = ctx.encode_padded(&input_vals[0])?;
        ctx.resolve_slot_idx(&inputs[0], &enc, false, &exclude)?
    } else {
        None
    };
    let src2_idx = if inputs.len() >= 2 {
        let enc = ctx.encode_padded(&input_vals[1])?;
        ctx.resolve_slot_idx(&inputs[1], &enc, false, &exclude)?
    } else {
        None
    };

    let mut writes = Vec::with_capacity(dst_slots.len());
    let mut written_slots = Vec::with_capacity(dst_slots.len());
    for (dst_slot, output) in dst_slots.iter().zip(&call.outputs) {
        let slot_index = *dst_slot as usize;
        let encoded = ctx.encode_padded(output)?;
        ctx.update_slot(slot_index, output.clone(), encoded.clone(), false)?;
        writes.push((slot_index, encoded, false));
        written_slots.push(slot_index);
    }

    let mut rec = ctx.empty_record(Opcode::Precompile);
    rec.written_slots = written_slots;
    rec.writes = writes;
    rec.precompile_id = Some(id.0);
    rec.instruction_index = Some(instr_idx as u32);
    rec.precompile_input_count = Some(input_vals.len() as u32);
    rec.precompile_output_count = Some(call.outputs.len() as u32);
    rec.precompile_event_digest = Some(core::array::from_fn(|idx| {
        p3_koala_bear::KoalaBear::new(header.event_digest[idx])
    }));
    rec.src1_slot_idx = src1_idx;
    rec.src2_slot_idx = src2_idx;
    if !input_vals.is_empty() {
        rec.src1_val = ctx.encode_padded(&input_vals[0])?;
    }
    if input_vals.len() >= 2 {
        rec.src2_val = ctx.encode_padded(&input_vals[1])?;
    }

    ctx.push_record(rec);
    Ok(())
}
