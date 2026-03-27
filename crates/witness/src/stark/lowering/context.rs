//! Shared lowering context and top-level dispatch.

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

use super::driver::LowerSuccessfulTxInput;
use crate::RelationClaim;

pub(crate) struct LoweringCx<'a, const W: usize> {
    pub(crate) tx_index: u32,
    pub(crate) program: &'a ir::Program,
    pub(crate) entry: &'a ir::Entry,
    pub(crate) params: &'a [TypedValue],
    pub(crate) context: &'a exec::ContextValues,
    pub(crate) empty_columns: &'a BTreeSet<(ir::TableId, ir::FieldId)>,
    pub(crate) type_runtimes: &'a TypeRuntimeRegistry,
    pub(crate) encoding_runtimes: &'a EncodingRuntimeRegistry,
    pub(crate) tuple_encoding_defaults: &'a TupleEncodingDefaults,
    pub(crate) hasher: &'a dyn Hasher,
    pub(crate) records: Vec<InstructionRecord>,
    pub(crate) ir_hash_calls: Vec<IrHashCall>,
    pub(crate) relation_transcript_calls: Vec<RelationTranscriptCall>,
    pub(crate) relation_claims: Vec<RelationClaim>,
    pub(crate) slots: Vec<Option<TypedValue>>,
    pub(crate) slot_fes: Vec<Vec<KoalaBear>>,
    pub(crate) slot_nulls: Vec<bool>,
    pub(crate) slot_initialized: Vec<bool>,
    pub(crate) next_aux_slot: usize,
    pub(crate) current_effect_ordinal: u32,
    pub(crate) true_slot: Option<usize>,
    pub(crate) zero_slot: Option<usize>,
    pub(crate) typed_zero_slots: BTreeMap<tabula_core::TypeId, usize>,
    pub(crate) null_zero_slots: BTreeMap<tabula_core::TypeId, usize>,
    pub(crate) state_effects_by_op: BTreeMap<usize, &'a exec::TypedStateEffect>,
    pub(crate) event_effects_by_op: BTreeMap<usize, &'a exec::TypedEventEffect>,
    pub(crate) relation_effects_by_op: BTreeMap<usize, &'a exec::RelationEffect>,
}

impl<'a, const W: usize> LoweringCx<'a, W> {
    pub(crate) fn new(input: LowerSuccessfulTxInput<'a>) -> Result<Self, TabulaError> {
        let mut state_effects_by_op = BTreeMap::new();
        for effect in input.state_effects {
            if state_effects_by_op
                .insert(effect.op_index, effect)
                .is_some()
            {
                return Err(TabulaError::ProofError {
                    phase: "next_trace_lowering",
                    detail: format!(
                        "duplicate state effect for tx={} op {}",
                        input.tx_index, effect.op_index
                    ),
                });
            }
        }
        let mut event_effects_by_op = BTreeMap::new();
        for effect in input.event_effects {
            if event_effects_by_op
                .insert(effect.op_index, effect)
                .is_some()
            {
                return Err(TabulaError::ProofError {
                    phase: "next_trace_lowering",
                    detail: format!(
                        "duplicate event effect for tx={} op {}",
                        input.tx_index, effect.op_index
                    ),
                });
            }
        }
        let mut relation_effects_by_op = BTreeMap::new();
        for effect in input.relation_effects {
            if relation_effects_by_op
                .insert(effect.op_index, effect)
                .is_some()
            {
                return Err(TabulaError::ProofError {
                    phase: "next_trace_lowering",
                    detail: format!(
                        "duplicate relation effect for tx={} op {}",
                        input.tx_index, effect.op_index
                    ),
                });
            }
        }

        let max_local_slot = input
            .entry
            .body
            .locals
            .iter()
            .map(|local| local.id.0 as usize)
            .max()
            .map_or(0, |slot| slot + 1);

        Ok(Self {
            tx_index: input.tx_index,
            program: input.program,
            entry: input.entry,
            params: &input.call.params,
            context: input.context,
            empty_columns: input.empty_columns,
            type_runtimes: input.type_runtimes,
            encoding_runtimes: input.encoding_runtimes,
            tuple_encoding_defaults: input.tuple_encoding_defaults,
            hasher: input.hasher,
            records: Vec::with_capacity(input.entry.body.ops.len() + max_local_slot),
            ir_hash_calls: Vec::new(),
            relation_transcript_calls: Vec::new(),
            relation_claims: Vec::new(),
            slots: vec![None; MAX_SLOTS],
            slot_fes: vec![vec![KoalaBear::ZERO; W]; MAX_SLOTS],
            slot_nulls: vec![false; MAX_SLOTS],
            slot_initialized: vec![false; MAX_SLOTS],
            next_aux_slot: max_local_slot,
            current_effect_ordinal: 0,
            true_slot: None,
            zero_slot: None,
            typed_zero_slots: BTreeMap::new(),
            null_zero_slots: BTreeMap::new(),
            state_effects_by_op,
            event_effects_by_op,
            relation_effects_by_op,
        })
    }

    pub(crate) fn lower_entry(&mut self) -> Result<(), TabulaError> {
        for (op_index, op) in self.entry.body.ops.iter().enumerate() {
            self.lower_op(op_index, op)?;
        }

        if let Some((&op_index, _)) = self.state_effects_by_op.first_key_value() {
            return Err(TabulaError::ProofError {
                phase: "next_trace_lowering",
                detail: format!(
                    "unconsumed state effect remained for tx={} op {}",
                    self.tx_index, op_index
                ),
            });
        }
        if let Some((&op_index, _)) = self.event_effects_by_op.first_key_value() {
            return Err(TabulaError::ProofError {
                phase: "next_trace_lowering",
                detail: format!(
                    "unconsumed event effect remained for tx={} op {}",
                    self.tx_index, op_index
                ),
            });
        }
        if let Some((&op_index, _)) = self.relation_effects_by_op.first_key_value() {
            return Err(TabulaError::ProofError {
                phase: "next_trace_lowering",
                detail: format!(
                    "unconsumed relation effect remained for tx={} op {}",
                    self.tx_index, op_index
                ),
            });
        }

        Ok(())
    }

    pub(crate) fn lower_op(&mut self, op_index: usize, op: &ir::Op) -> Result<(), TabulaError> {
        match op {
            ir::Op::Arith { dst, op, lhs, rhs } => self.lower_arith(*dst, *op, lhs, rhs),
            ir::Op::Cmp { dst, op, lhs, rhs } => self.lower_cmp(*dst, *op, lhs, rhs),
            ir::Op::Not { dst, src } => self.lower_not(*dst, src),
            ir::Op::And { dst, lhs, rhs } => self.lower_and(*dst, lhs, rhs),
            ir::Op::Or { dst, lhs, rhs } => self.lower_or(*dst, lhs, rhs),
            ir::Op::Select {
                dst,
                cond,
                if_true,
                if_false,
            } => self.lower_select(*dst, cond, if_true, if_false),
            ir::Op::Hash {
                dst,
                family: ir::HashFamily::Poseidon,
                inputs,
            } => self.lower_hash(*dst, inputs),
            ir::Op::DivMod {
                guard,
                dst_q,
                dst_r,
                lhs,
                rhs,
            } => self.lower_divmod(*guard, op_index, *dst_q, *dst_r, lhs, rhs),
            ir::Op::ReadState {
                guard,
                dst_value,
                dst_present,
                table,
                key,
                field,
            } => self.lower_read_state(
                *guard,
                op_index,
                *dst_value,
                *dst_present,
                *table,
                key,
                *field,
            ),
            ir::Op::WriteState {
                guard,
                table,
                key,
                field,
                value,
            } => self.lower_write_state(*guard, op_index, *table, key, *field, value),
            ir::Op::DeleteState {
                guard,
                table,
                key,
                field,
            } => self.lower_delete_state(*guard, op_index, *table, key, *field),
            ir::Op::Assert { guard, cond } => self.lower_assert(*guard, op_index, cond),
            ir::Op::EmitEvent { guard, event, args } => {
                self.lower_emit_event(*guard, op_index, *event, args)
            }
            ir::Op::AssertRelation {
                guard,
                relation,
                args,
            } => self.lower_assert_relation(*guard, op_index, *relation, args),
            ir::Op::EvalRelation {
                guard,
                relation,
                inputs,
                dsts,
            } => self.lower_eval_relation(*guard, op_index, *relation, inputs, dsts),
            ir::Op::ReadStateProperty { .. } | ir::Op::CallCapability { .. } => {
                self.reject_deferred_proof_feature(op_index)
            }
            ir::Op::Return { .. } => Ok(()),
        }
    }

    pub(crate) fn field_type(
        &self,
        table: ir::TableId,
        field: ir::FieldId,
    ) -> Result<tabula_core::TypeId, TabulaError> {
        self.program
            .state
            .tables
            .iter()
            .find(|schema| schema.id == table)
            .and_then(|schema| schema.fields.iter().find(|candidate| candidate.id == field))
            .map(|field| field.ty)
            .ok_or_else(|| TabulaError::InvalidIr(format!("unknown field {}.{}", table.0, field.0)))
    }

    pub(crate) fn take_state_effect(
        &mut self,
        op_index: usize,
        kind: exec::StateEffectKind,
    ) -> Result<&'a exec::TypedStateEffect, TabulaError> {
        let effect =
            self.state_effects_by_op
                .remove(&op_index)
                .ok_or_else(|| TabulaError::ProofError {
                    phase: "next_trace_lowering",
                    detail: format!(
                        "missing state effect for tx={} op {}",
                        self.tx_index, op_index
                    ),
                })?;
        if effect.kind != kind {
            return Err(TabulaError::ProofError {
                phase: "next_trace_lowering",
                detail: format!(
                    "state effect kind mismatch for tx={} op {}: expected {:?}, got {:?}",
                    self.tx_index, op_index, kind, effect.kind
                ),
            });
        }
        Ok(effect)
    }

    pub(crate) fn expect_no_state_effect(&self, op_index: usize) -> Result<(), TabulaError> {
        if self.state_effects_by_op.contains_key(&op_index) {
            return Err(TabulaError::ProofError {
                phase: "next_trace_lowering",
                detail: format!(
                    "inactive op {} in tx={} unexpectedly produced a state effect",
                    op_index, self.tx_index
                ),
            });
        }
        Ok(())
    }

    pub(crate) fn take_relation_effect(
        &mut self,
        op_index: usize,
        kind: exec::RelationEffectKind,
    ) -> Result<&'a exec::RelationEffect, TabulaError> {
        let effect = self
            .relation_effects_by_op
            .remove(&op_index)
            .ok_or_else(|| TabulaError::ProofError {
                phase: "next_trace_lowering",
                detail: format!(
                    "missing relation effect for tx={} op {}",
                    self.tx_index, op_index
                ),
            })?;
        if effect.kind != kind {
            return Err(TabulaError::ProofError {
                phase: "next_trace_lowering",
                detail: format!(
                    "relation op {} in tx={} expected {:?} but journal recorded {:?}",
                    op_index, self.tx_index, kind, effect.kind
                ),
            });
        }
        Ok(effect)
    }

    pub(crate) fn expect_no_relation_effect(&self, op_index: usize) -> Result<(), TabulaError> {
        if self.relation_effects_by_op.contains_key(&op_index) {
            return Err(TabulaError::ProofError {
                phase: "next_trace_lowering",
                detail: format!(
                    "inactive op {} in tx={} unexpectedly produced a relation effect",
                    op_index, self.tx_index
                ),
            });
        }
        Ok(())
    }
}
