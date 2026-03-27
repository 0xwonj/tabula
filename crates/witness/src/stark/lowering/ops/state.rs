//! State read/write/delete lowering.

#![allow(unused_imports)]

use std::collections::{BTreeMap, BTreeSet};

use p3_field::PrimeCharacteristicRing;
use p3_koala_bear::KoalaBear;

use tabula_chips::execution::trace::{CmpOp as TraceCmpOp, InstructionRecord, Opcode};
use tabula_chips::execution::{EXECUTION_STANDARD_VALUE_WIDTH, MAX_SLOTS};
use tabula_chips::ir_hash::IrHashCall;
use tabula_chips::relation_transcript::RelationTranscriptCall;
use tabula_chips::static_table::trace::StaticTableRow;
use tabula_contract::format::typed_tuple::{TupleEncodingDefaults, TypedTupleRole};
use tabula_core::error::TabulaError;
use tabula_core::traits::Hasher;
use tabula_executor as exec;
use tabula_ir as ir;
use tabula_profile::is_u64_type;
use tabula_types::{
    ArithmeticOp, EncodingRuntimeRegistry, TypeRuntimeRegistry, TypedValue, bool_typed,
    bytes32_typed, typed_bool, typed_row_key, u64_typed,
};

use super::super::context::LoweringCx;
use crate::RelationClaim;

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
        rec.access_r = Some(expected_key.row.0);
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

        let mut rec = self.empty_record(Opcode::Write);
        rec.effect_ordinal_in_tx = effect.effect_ordinal_in_entry;
        rec.src1_val = value_enc.clone();
        rec.src1_slot_idx = Some(src_slot);
        rec.access_t = Some(table.0);
        rec.access_c = Some(field.0);
        rec.access_r = Some(expected_key.row.0);
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

        let mut rec = self.empty_record(Opcode::Write);
        rec.effect_ordinal_in_tx = effect.effect_ordinal_in_entry;
        rec.src1_val = zero_enc.clone();
        rec.src1_slot_idx = Some(null_slot);
        rec.access_t = Some(table.0);
        rec.access_c = Some(field.0);
        rec.access_r = Some(expected_key.row.0);
        rec.access_val = Some(zero_enc);
        rec.access_is_null = Some(true);
        self.records.push(rec);
        self.current_effect_ordinal = effect.effect_ordinal_in_entry.saturating_add(1);
        Ok(())
    }
}
