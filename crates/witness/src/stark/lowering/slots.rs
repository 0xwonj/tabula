//! Slot materialization and value-resolution helpers for lowering.

use p3_field::PrimeCharacteristicRing;
use p3_koala_bear::KoalaBear;

use tabula_chips::execution::trace::{InstructionRecord, Opcode};
use tabula_chips::execution::{EXECUTION_STANDARD_VALUE_WIDTH, MAX_SLOTS};
use tabula_core::CommittedKey;
use tabula_core::error::TabulaError;
use tabula_ir as ir;
use tabula_types::{NativeKeyPayload, TypedValue, bool_typed, typed_bool, u64_typed};

use super::context::LoweringCx;

impl<'a, const W: usize> LoweringCx<'a, W> {
    pub(crate) fn eval_value(&self, value: &ir::ValueRef) -> Result<TypedValue, TabulaError> {
        match value {
            ir::ValueRef::Literal(value) => self.type_runtimes.decode_portable(value),
            ir::ValueRef::Param(id) => self
                .params
                .get(id.0 as usize)
                .cloned()
                .ok_or_else(|| TabulaError::InvalidIr(format!("missing param {}", id.0))),
            ir::ValueRef::Context(id) => {
                self.context.fields.get(id).cloned().ok_or_else(|| {
                    TabulaError::InvalidIr(format!("missing context field {}", id.0))
                })
            }
            ir::ValueRef::Local(id) => {
                let slot = Self::local_slot(*id)?;
                self.slots[slot]
                    .as_ref()
                    .cloned()
                    .ok_or_else(|| TabulaError::InvalidIr(format!("unassigned local {}", id.0)))
            }
            ir::ValueRef::Const(id) => self
                .program
                .const_pool
                .entries
                .iter()
                .find(|entry| entry.id == *id)
                .ok_or_else(|| TabulaError::InvalidIr(format!("missing const {}", id.0)))
                .and_then(|entry| self.type_runtimes.decode_portable(&entry.value)),
        }
    }

    pub(crate) fn eval_tuple(
        &self,
        values: &ir::ValueTupleRef,
    ) -> Result<Vec<TypedValue>, TabulaError> {
        values
            .0
            .iter()
            .map(|value| self.eval_value(value))
            .collect()
    }

    pub(crate) fn encode_padded(&self, value: &TypedValue) -> Result<Vec<KoalaBear>, TabulaError> {
        let encoding_profile_id = self.tuple_encoding_defaults.resolve(value.type_id())?;
        let mut fes = self
            .encoding_runtimes
            .encode_field_elements_for_profile(encoding_profile_id, value)?;
        if fes.len() > W {
            return Err(TabulaError::ProofError {
                phase: "next_trace_lowering",
                detail: format!(
                    "value of type {} encoded width {} exceeds execution trace width {}",
                    value.type_id().0,
                    fes.len(),
                    W
                ),
            });
        }
        fes.resize(W, KoalaBear::ZERO);
        Ok(fes)
    }

    pub(crate) fn resolve_operand_slot(
        &mut self,
        value_ref: &ir::ValueRef,
        value: &TypedValue,
        is_null: bool,
        exclude_slots: &[usize],
    ) -> Result<usize, TabulaError> {
        match value_ref {
            ir::ValueRef::Local(id) => {
                let slot = Self::local_slot(*id)?;
                if !self.slot_initialized[slot] {
                    return Err(TabulaError::InvalidIr(format!(
                        "operand local {} is uninitialized",
                        id.0
                    )));
                }
                Ok(slot)
            }
            ir::ValueRef::Param(id) => {
                self.param_slot_by_id
                    .get(id)
                    .copied()
                    .ok_or_else(|| TabulaError::ProofError {
                        phase: "next_trace_lowering",
                        detail: format!(
                            "tx={} param {} is missing the reserved proof-claim slot",
                            self.tx_index, id.0
                        ),
                    })
            }
            ir::ValueRef::Context(id) => {
                self.context_slot_by_id
                    .get(id)
                    .copied()
                    .ok_or_else(|| TabulaError::ProofError {
                        phase: "next_trace_lowering",
                        detail: format!(
                            "tx={} context field {} is missing the reserved proof-claim slot",
                            self.tx_index, id.0
                        ),
                    })
            }
            ir::ValueRef::Const(_) | ir::ValueRef::Literal(_) => {
                let encoded = self.encode_padded(value)?;
                if let Some(slot) = self.find_materialized_slot(&encoded, is_null, exclude_slots) {
                    return Ok(slot);
                }
                if is_null {
                    let slot = self.ensure_null_zero_slot(value.type_id())?;
                    if self.slot_fes[slot] == encoded {
                        return Ok(slot);
                    }
                }
                self.materialize_non_null_slot(value.clone(), encoded, is_null)
            }
        }
    }

    pub(crate) fn copy_into_local(
        &mut self,
        dst: ir::LocalId,
        src_slot: usize,
        value: TypedValue,
    ) -> Result<(), TabulaError> {
        let dst_slot = Self::local_slot(dst)?;
        let dst_enc = self.encode_padded(&value)?;
        self.write_slot(dst_slot, value, dst_enc.clone(), false)?;
        let zero_slot = self.ensure_zero_slot()?;
        let true_slot = self.ensure_true_slot()?;

        let mut rec = self.empty_record(Opcode::Select);
        rec.written_slots = vec![dst_slot];
        rec.src1_val = self.slot_fes[src_slot].clone();
        rec.src2_val = self.slot_fes[zero_slot].clone();
        rec.cond_val = true;
        rec.src1_slot_idx = Some(src_slot);
        rec.src2_slot_idx = Some(zero_slot);
        rec.cond_slot_idx = Some(true_slot);
        rec.writes.push((dst_slot, dst_enc, false));
        self.records.push(rec);
        Ok(())
    }

    pub(crate) fn materialize_non_null_slot(
        &mut self,
        value: TypedValue,
        encoded: Vec<KoalaBear>,
        is_null: bool,
    ) -> Result<usize, TabulaError> {
        if is_null {
            return Err(TabulaError::ProofError {
                phase: "next_trace_lowering",
                detail: "materialize_non_null_slot called for a null operand".into(),
            });
        }
        let slot = self.alloc_slot()?;
        self.write_slot(slot, value, encoded.clone(), false)?;
        let zero_slot = self.ensure_zero_slot()?;
        let true_slot = self.ensure_true_slot()?;

        let mut rec = self.empty_record(Opcode::Select);
        rec.written_slots = vec![slot];
        rec.src1_val = encoded.clone();
        rec.src2_val = self.slot_fes[zero_slot].clone();
        rec.cond_val = true;
        rec.src1_slot_idx = Some(slot);
        rec.src2_slot_idx = Some(zero_slot);
        rec.cond_slot_idx = Some(true_slot);
        rec.writes.push((slot, encoded, false));
        self.records.push(rec);
        Ok(slot)
    }

    pub(crate) fn ensure_zero_slot(&mut self) -> Result<usize, TabulaError> {
        if let Some(slot) = self.zero_slot {
            return Ok(slot);
        }
        let slot = self.alloc_slot()?;
        self.write_slot(slot, u64_typed(0), vec![KoalaBear::ZERO; W], false)?;
        self.zero_slot = Some(slot);
        Ok(slot)
    }

    pub(crate) fn ensure_true_slot(&mut self) -> Result<usize, TabulaError> {
        if let Some(slot) = self.true_slot {
            return Ok(slot);
        }
        let zero_slot = self.ensure_zero_slot()?;
        let slot = self.alloc_slot()?;
        let value = bool_typed(true);
        let encoded = self.encode_padded(&value)?;
        self.write_slot(slot, value, encoded.clone(), false)?;

        let mut rec = self.empty_record(Opcode::Not);
        rec.written_slots = vec![slot];
        rec.src1_val = self.slot_fes[zero_slot].clone();
        rec.src1_slot_idx = Some(zero_slot);
        rec.writes.push((slot, encoded, false));
        self.records.push(rec);

        self.true_slot = Some(slot);
        Ok(slot)
    }

    pub(crate) fn ensure_bool_slot(&mut self, value: bool) -> Result<usize, TabulaError> {
        if value {
            self.ensure_true_slot()
        } else {
            self.ensure_zero_slot()
        }
    }

    pub(crate) fn ensure_typed_zero_slot(
        &mut self,
        ty: tabula_core::TypeId,
    ) -> Result<usize, TabulaError> {
        if let Some(slot) = self.typed_zero_slots.get(&ty).copied() {
            return Ok(slot);
        }
        let zero = self.type_runtimes.zero_of(ty)?;
        let encoded = self.encode_padded(&zero)?;
        let slot = self.materialize_non_null_slot(zero, encoded, false)?;
        self.typed_zero_slots.insert(ty, slot);
        Ok(slot)
    }

    pub(crate) fn ensure_null_zero_slot(
        &mut self,
        ty: tabula_core::TypeId,
    ) -> Result<usize, TabulaError> {
        if let Some(slot) = self.null_zero_slots.get(&ty).copied() {
            return Ok(slot);
        }
        let value = self.type_runtimes.zero_of(ty)?;
        let encoded = self.encode_padded(&value)?;
        let zero_slot = self.ensure_zero_slot()?;
        let slot = self.alloc_slot()?;
        self.write_slot(slot, value, encoded.clone(), true)?;

        let mut rec = self.empty_record(Opcode::Add);
        rec.written_slots = vec![slot];
        rec.src1_val = self.slot_fes[zero_slot].clone();
        rec.src2_val = self.slot_fes[zero_slot].clone();
        rec.src1_slot_idx = Some(zero_slot);
        rec.src2_slot_idx = Some(zero_slot);
        rec.writes.push((slot, encoded, true));
        self.records.push(rec);

        self.null_zero_slots.insert(ty, slot);
        Ok(slot)
    }

    pub(crate) fn alloc_slot(&mut self) -> Result<usize, TabulaError> {
        if self.next_aux_slot >= self.aux_slot_limit {
            return Err(TabulaError::ProofError {
                phase: "next_trace_lowering",
                detail: format!(
                    "slot allocation exceeded aux-slot limit {} in tx={} (MAX_SLOTS={MAX_SLOTS})",
                    self.aux_slot_limit, self.tx_index
                ),
            });
        }
        let slot = self.next_aux_slot;
        self.next_aux_slot += 1;
        Ok(slot)
    }

    pub(crate) fn find_materialized_slot(
        &self,
        encoded: &[KoalaBear],
        is_null: bool,
        exclude_slots: &[usize],
    ) -> Option<usize> {
        (0..self.next_aux_slot)
            .filter(|slot| !exclude_slots.contains(slot))
            .find(|slot| {
                self.slot_initialized[*slot]
                    && self.slot_nulls[*slot] == is_null
                    && self.slot_fes[*slot] == encoded
            })
    }

    pub(crate) fn local_slot(id: ir::LocalId) -> Result<usize, TabulaError> {
        let slot = id.0 as usize;
        if slot >= MAX_SLOTS {
            return Err(TabulaError::ProofError {
                phase: "next_trace_lowering",
                detail: format!("local slot {slot} exceeds MAX_SLOTS ({MAX_SLOTS})"),
            });
        }
        Ok(slot)
    }

    pub(crate) fn local_type(&self, id: ir::LocalId) -> Result<tabula_core::TypeId, TabulaError> {
        self.entry
            .body
            .locals
            .iter()
            .find(|local| local.id == id)
            .map(|local| local.ty)
            .ok_or_else(|| TabulaError::InvalidIr(format!("unknown local {}", id.0)))
    }

    pub(crate) fn write_slot(
        &mut self,
        slot: usize,
        value: TypedValue,
        encoded: Vec<KoalaBear>,
        is_null: bool,
    ) -> Result<(), TabulaError> {
        if slot >= MAX_SLOTS {
            return Err(TabulaError::ProofError {
                phase: "next_trace_lowering",
                detail: format!("slot {slot} exceeds MAX_SLOTS ({MAX_SLOTS})"),
            });
        }
        self.slots[slot] = Some(value);
        self.slot_fes[slot] = encoded;
        self.slot_nulls[slot] = is_null;
        self.slot_initialized[slot] = true;
        Ok(())
    }

    pub(crate) fn empty_record(&self, opcode: Opcode) -> InstructionRecord {
        InstructionRecord {
            opcode,
            tx_index: self.tx_index,
            effect_ordinal_in_tx: self.current_effect_ordinal,
            written_slots: vec![],
            src1_val: vec![KoalaBear::ZERO; W],
            src2_val: vec![KoalaBear::ZERO; W],
            cond_val: false,
            src1_slot_idx: None,
            src2_slot_idx: None,
            cond_slot_idx: None,
            access_t: None,
            access_c: None,
            access_r: None,
            access_val: None,
            access_is_null: None,
            writes: vec![],
            hash_digest: None,
            is_empty_col: false,
            capability_transcript_id: None,
            instruction_index: None,
            capability_input_count: None,
            capability_output_count: None,
            capability_event_digest: None,
            property_query_type: None,
            property_query_arg0: vec![],
            property_query_arg1: vec![],
            property_result_val: vec![],
            property_result_key: vec![],
            property_result_is_null: false,
            relation_is_eval: false,
            relation_id: None,
            relation_input_digest: None,
            relation_output_digest: None,
            relation_input_used: [false; MAX_SLOTS],
            relation_input_type_ids: [0; MAX_SLOTS],
            relation_output_used: [false; MAX_SLOTS],
            relation_output_type_ids: [0; MAX_SLOTS],
            relation_input_vals: [[KoalaBear::ZERO; EXECUTION_STANDARD_VALUE_WIDTH]; MAX_SLOTS],
            relation_output_vals: [[KoalaBear::ZERO; EXECUTION_STANDARD_VALUE_WIDTH]; MAX_SLOTS],
            relation_input_sel: [[false; MAX_SLOTS]; MAX_SLOTS],
            relation_output_sel: [[false; MAX_SLOTS]; MAX_SLOTS],
            proof_meta0: None,
            proof_meta1: None,
            proof_meta2: None,
            proof_meta3: None,
        }
    }

    pub(crate) fn guard_active(&self, guard: Option<ir::GuardRef>) -> Result<bool, TabulaError> {
        match guard {
            Some(guard) => {
                let slot = Self::local_slot(guard.0)?;
                let value = self.slots[slot].as_ref().ok_or_else(|| {
                    TabulaError::InvalidIr(format!("missing guard local {}", guard.0.0))
                })?;
                typed_bool(value, self.type_runtimes)
            }
            None => Ok(true),
        }
    }

    pub(crate) fn resolve_cell_key(
        &self,
        table: ir::TableId,
        field: ir::FieldId,
        key: &ir::ValueTupleRef,
    ) -> Result<tabula_core::CommittedCellKey, TabulaError> {
        self.state_runtime
            .encode_cell_key(table, field, &self.eval_tuple(key)?)
    }

    pub(crate) fn proof_key_payload(
        &self,
        table: ir::TableId,
        key: &CommittedKey,
    ) -> Result<NativeKeyPayload, TabulaError> {
        self.state_runtime.encode_key_payload(table, key)
    }
}
