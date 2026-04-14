//! Event emission lowering helpers.

use tabula_chips::execution::trace::Opcode;
use tabula_core::error::TabulaError;
use tabula_ir as ir;

use super::super::context::LoweringCx;

impl<'a, const W: usize> LoweringCx<'a, W> {
    pub(crate) fn lower_emit_event(
        &mut self,
        guard: Option<ir::GuardRef>,
        op_index: usize,
        event: ir::EventId,
        args: &ir::ValueTupleRef,
    ) -> Result<(), TabulaError> {
        if !self.guard_active(guard)? {
            let _ = self.event_effects_by_op.remove(&op_index);
            return Ok(());
        }
        let effect =
            self.event_effects_by_op
                .remove(&op_index)
                .ok_or_else(|| TabulaError::ProofError {
                    phase: "next_trace_lowering",
                    detail: format!(
                        "missing event effect for tx={} op {}",
                        self.tx_index, op_index
                    ),
                })?;
        if effect.event != event {
            return Err(TabulaError::ProofError {
                phase: "next_trace_lowering",
                detail: format!(
                    "event op {} in tx={} emitted event {} but journal recorded {}",
                    op_index, self.tx_index, event.0, effect.event.0
                ),
            });
        }
        let expected_args = self.eval_tuple(args)?;
        if effect.args != expected_args {
            return Err(TabulaError::ProofError {
                phase: "next_trace_lowering",
                detail: format!(
                    "event op {} in tx={} arguments do not match the execution journal",
                    op_index, self.tx_index
                ),
            });
        }

        let Some(base_item_index) = self.event_item_bases_by_op.get(&op_index).copied() else {
            return Err(TabulaError::ProofError {
                phase: "next_trace_lowering",
                detail: format!(
                    "event op {} in tx={} is missing its canonical event-transcript item index",
                    op_index, self.tx_index
                ),
            });
        };

        let mut header = self.empty_record(Opcode::EmitEventHeader);
        header.instruction_index = Some(op_index as u32);
        header.proof_meta0 = Some(base_item_index);
        header.proof_meta1 = Some(effect.effect_ordinal_in_entry);
        header.proof_meta2 = Some(effect.event.0);
        header.proof_meta3 = Some(effect.args.len() as u32);
        self.records.push(header);

        for (arg_index, value) in effect.args.iter().enumerate() {
            let encoded = self.encode_padded(value)?;
            let mut arg = self.empty_record(Opcode::EmitEventArg);
            arg.instruction_index = Some(op_index as u32);
            arg.proof_meta0 = Some(base_item_index + 1 + arg_index as u32);
            arg.proof_meta1 = Some(effect.effect_ordinal_in_entry);
            arg.proof_meta2 = Some(arg_index as u32);
            arg.proof_meta3 = Some(value.type_id().0);
            arg.src1_val = encoded;
            self.records.push(arg);
        }
        Ok(())
    }
}
