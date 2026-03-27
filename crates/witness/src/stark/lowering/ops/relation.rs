//! Relation assertion/evaluation lowering helpers.

use p3_koala_bear::KoalaBear;

use tabula_chips::execution::trace::Opcode;
use tabula_chips::relation_transcript::RelationTranscriptCall;
use tabula_contract::format::typed_tuple::TypedTupleRole;
use tabula_core::error::TabulaError;
use tabula_executor as exec;
use tabula_ir as ir;

use super::super::context::LoweringCx;
use crate::relation_proof::relation_claim_from_effect;

impl<'a, const W: usize> LoweringCx<'a, W> {
    pub(crate) fn lower_assert_relation(
        &mut self,
        guard: Option<ir::GuardRef>,
        op_index: usize,
        relation: ir::RelationId,
        args: &ir::ValueTupleRef,
    ) -> Result<(), TabulaError> {
        if !self.guard_active(guard)? {
            self.expect_no_relation_effect(op_index)?;
            return Ok(());
        }

        let effect = self.take_relation_effect(op_index, exec::RelationEffectKind::Assert)?;
        let expected_inputs = self.eval_tuple(args)?;
        if effect.relation != relation {
            return Err(TabulaError::ProofError {
                phase: "next_trace_lowering",
                detail: format!(
                    "assert relation op {} in tx={} referenced relation {} but journal recorded {}",
                    op_index, self.tx_index, relation.0, effect.relation.0
                ),
            });
        }
        if effect.inputs != expected_inputs {
            return Err(TabulaError::ProofError {
                phase: "next_trace_lowering",
                detail: format!(
                    "assert relation op {} in tx={} inputs do not match the execution journal",
                    op_index, self.tx_index
                ),
            });
        }
        if !effect.outputs.is_empty() {
            return Err(TabulaError::ProofError {
                phase: "next_trace_lowering",
                detail: format!(
                    "assert relation op {} in tx={} unexpectedly produced outputs",
                    op_index, self.tx_index
                ),
            });
        }

        let input_call = RelationTranscriptCall::from_typed_values(
            self.tx_index,
            effect.effect_ordinal_in_entry,
            op_index as u32,
            TypedTupleRole::RelationInput,
            &effect.inputs,
            self.tuple_encoding_defaults,
            self.encoding_runtimes,
        )?;
        let output_call = RelationTranscriptCall::from_typed_values(
            self.tx_index,
            effect.effect_ordinal_in_entry,
            op_index as u32,
            TypedTupleRole::RelationOutput,
            &[],
            self.tuple_encoding_defaults,
            self.encoding_runtimes,
        )?;

        let mut rec = self.empty_record(Opcode::RelationProof);
        rec.effect_ordinal_in_tx = effect.effect_ordinal_in_entry;
        rec.relation_is_eval = false;
        rec.relation_id = Some(relation.0);
        rec.instruction_index = Some(op_index as u32);
        rec.relation_input_digest = Some(core::array::from_fn(|idx| {
            KoalaBear::new(input_call.digest[idx])
        }));
        rec.relation_output_digest = Some(core::array::from_fn(|idx| {
            KoalaBear::new(output_call.digest[idx])
        }));

        for (index, (value_ref, value)) in args.0.iter().zip(effect.inputs.iter()).enumerate() {
            let slot = self.resolve_operand_slot(value_ref, value, false, &[])?;
            rec.relation_input_used[index] = true;
            rec.relation_input_type_ids[index] = value.type_id().0;
            rec.relation_input_vals[index] = input_call.tuple_values[index];
            rec.relation_input_sel[index][slot] = true;
        }

        let input_digest = input_call.digest;
        let output_digest = output_call.digest;
        self.records.push(rec);
        self.relation_transcript_calls.push(input_call);
        self.relation_transcript_calls.push(output_call);
        self.relation_claims.push(relation_claim_from_effect(
            self.tx_index,
            effect,
            input_digest,
            output_digest,
        ));
        self.current_effect_ordinal = effect.effect_ordinal_in_entry.saturating_add(1);
        Ok(())
    }

    pub(crate) fn lower_eval_relation(
        &mut self,
        guard: Option<ir::GuardRef>,
        op_index: usize,
        relation: ir::RelationId,
        inputs: &ir::ValueTupleRef,
        dsts: &[ir::LocalId],
    ) -> Result<(), TabulaError> {
        if !self.guard_active(guard)? {
            self.expect_no_relation_effect(op_index)?;
            for dst in dsts {
                let ty = self.local_type(*dst)?;
                let zero_slot = self.ensure_typed_zero_slot(ty)?;
                let zero_value = self.slots[zero_slot]
                    .as_ref()
                    .cloned()
                    .ok_or_else(|| TabulaError::InvalidIr("missing typed zero slot".into()))?;
                self.copy_into_local(*dst, zero_slot, zero_value)?;
            }
            return Ok(());
        }

        let effect = self.take_relation_effect(op_index, exec::RelationEffectKind::Eval)?;
        let expected_inputs = self.eval_tuple(inputs)?;
        if effect.relation != relation {
            return Err(TabulaError::ProofError {
                phase: "next_trace_lowering",
                detail: format!(
                    "eval relation op {} in tx={} referenced relation {} but journal recorded {}",
                    op_index, self.tx_index, relation.0, effect.relation.0
                ),
            });
        }
        if effect.inputs != expected_inputs {
            return Err(TabulaError::ProofError {
                phase: "next_trace_lowering",
                detail: format!(
                    "eval relation op {} in tx={} inputs do not match the execution journal",
                    op_index, self.tx_index
                ),
            });
        }
        if effect.outputs.len() != dsts.len() {
            return Err(TabulaError::ProofError {
                phase: "next_trace_lowering",
                detail: format!(
                    "eval relation op {} in tx={} output arity mismatch: journal={} dsts={}",
                    op_index,
                    self.tx_index,
                    effect.outputs.len(),
                    dsts.len(),
                ),
            });
        }

        let input_call = RelationTranscriptCall::from_typed_values(
            self.tx_index,
            effect.effect_ordinal_in_entry,
            op_index as u32,
            TypedTupleRole::RelationInput,
            &effect.inputs,
            self.tuple_encoding_defaults,
            self.encoding_runtimes,
        )?;
        let output_call = RelationTranscriptCall::from_typed_values(
            self.tx_index,
            effect.effect_ordinal_in_entry,
            op_index as u32,
            TypedTupleRole::RelationOutput,
            &effect.outputs,
            self.tuple_encoding_defaults,
            self.encoding_runtimes,
        )?;

        let mut rec = self.empty_record(Opcode::RelationProof);
        rec.effect_ordinal_in_tx = effect.effect_ordinal_in_entry;
        rec.relation_is_eval = true;
        rec.relation_id = Some(relation.0);
        rec.instruction_index = Some(op_index as u32);
        rec.relation_input_digest = Some(core::array::from_fn(|idx| {
            KoalaBear::new(input_call.digest[idx])
        }));
        rec.relation_output_digest = Some(core::array::from_fn(|idx| {
            KoalaBear::new(output_call.digest[idx])
        }));

        for (index, (value_ref, value)) in inputs.0.iter().zip(effect.inputs.iter()).enumerate() {
            let slot = self.resolve_operand_slot(value_ref, value, false, &[])?;
            rec.relation_input_used[index] = true;
            rec.relation_input_type_ids[index] = value.type_id().0;
            rec.relation_input_vals[index] = input_call.tuple_values[index];
            rec.relation_input_sel[index][slot] = true;
        }

        for (index, (dst, output)) in dsts.iter().zip(effect.outputs.iter()).enumerate() {
            let dst_slot = Self::local_slot(*dst)?;
            let encoded = self.encode_padded(output)?;
            self.write_slot(dst_slot, output.clone(), encoded.clone(), false)?;
            rec.written_slots.push(dst_slot);
            rec.writes.push((dst_slot, encoded, false));
            rec.relation_output_used[index] = true;
            rec.relation_output_type_ids[index] = output.type_id().0;
            rec.relation_output_vals[index] = output_call.tuple_values[index];
            rec.relation_output_sel[index][dst_slot] = true;
        }

        let input_digest = input_call.digest;
        let output_digest = output_call.digest;
        self.records.push(rec);
        self.relation_transcript_calls.push(input_call);
        self.relation_transcript_calls.push(output_call);
        self.relation_claims.push(relation_claim_from_effect(
            self.tx_index,
            effect,
            input_digest,
            output_digest,
        ));
        self.current_effect_ordinal = effect.effect_ordinal_in_entry.saturating_add(1);
        Ok(())
    }
}
