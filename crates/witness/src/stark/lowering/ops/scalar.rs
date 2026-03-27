//! Scalar/value-producing opcode lowering.

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
    pub(crate) fn lower_arith(
        &mut self,
        dst: ir::LocalId,
        op: ir::ArithOp,
        lhs: &ir::ValueRef,
        rhs: &ir::ValueRef,
    ) -> Result<(), TabulaError> {
        let lhs_value = self.eval_value(lhs)?;
        let rhs_value = self.eval_value(rhs)?;
        let runtime = self.type_runtimes.resolve(lhs_value.type_id())?;
        let result = runtime.apply_arithmetic(map_arith(op), &lhs_value, &rhs_value)?;
        let lhs_enc = self.encode_padded(&lhs_value)?;
        let rhs_enc = self.encode_padded(&rhs_value)?;
        let dst_enc = self.encode_padded(&result)?;
        let dst_slot = Self::local_slot(dst)?;
        let src1_slot = self.resolve_operand_slot(lhs, &lhs_value, false, &[dst_slot])?;
        let src2_slot = self.resolve_operand_slot(rhs, &rhs_value, false, &[dst_slot])?;
        self.write_slot(dst_slot, result.clone(), dst_enc.clone(), false)?;

        let mut rec = self.empty_record(match op {
            ir::ArithOp::Add => Opcode::Add,
            ir::ArithOp::Sub => Opcode::Sub,
            ir::ArithOp::Mul => Opcode::Mul,
        });
        rec.written_slots = vec![dst_slot];
        rec.src1_val = lhs_enc;
        rec.src2_val = rhs_enc;
        rec.src1_slot_idx = Some(src1_slot);
        rec.src2_slot_idx = Some(src2_slot);
        rec.writes.push((dst_slot, dst_enc, false));
        self.records.push(rec);
        Ok(())
    }

    pub(crate) fn lower_cmp(
        &mut self,
        dst: ir::LocalId,
        op: ir::CmpOp,
        lhs: &ir::ValueRef,
        rhs: &ir::ValueRef,
    ) -> Result<(), TabulaError> {
        let lhs_value = self.eval_value(lhs)?;
        let rhs_value = self.eval_value(rhs)?;
        let runtime = self.type_runtimes.resolve(lhs_value.type_id())?;
        let result = match op {
            ir::CmpOp::Eq => runtime.eq_value(&lhs_value, &rhs_value)?,
            ir::CmpOp::Ne => !runtime.eq_value(&lhs_value, &rhs_value)?,
            ir::CmpOp::Lt => runtime.cmp_value(&lhs_value, &rhs_value)? == std::cmp::Ordering::Less,
            ir::CmpOp::Lte => {
                runtime.cmp_value(&lhs_value, &rhs_value)? != std::cmp::Ordering::Greater
            }
            ir::CmpOp::Gt => {
                runtime.cmp_value(&lhs_value, &rhs_value)? == std::cmp::Ordering::Greater
            }
            ir::CmpOp::Gte => {
                runtime.cmp_value(&lhs_value, &rhs_value)? != std::cmp::Ordering::Less
            }
        };
        let lhs_enc = self.encode_padded(&lhs_value)?;
        let rhs_enc = self.encode_padded(&rhs_value)?;
        let dst_value = bool_typed(result);
        let dst_enc = self.encode_padded(&dst_value)?;
        let dst_slot = Self::local_slot(dst)?;
        let src1_slot = self.resolve_operand_slot(lhs, &lhs_value, false, &[dst_slot])?;
        let src2_slot = self.resolve_operand_slot(rhs, &rhs_value, false, &[dst_slot])?;
        self.write_slot(dst_slot, dst_value, dst_enc.clone(), false)?;

        let mut rec = self.empty_record(Opcode::Cmp(map_cmp(op)));
        rec.written_slots = vec![dst_slot];
        rec.src1_val = lhs_enc;
        rec.src2_val = rhs_enc;
        rec.src1_slot_idx = Some(src1_slot);
        rec.src2_slot_idx = Some(src2_slot);
        rec.writes.push((dst_slot, dst_enc, false));
        self.records.push(rec);
        Ok(())
    }

    pub(crate) fn lower_not(
        &mut self,
        dst: ir::LocalId,
        src: &ir::ValueRef,
    ) -> Result<(), TabulaError> {
        let src_value = self.eval_value(src)?;
        let result = bool_typed(!typed_bool(&src_value, self.type_runtimes)?);
        let src_enc = self.encode_padded(&src_value)?;
        let dst_enc = self.encode_padded(&result)?;
        let dst_slot = Self::local_slot(dst)?;
        let src_slot = self.resolve_operand_slot(src, &src_value, false, &[dst_slot])?;
        self.write_slot(dst_slot, result, dst_enc.clone(), false)?;

        let mut rec = self.empty_record(Opcode::Not);
        rec.written_slots = vec![dst_slot];
        rec.src1_val = src_enc;
        rec.src1_slot_idx = Some(src_slot);
        rec.writes.push((dst_slot, dst_enc, false));
        self.records.push(rec);
        Ok(())
    }

    pub(crate) fn lower_and(
        &mut self,
        dst: ir::LocalId,
        lhs: &ir::ValueRef,
        rhs: &ir::ValueRef,
    ) -> Result<(), TabulaError> {
        let lhs_value = self.eval_value(lhs)?;
        let rhs_value = self.eval_value(rhs)?;
        let result = bool_typed(
            typed_bool(&lhs_value, self.type_runtimes)?
                && typed_bool(&rhs_value, self.type_runtimes)?,
        );
        let lhs_enc = self.encode_padded(&lhs_value)?;
        let rhs_enc = self.encode_padded(&rhs_value)?;
        let dst_enc = self.encode_padded(&result)?;
        let dst_slot = Self::local_slot(dst)?;
        let src1_slot = self.resolve_operand_slot(lhs, &lhs_value, false, &[dst_slot])?;
        let src2_slot = self.resolve_operand_slot(rhs, &rhs_value, false, &[dst_slot])?;
        self.write_slot(dst_slot, result, dst_enc.clone(), false)?;

        let mut rec = self.empty_record(Opcode::And);
        rec.written_slots = vec![dst_slot];
        rec.src1_val = lhs_enc;
        rec.src2_val = rhs_enc;
        rec.src1_slot_idx = Some(src1_slot);
        rec.src2_slot_idx = Some(src2_slot);
        rec.writes.push((dst_slot, dst_enc, false));
        self.records.push(rec);
        Ok(())
    }

    pub(crate) fn lower_or(
        &mut self,
        dst: ir::LocalId,
        lhs: &ir::ValueRef,
        rhs: &ir::ValueRef,
    ) -> Result<(), TabulaError> {
        let lhs_value = self.eval_value(lhs)?;
        let rhs_value = self.eval_value(rhs)?;
        let result = bool_typed(
            typed_bool(&lhs_value, self.type_runtimes)?
                || typed_bool(&rhs_value, self.type_runtimes)?,
        );
        let lhs_enc = self.encode_padded(&lhs_value)?;
        let rhs_enc = self.encode_padded(&rhs_value)?;
        let dst_enc = self.encode_padded(&result)?;
        let dst_slot = Self::local_slot(dst)?;
        let src1_slot = self.resolve_operand_slot(lhs, &lhs_value, false, &[dst_slot])?;
        let src2_slot = self.resolve_operand_slot(rhs, &rhs_value, false, &[dst_slot])?;
        self.write_slot(dst_slot, result, dst_enc.clone(), false)?;

        let mut rec = self.empty_record(Opcode::Or);
        rec.written_slots = vec![dst_slot];
        rec.src1_val = lhs_enc;
        rec.src2_val = rhs_enc;
        rec.src1_slot_idx = Some(src1_slot);
        rec.src2_slot_idx = Some(src2_slot);
        rec.writes.push((dst_slot, dst_enc, false));
        self.records.push(rec);
        Ok(())
    }

    pub(crate) fn lower_select(
        &mut self,
        dst: ir::LocalId,
        cond: &ir::ValueRef,
        if_true: &ir::ValueRef,
        if_false: &ir::ValueRef,
    ) -> Result<(), TabulaError> {
        let cond_value = self.eval_value(cond)?;
        let cond_bool = typed_bool(&cond_value, self.type_runtimes)?;
        let true_value = self.eval_value(if_true)?;
        let false_value = self.eval_value(if_false)?;
        let result = if cond_bool {
            true_value.clone()
        } else {
            false_value.clone()
        };

        let true_enc = self.encode_padded(&true_value)?;
        let false_enc = self.encode_padded(&false_value)?;
        let dst_enc = self.encode_padded(&result)?;
        let dst_slot = Self::local_slot(dst)?;
        let src1_slot = self.resolve_operand_slot(if_true, &true_value, false, &[dst_slot])?;
        let src2_slot = self.resolve_operand_slot(if_false, &false_value, false, &[dst_slot])?;
        let cond_slot = self.resolve_operand_slot(cond, &cond_value, false, &[dst_slot])?;
        self.write_slot(dst_slot, result, dst_enc.clone(), false)?;

        let mut rec = self.empty_record(Opcode::Select);
        rec.written_slots = vec![dst_slot];
        rec.src1_val = true_enc;
        rec.src2_val = false_enc;
        rec.cond_val = cond_bool;
        rec.src1_slot_idx = Some(src1_slot);
        rec.src2_slot_idx = Some(src2_slot);
        rec.cond_slot_idx = Some(cond_slot);
        rec.writes.push((dst_slot, dst_enc, false));
        self.records.push(rec);
        Ok(())
    }

    pub(crate) fn lower_divmod(
        &mut self,
        guard: Option<ir::GuardRef>,
        _op_index: usize,
        dst_q: ir::LocalId,
        dst_r: ir::LocalId,
        lhs: &ir::ValueRef,
        rhs: &ir::ValueRef,
    ) -> Result<(), TabulaError> {
        let lhs_value = self.eval_value(lhs)?;
        let lhs_ty = lhs_value.type_id();
        if !self.guard_active(guard)? {
            let zero_slot = self.ensure_typed_zero_slot(lhs_ty)?;
            let zero_value = self.slots[zero_slot]
                .as_ref()
                .cloned()
                .ok_or_else(|| TabulaError::InvalidIr("missing typed zero slot".into()))?;
            self.copy_into_local(dst_q, zero_slot, zero_value.clone())?;
            self.copy_into_local(dst_r, zero_slot, zero_value)?;
            return Ok(());
        }

        let rhs_value = self.eval_value(rhs)?;
        let runtime = self.type_runtimes.resolve(lhs_ty)?;
        let (q, r) = runtime.divmod(&lhs_value, &rhs_value)?;
        let lhs_enc = self.encode_padded(&lhs_value)?;
        let rhs_enc = self.encode_padded(&rhs_value)?;
        let q_enc = self.encode_padded(&q)?;
        let r_enc = self.encode_padded(&r)?;
        let q_slot = Self::local_slot(dst_q)?;
        let r_slot = Self::local_slot(dst_r)?;
        let src1_slot = self.resolve_operand_slot(lhs, &lhs_value, false, &[q_slot, r_slot])?;
        let src2_slot = self.resolve_operand_slot(rhs, &rhs_value, false, &[q_slot, r_slot])?;
        self.write_slot(q_slot, q, q_enc.clone(), false)?;
        self.write_slot(r_slot, r, r_enc.clone(), false)?;

        let mut rec = self.empty_record(Opcode::DivMod);
        rec.written_slots = vec![q_slot, r_slot];
        rec.src1_val = lhs_enc;
        rec.src2_val = rhs_enc;
        rec.src1_slot_idx = Some(src1_slot);
        rec.src2_slot_idx = Some(src2_slot);
        rec.writes.push((q_slot, q_enc, false));
        rec.writes.push((r_slot, r_enc, false));
        self.records.push(rec);
        Ok(())
    }
}

fn map_arith(op: ir::ArithOp) -> ArithmeticOp {
    match op {
        ir::ArithOp::Add => ArithmeticOp::Add,
        ir::ArithOp::Sub => ArithmeticOp::Sub,
        ir::ArithOp::Mul => ArithmeticOp::Mul,
    }
}

fn map_cmp(op: ir::CmpOp) -> TraceCmpOp {
    match op {
        ir::CmpOp::Eq => TraceCmpOp::Eq,
        ir::CmpOp::Ne => TraceCmpOp::Ne,
        ir::CmpOp::Lt => TraceCmpOp::Lt,
        ir::CmpOp::Lte => TraceCmpOp::Lte,
        ir::CmpOp::Gt => TraceCmpOp::Gt,
        ir::CmpOp::Gte => TraceCmpOp::Gte,
    }
}
