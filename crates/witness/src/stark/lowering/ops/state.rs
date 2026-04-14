//! State read/write/delete lowering.

use tabula_chips::execution::trace::Opcode;
use tabula_core::error::TabulaError;
use tabula_core::{CommittedPropertyQuery, PropertyAggregateKind, PropertyQueryKind};
use tabula_executor as exec;
use tabula_ir as ir;
use tabula_types::{NativeKeyPayload, bool_typed, zero_key_payload};

use super::super::context::LoweringCx;

impl<'a, const W: usize> LoweringCx<'a, W> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn lower_read_state(
        &mut self,
        guard: Option<ir::GuardRef>,
        op_index: usize,
        dst_value: ir::LocalId,
        dst_present: ir::LocalId,
        table: ir::TableId,
        key: &ir::ValueTupleRef,
        field: ir::FieldId,
    ) -> Result<(), TabulaError> {
        let field_ty = self.field_type(table, field)?;
        if !self.guard_active(guard)? {
            self.expect_no_state_effect(op_index)?;
            let zero_slot = self.ensure_typed_zero_slot(field_ty)?;
            let zero_value = self.slots[zero_slot]
                .as_ref()
                .cloned()
                .ok_or_else(|| TabulaError::InvalidIr("missing typed zero slot".into()))?;
            self.copy_into_local(dst_value, zero_slot, zero_value)?;
            let false_slot = self.ensure_bool_slot(false)?;
            self.copy_into_local(dst_present, false_slot, bool_typed(false))?;
            return Ok(());
        }

        let effect = self.take_state_effect(op_index, exec::StateEffectKind::Read)?;
        let expected_key = self.resolve_cell_key(table, field, key)?;
        if effect.key != expected_key {
            return Err(TabulaError::ProofError {
                phase: "next_trace_lowering",
                detail: format!(
                    "read op {} in tx={} resolved key {:?} but journal recorded {:?}",
                    op_index, self.tx_index, expected_key, effect.key
                ),
            });
        }
        if effect.type_id != field_ty {
            return Err(TabulaError::ProofError {
                phase: "next_trace_lowering",
                detail: format!(
                    "read op {} in tx={} expected field type {} but journal recorded {}",
                    op_index, self.tx_index, field_ty.0, effect.type_id.0
                ),
            });
        }

        let is_null = effect.value.is_none();
        let access_row = self.proof_key_payload(table, &expected_key.key)?;
        let raw_value = match &effect.value {
            Some(value) => value.clone(),
            None => self.type_runtimes.zero_of(field_ty)?,
        };
        let raw_enc = self.encode_padded(&raw_value)?;
        let raw_slot = self.alloc_slot()?;
        self.write_slot(raw_slot, raw_value.clone(), raw_enc.clone(), is_null)?;

        let mut rec = self.empty_record(Opcode::Read);
        rec.effect_ordinal_in_tx = effect.effect_ordinal_in_entry;
        rec.written_slots = vec![raw_slot];
        rec.access_t = Some(table.0);
        rec.access_c = Some(field.0);
        rec.access_r = Some(access_row);
        rec.access_val = Some(raw_enc.clone());
        rec.access_is_null = Some(is_null);
        rec.is_empty_col = self.empty_columns.contains(&(table, field));
        rec.writes.push((raw_slot, raw_enc, is_null));
        self.records.push(rec);
        self.current_effect_ordinal = effect.effect_ordinal_in_entry.saturating_add(1);

        if is_null {
            let zero_slot = self.ensure_typed_zero_slot(field_ty)?;
            let zero_value = self.slots[zero_slot]
                .as_ref()
                .cloned()
                .ok_or_else(|| TabulaError::InvalidIr("missing typed zero slot".into()))?;
            self.copy_into_local(dst_value, zero_slot, zero_value)?;
        } else {
            self.copy_into_local(dst_value, raw_slot, raw_value)?;
        }
        let present_slot = self.ensure_bool_slot(!is_null)?;
        self.copy_into_local(dst_present, present_slot, bool_typed(!is_null))?;
        Ok(())
    }

    pub(crate) fn lower_write_state(
        &mut self,
        guard: Option<ir::GuardRef>,
        op_index: usize,
        table: ir::TableId,
        key: &ir::ValueTupleRef,
        field: ir::FieldId,
        value: &ir::ValueRef,
    ) -> Result<(), TabulaError> {
        if !self.guard_active(guard)? {
            self.expect_no_state_effect(op_index)?;
            return Ok(());
        }

        let effect = self.take_state_effect(op_index, exec::StateEffectKind::Write)?;
        let field_ty = self.field_type(table, field)?;
        let expected_key = self.resolve_cell_key(table, field, key)?;
        if effect.key != expected_key {
            return Err(TabulaError::ProofError {
                phase: "next_trace_lowering",
                detail: format!(
                    "write op {} in tx={} resolved key {:?} but journal recorded {:?}",
                    op_index, self.tx_index, expected_key, effect.key
                ),
            });
        }
        if effect.type_id != field_ty {
            return Err(TabulaError::ProofError {
                phase: "next_trace_lowering",
                detail: format!(
                    "write op {} in tx={} expected field type {} but journal recorded {}",
                    op_index, self.tx_index, field_ty.0, effect.type_id.0
                ),
            });
        }

        let written_value = self.eval_value(value)?;
        let journal_value = effect
            .value
            .as_ref()
            .ok_or_else(|| TabulaError::ProofError {
                phase: "next_trace_lowering",
                detail: format!(
                    "write op {} in tx={} recorded a null journal value",
                    op_index, self.tx_index
                ),
            })?;
        if &written_value != journal_value {
            return Err(TabulaError::ProofError {
                phase: "next_trace_lowering",
                detail: format!(
                    "write op {} in tx={} value does not match journal",
                    op_index, self.tx_index
                ),
            });
        }
        let value_enc = self.encode_padded(&written_value)?;
        let src_slot = self.resolve_operand_slot(value, &written_value, false, &[])?;
        let access_row = self.proof_key_payload(table, &expected_key.key)?;

        let mut rec = self.empty_record(Opcode::Write);
        rec.effect_ordinal_in_tx = effect.effect_ordinal_in_entry;
        rec.src1_val = value_enc.clone();
        rec.src1_slot_idx = Some(src_slot);
        rec.access_t = Some(table.0);
        rec.access_c = Some(field.0);
        rec.access_r = Some(access_row);
        rec.access_val = Some(value_enc);
        rec.access_is_null = Some(false);
        self.records.push(rec);
        self.current_effect_ordinal = effect.effect_ordinal_in_entry.saturating_add(1);
        Ok(())
    }

    pub(crate) fn lower_delete_state(
        &mut self,
        guard: Option<ir::GuardRef>,
        op_index: usize,
        table: ir::TableId,
        key: &ir::ValueTupleRef,
        field: ir::FieldId,
    ) -> Result<(), TabulaError> {
        if !self.guard_active(guard)? {
            self.expect_no_state_effect(op_index)?;
            return Ok(());
        }

        let effect = self.take_state_effect(op_index, exec::StateEffectKind::Delete)?;
        let field_ty = self.field_type(table, field)?;
        let expected_key = self.resolve_cell_key(table, field, key)?;
        if effect.key != expected_key {
            return Err(TabulaError::ProofError {
                phase: "next_trace_lowering",
                detail: format!(
                    "delete op {} in tx={} resolved key {:?} but journal recorded {:?}",
                    op_index, self.tx_index, expected_key, effect.key
                ),
            });
        }
        if effect.type_id != field_ty {
            return Err(TabulaError::ProofError {
                phase: "next_trace_lowering",
                detail: format!(
                    "delete op {} in tx={} expected field type {} but journal recorded {}",
                    op_index, self.tx_index, field_ty.0, effect.type_id.0
                ),
            });
        }

        let null_slot = self.ensure_null_zero_slot(field_ty)?;
        let zero_value = self.type_runtimes.zero_of(field_ty)?;
        let zero_enc = self.encode_padded(&zero_value)?;
        let access_row = self.proof_key_payload(table, &expected_key.key)?;

        let mut rec = self.empty_record(Opcode::Write);
        rec.effect_ordinal_in_tx = effect.effect_ordinal_in_entry;
        rec.src1_val = zero_enc.clone();
        rec.src1_slot_idx = Some(null_slot);
        rec.access_t = Some(table.0);
        rec.access_c = Some(field.0);
        rec.access_r = Some(access_row);
        rec.access_val = Some(zero_enc);
        rec.access_is_null = Some(true);
        self.records.push(rec);
        self.current_effect_ordinal = effect.effect_ordinal_in_entry.saturating_add(1);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn lower_read_state_property(
        &mut self,
        guard: Option<ir::GuardRef>,
        op_index: usize,
        dst_value: ir::LocalId,
        dst_key_components: &[ir::LocalId],
        dst_is_null: ir::LocalId,
        table: ir::TableId,
        field: ir::FieldId,
        query: &ir::StatePropertyQuery,
    ) -> Result<(), TabulaError> {
        let field_ty = self.field_type(table, field)?;
        let key_component_tys = self.state_runtime.key_component_types(table)?;
        if dst_key_components.len() != key_component_tys.len() {
            return Err(TabulaError::InvalidIr(format!(
                "property read op {} in tx={} expected {} key destinations, got {}",
                op_index,
                self.tx_index,
                key_component_tys.len(),
                dst_key_components.len()
            )));
        }
        if !self.guard_active(guard)? {
            self.expect_no_property_effect(op_index)?;
            let zero_slot = self.ensure_typed_zero_slot(field_ty)?;
            let zero_value = self.slots[zero_slot]
                .as_ref()
                .cloned()
                .ok_or_else(|| TabulaError::InvalidIr("missing typed zero slot".into()))?;
            self.copy_into_local(dst_value, zero_slot, zero_value)?;
            for (dst, ty) in dst_key_components
                .iter()
                .zip(key_component_tys.iter().copied())
            {
                let key_zero_slot = self.ensure_typed_zero_slot(ty)?;
                let key_zero_value = self.slots[key_zero_slot]
                    .as_ref()
                    .cloned()
                    .ok_or_else(|| TabulaError::InvalidIr("missing typed zero slot".into()))?;
                self.copy_into_local(*dst, key_zero_slot, key_zero_value)?;
            }
            let false_slot = self.ensure_bool_slot(false)?;
            self.copy_into_local(dst_is_null, false_slot, bool_typed(false))?;
            return Ok(());
        }

        let effect = self.take_property_effect(op_index)?;
        let expected_query = self.lower_committed_property_query(table, query)?;
        if effect.query != expected_query {
            return Err(TabulaError::ProofError {
                phase: "next_trace_lowering",
                detail: format!(
                    "property read op {} in tx={} resolved query {:?} but journal recorded {:?}",
                    op_index, self.tx_index, expected_query, effect.query
                ),
            });
        }
        if effect.result.value.type_id() != field_ty {
            return Err(TabulaError::ProofError {
                phase: "next_trace_lowering",
                detail: format!(
                    "property read op {} in tx={} expected field type {} but journal recorded {}",
                    op_index,
                    self.tx_index,
                    field_ty.0,
                    effect.result.value.type_id().0
                ),
            });
        }

        let result_value = effect.result.value.clone();
        let result_value_enc = self.encode_padded(&result_value)?;
        let result_key = effect.result.key.clone();
        let result_key_payload = match result_key.as_ref() {
            Some(key) => self.state_runtime.encode_key_payload(table, key)?,
            None => zero_key_payload(),
        };
        let result_key_values = match result_key.as_ref() {
            Some(key) => self.state_runtime.decode_committed_key(table, key)?,
            None => key_component_tys
                .iter()
                .copied()
                .map(|ty| self.type_runtimes.zero_of(ty))
                .collect::<Result<Vec<_>, _>>()?,
        };
        if result_key_values.len() != dst_key_components.len() {
            return Err(TabulaError::ProofError {
                phase: "next_trace_lowering",
                detail: format!(
                    "property read op {} in tx={} produced {} key components but {} destinations were declared",
                    op_index,
                    self.tx_index,
                    result_key_values.len(),
                    dst_key_components.len()
                ),
            });
        }
        let result_key_value =
            result_key_values
                .first()
                .cloned()
                .ok_or_else(|| TabulaError::ProofError {
                    phase: "next_trace_lowering",
                    detail: format!(
                        "property read op {} in tx={} currently requires exactly one key component",
                        op_index, self.tx_index
                    ),
                })?;
        let result_key_enc = result_key_payload.to_vec();
        let result_is_null_value = bool_typed(effect.result.is_null);
        let result_is_null_enc = self.encode_padded(&result_is_null_value)?;

        let value_slot = Self::local_slot(dst_value)?;
        let key_slot = Self::local_slot(*dst_key_components.first().ok_or_else(|| {
            TabulaError::ProofError {
                phase: "next_trace_lowering",
                detail: format!(
                    "property read op {} in tx={} expected one key destination",
                    op_index, self.tx_index
                ),
            }
        })?)?;
        let is_null_slot = Self::local_slot(dst_is_null)?;

        self.write_slot(
            value_slot,
            result_value.clone(),
            result_value_enc.clone(),
            false,
        )?;
        self.write_slot(
            key_slot,
            result_key_value.clone(),
            result_key_enc.clone(),
            false,
        )?;
        self.write_slot(
            is_null_slot,
            result_is_null_value.clone(),
            result_is_null_enc.clone(),
            false,
        )?;

        let (query_type, query_arg0, query_arg1) =
            self.encode_property_query_witness(table, &expected_query)?;

        let mut rec = self.empty_record(Opcode::PropertyRead);
        rec.effect_ordinal_in_tx = effect.effect_ordinal_in_entry;
        rec.written_slots = vec![value_slot, key_slot, is_null_slot];
        rec.access_t = Some(table.0);
        rec.access_c = Some(field.0);
        rec.property_query_type = Some(query_type.ordinal());
        rec.property_query_arg0 = query_arg0.to_vec();
        rec.property_query_arg1 = query_arg1.to_vec();
        rec.property_result_val = result_value_enc.clone();
        rec.property_result_key = result_key_enc.clone();
        rec.property_result_is_null = effect.result.is_null;
        rec.writes.push((value_slot, result_value_enc, false));
        rec.writes.push((key_slot, result_key_enc, false));
        rec.writes.push((is_null_slot, result_is_null_enc, false));
        self.records.push(rec);
        self.current_effect_ordinal = effect.effect_ordinal_in_entry.saturating_add(1);
        Ok(())
    }

    fn lower_committed_property_query(
        &self,
        table: ir::TableId,
        query: &ir::StatePropertyQuery,
    ) -> Result<CommittedPropertyQuery, TabulaError> {
        Ok(match query {
            ir::StatePropertyQuery::Minimum => CommittedPropertyQuery::Minimum,
            ir::StatePropertyQuery::Maximum => CommittedPropertyQuery::Maximum,
            ir::StatePropertyQuery::Successor { key } => CommittedPropertyQuery::Successor {
                key: self
                    .state_runtime
                    .encode_committed_key(table, &self.eval_tuple(key)?)?,
            },
            ir::StatePropertyQuery::Predecessor { key } => CommittedPropertyQuery::Predecessor {
                key: self
                    .state_runtime
                    .encode_committed_key(table, &self.eval_tuple(key)?)?,
            },
            ir::StatePropertyQuery::NonExistenceRange { lower, upper } => {
                CommittedPropertyQuery::NonExistenceRange {
                    lower: self
                        .state_runtime
                        .encode_committed_key(table, &self.eval_tuple(lower)?)?,
                    upper: self
                        .state_runtime
                        .encode_committed_key(table, &self.eval_tuple(upper)?)?,
                }
            }
            ir::StatePropertyQuery::Aggregate { kind } => CommittedPropertyQuery::Aggregate {
                kind: match kind {
                    ir::AggregateKind::Sum => PropertyAggregateKind::Sum,
                    ir::AggregateKind::Count => PropertyAggregateKind::Count,
                },
            },
        })
    }

    fn encode_property_query_witness(
        &self,
        table: ir::TableId,
        query: &CommittedPropertyQuery,
    ) -> Result<(PropertyQueryKind, NativeKeyPayload, NativeKeyPayload), TabulaError> {
        let zero = zero_key_payload();
        Ok(match query {
            CommittedPropertyQuery::Minimum => (PropertyQueryKind::Minimum, zero, zero),
            CommittedPropertyQuery::Maximum => (PropertyQueryKind::Maximum, zero, zero),
            CommittedPropertyQuery::Successor { key } => (
                PropertyQueryKind::Successor,
                self.state_runtime.encode_key_payload(table, key)?,
                zero,
            ),
            CommittedPropertyQuery::Predecessor { key } => (
                PropertyQueryKind::Predecessor,
                self.state_runtime.encode_key_payload(table, key)?,
                zero,
            ),
            CommittedPropertyQuery::NonExistenceRange { lower, upper } => (
                PropertyQueryKind::NonExistenceRange,
                self.state_runtime.encode_key_payload(table, lower)?,
                self.state_runtime.encode_key_payload(table, upper)?,
            ),
            CommittedPropertyQuery::Aggregate { .. } => (PropertyQueryKind::Aggregate, zero, zero),
        })
    }
}
