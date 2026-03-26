//! Next-native witness lowering from canonical `tabula_ir` execution.

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

use crate::RelationClaim;
use crate::relation::relation_claim_from_effect;

/// Input bundle for lowering one successful native transaction.
#[derive(Clone, Copy)]
pub struct LowerSuccessfulTxInput<'a> {
    /// Zero-based transaction index within the batch.
    pub tx_index: u32,
    /// Canonical program containing the entry being lowered.
    pub program: &'a ir::Program,
    /// Resolved transaction call.
    pub call: &'a exec::TxCall,
    /// Entry definition being lowered.
    pub entry: &'a ir::Entry,
    /// Execution context values.
    pub context: &'a exec::ContextValues,
    /// Proof-relevant state effects emitted by the executor.
    pub state_effects: &'a [exec::TypedStateEffect],
    /// Proof-relevant event effects emitted by the executor.
    pub event_effects: &'a [exec::TypedEventEffect],
    /// Proof-relevant relation effects emitted by the executor.
    pub relation_effects: &'a [exec::RelationEffect],
    /// Columns known to be empty in the committed pre-state.
    pub empty_columns: &'a BTreeSet<(ir::TableId, ir::FieldId)>,
    /// Installed type runtimes used for typed semantics.
    pub type_runtimes: &'a TypeRuntimeRegistry,
    /// Installed encoding runtimes used for execution-lane witness encoding.
    pub encoding_runtimes: &'a EncodingRuntimeRegistry,
    /// Compiler-sealed tuple-encoding defaults used for tuple/static-table
    /// digests and execution witness encoding.
    pub tuple_encoding_defaults: &'a TupleEncodingDefaults,
    /// Installed canonical IR hash family implementation.
    pub hasher: &'a dyn Hasher,
}

/// Output of full native execution lowering.
#[derive(Debug, Clone)]
pub struct LoweringOutput {
    /// Instruction records for all opcodes across all successful txs.
    pub instruction_records: Vec<InstructionRecord>,
    /// Static table rows accumulated from lookup-like operations.
    pub static_table_rows: Vec<StaticTableRow>,
    /// Canonical IR-hash calls consumed by the dedicated hash lane.
    pub ir_hash_calls: Vec<IrHashCall>,
    /// Relation transcript calls consumed by the dedicated relation transcript lane.
    pub relation_transcript_calls: Vec<RelationTranscriptCall>,
    /// Relation claims aggregated across all successful txs.
    pub relation_claims: Vec<RelationClaim>,
}

/// Output of lowering one successful native transaction.
#[derive(Debug, Clone)]
pub struct TxLoweringOutput {
    /// Instruction records for all ops in the entry body.
    pub instruction_records: Vec<InstructionRecord>,
    /// Static table rows accumulated while lowering this entry.
    pub static_table_rows: Vec<StaticTableRow>,
    /// Canonical IR-hash calls consumed by the dedicated hash lane.
    pub ir_hash_calls: Vec<IrHashCall>,
    /// Relation transcript calls for this tx.
    pub relation_transcript_calls: Vec<RelationTranscriptCall>,
    /// Relation claims for this tx.
    pub relation_claims: Vec<RelationClaim>,
}

/// Merge per-tx lowering outputs into one execution-tier bundle.
pub fn merge_lowering_outputs<'a>(
    outputs: impl IntoIterator<Item = &'a TxLoweringOutput>,
) -> LoweringOutput {
    let mut instruction_records = Vec::new();
    let mut static_rows: BTreeMap<(u32, u16, u64), StaticTableRow> = BTreeMap::new();
    let mut ir_hash_calls = Vec::new();
    let mut relation_transcript_calls = Vec::new();
    let mut relation_claims = Vec::new();

    for output in outputs {
        instruction_records.extend(output.instruction_records.iter().cloned());
        ir_hash_calls.extend(output.ir_hash_calls.iter().cloned());
        relation_transcript_calls.extend(output.relation_transcript_calls.iter().cloned());
        relation_claims.extend(output.relation_claims.iter().cloned());
        for row in &output.static_table_rows {
            let key = (row.table_id, row.col_id, row.row_key);
            static_rows
                .entry(key)
                .and_modify(|existing| existing.lookup_mult += row.lookup_mult)
                .or_insert_with(|| row.clone());
        }
    }

    LoweringOutput {
        instruction_records,
        static_table_rows: static_rows.into_values().collect(),
        ir_hash_calls,
        relation_transcript_calls,
        relation_claims,
    }
}

/// Lower one successful native transaction into witness-ready execution records.
pub fn lower_successful_tx<const W: usize>(
    input: LowerSuccessfulTxInput<'_>,
) -> Result<TxLoweringOutput, TabulaError> {
    let mut lowering = LoweringCx::<W>::new(input)?;
    lowering.lower_entry()?;
    Ok(TxLoweringOutput {
        instruction_records: lowering.records,
        static_table_rows: Vec::new(),
        ir_hash_calls: lowering.ir_hash_calls,
        relation_transcript_calls: lowering.relation_transcript_calls,
        relation_claims: lowering.relation_claims,
    })
}

struct LoweringCx<'a, const W: usize> {
    tx_index: u32,
    program: &'a ir::Program,
    entry: &'a ir::Entry,
    params: &'a [TypedValue],
    context: &'a exec::ContextValues,
    empty_columns: &'a BTreeSet<(ir::TableId, ir::FieldId)>,
    type_runtimes: &'a TypeRuntimeRegistry,
    encoding_runtimes: &'a EncodingRuntimeRegistry,
    tuple_encoding_defaults: &'a TupleEncodingDefaults,
    hasher: &'a dyn Hasher,
    records: Vec<InstructionRecord>,
    ir_hash_calls: Vec<IrHashCall>,
    relation_transcript_calls: Vec<RelationTranscriptCall>,
    relation_claims: Vec<RelationClaim>,
    slots: Vec<Option<TypedValue>>,
    slot_fes: Vec<Vec<KoalaBear>>,
    slot_nulls: Vec<bool>,
    slot_initialized: Vec<bool>,
    next_aux_slot: usize,
    current_effect_ordinal: u32,
    true_slot: Option<usize>,
    zero_slot: Option<usize>,
    typed_zero_slots: BTreeMap<tabula_core::TypeId, usize>,
    null_zero_slots: BTreeMap<tabula_core::TypeId, usize>,
    state_effects_by_op: BTreeMap<usize, &'a exec::TypedStateEffect>,
    event_effects_by_op: BTreeMap<usize, &'a exec::TypedEventEffect>,
    relation_effects_by_op: BTreeMap<usize, &'a exec::RelationEffect>,
}

impl<'a, const W: usize> LoweringCx<'a, W> {
    fn new(input: LowerSuccessfulTxInput<'a>) -> Result<Self, TabulaError> {
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

    fn lower_entry(&mut self) -> Result<(), TabulaError> {
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

    fn lower_op(&mut self, op_index: usize, op: &ir::Op) -> Result<(), TabulaError> {
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
                Err(TabulaError::ProofError {
                    phase: "next_trace_lowering",
                    detail: format!(
                        "op {op_index} in tx={} uses a deferred proof feature that is not part of the core-first native proving cutover",
                        self.tx_index
                    ),
                })
            }
            ir::Op::Return { .. } => Ok(()),
        }
    }

    fn lower_arith(
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

    fn lower_cmp(
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

    fn lower_not(&mut self, dst: ir::LocalId, src: &ir::ValueRef) -> Result<(), TabulaError> {
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

    fn lower_and(
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

    fn lower_or(
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

    fn lower_select(
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

    fn lower_hash(
        &mut self,
        dst: ir::LocalId,
        inputs: &ir::ValueTupleRef,
    ) -> Result<(), TabulaError> {
        let typed_inputs = self.eval_tuple(inputs)?;
        let portable_inputs = typed_inputs
            .iter()
            .map(|value| self.type_runtimes.encode_typed(value))
            .collect::<Result<Vec<_>, _>>()?;
        let instruction_index = self.records.len() as u32;
        let call = IrHashCall::from_inputs(self.tx_index, instruction_index, &portable_inputs)?;
        let digest = self.hasher.hash_ir(&portable_inputs);
        let digest_typed = bytes32_typed(digest);
        let dst_enc = call
            .digest
            .iter()
            .take(W)
            .map(|value| KoalaBear::new(*value))
            .collect::<Vec<_>>();
        let dst_slot = Self::local_slot(dst)?;
        self.write_slot(dst_slot, digest_typed, dst_enc.clone(), false)?;

        let mut rec = self.empty_record(Opcode::Hash);
        rec.written_slots = vec![dst_slot];
        rec.instruction_index = Some(instruction_index);
        rec.hash_digest = Some(core::array::from_fn(|index| {
            KoalaBear::new(call.digest[index])
        }));
        rec.writes.push((dst_slot, dst_enc, false));
        self.records.push(rec);
        self.ir_hash_calls.push(call);
        Ok(())
    }

    fn lower_divmod(
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

    #[allow(clippy::too_many_arguments)]
    fn lower_read_state(
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

    fn lower_write_state(
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

    fn lower_delete_state(
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

    fn lower_assert(
        &mut self,
        guard: Option<ir::GuardRef>,
        _op_index: usize,
        cond: &ir::ValueRef,
    ) -> Result<(), TabulaError> {
        if !self.guard_active(guard)? {
            return Ok(());
        }
        let cond_value = self.eval_value(cond)?;
        if !typed_bool(&cond_value, self.type_runtimes)? {
            return Err(TabulaError::ProofError {
                phase: "next_trace_lowering",
                detail: format!(
                    "assert in tx={} lowered after unsuccessful execution",
                    self.tx_index
                ),
            });
        }
        let cond_enc = self.encode_padded(&cond_value)?;
        let cond_slot = self.resolve_operand_slot(cond, &cond_value, false, &[])?;

        let mut rec = self.empty_record(Opcode::Assert);
        rec.src1_val = cond_enc;
        rec.src1_slot_idx = Some(cond_slot);
        self.records.push(rec);
        Ok(())
    }

    fn lower_emit_event(
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
        Ok(())
    }

    fn lower_assert_relation(
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

    fn lower_eval_relation(
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

    fn eval_value(&self, value: &ir::ValueRef) -> Result<TypedValue, TabulaError> {
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

    fn eval_tuple(&self, values: &ir::ValueTupleRef) -> Result<Vec<TypedValue>, TabulaError> {
        values
            .0
            .iter()
            .map(|value| self.eval_value(value))
            .collect()
    }

    fn encode_padded(&self, value: &TypedValue) -> Result<Vec<KoalaBear>, TabulaError> {
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

    fn resolve_operand_slot(
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
            ir::ValueRef::Param(_)
            | ir::ValueRef::Context(_)
            | ir::ValueRef::Const(_)
            | ir::ValueRef::Literal(_) => {
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

    fn copy_into_local(
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

    fn materialize_non_null_slot(
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

    fn ensure_zero_slot(&mut self) -> Result<usize, TabulaError> {
        if let Some(slot) = self.zero_slot {
            return Ok(slot);
        }
        let slot = self.alloc_slot()?;
        self.write_slot(slot, u64_typed(0), vec![KoalaBear::ZERO; W], false)?;
        self.zero_slot = Some(slot);
        Ok(slot)
    }

    fn ensure_true_slot(&mut self) -> Result<usize, TabulaError> {
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

    fn ensure_bool_slot(&mut self, value: bool) -> Result<usize, TabulaError> {
        if value {
            self.ensure_true_slot()
        } else {
            self.ensure_zero_slot()
        }
    }

    fn ensure_typed_zero_slot(&mut self, ty: tabula_core::TypeId) -> Result<usize, TabulaError> {
        if let Some(slot) = self.typed_zero_slots.get(&ty).copied() {
            return Ok(slot);
        }
        let zero = self.type_runtimes.zero_of(ty)?;
        let encoded = self.encode_padded(&zero)?;
        let slot = self.materialize_non_null_slot(zero, encoded, false)?;
        self.typed_zero_slots.insert(ty, slot);
        Ok(slot)
    }

    fn ensure_null_zero_slot(&mut self, ty: tabula_core::TypeId) -> Result<usize, TabulaError> {
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

    fn alloc_slot(&mut self) -> Result<usize, TabulaError> {
        if self.next_aux_slot >= MAX_SLOTS {
            return Err(TabulaError::ProofError {
                phase: "next_trace_lowering",
                detail: format!(
                    "slot allocation exceeded MAX_SLOTS ({MAX_SLOTS}) in tx={}",
                    self.tx_index
                ),
            });
        }
        let slot = self.next_aux_slot;
        self.next_aux_slot += 1;
        Ok(slot)
    }

    fn find_materialized_slot(
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

    fn local_slot(id: ir::LocalId) -> Result<usize, TabulaError> {
        let slot = id.0 as usize;
        if slot >= MAX_SLOTS {
            return Err(TabulaError::ProofError {
                phase: "next_trace_lowering",
                detail: format!("local slot {slot} exceeds MAX_SLOTS ({MAX_SLOTS})"),
            });
        }
        Ok(slot)
    }

    fn local_type(&self, id: ir::LocalId) -> Result<tabula_core::TypeId, TabulaError> {
        self.entry
            .body
            .locals
            .iter()
            .find(|local| local.id == id)
            .map(|local| local.ty)
            .ok_or_else(|| TabulaError::InvalidIr(format!("unknown local {}", id.0)))
    }

    fn write_slot(
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

    fn empty_record(&self, opcode: Opcode) -> InstructionRecord {
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
        }
    }

    fn guard_active(&self, guard: Option<ir::GuardRef>) -> Result<bool, TabulaError> {
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

    fn resolve_cell_key(
        &self,
        table: ir::TableId,
        field: ir::FieldId,
        key: &ir::ValueTupleRef,
    ) -> Result<tabula_core::CellKey, TabulaError> {
        let schema = self
            .program
            .state
            .tables
            .iter()
            .find(|schema| schema.id == table)
            .ok_or_else(|| TabulaError::InvalidIr(format!("unknown table {}", table.0)))?;
        if schema.key_tys.len() != 1 || !is_u64_type(schema.key_tys[0]) {
            return Err(TabulaError::InvalidIr(format!(
                "V1 native witness lowering only supports [u64] state keys, table {} declared {:?}",
                table.0,
                schema.key_tys.iter().map(|ty| ty.0).collect::<Vec<_>>()
            )));
        }
        if key.0.len() != 1 {
            return Err(TabulaError::InvalidIr(
                "V1 native witness lowering only supports single-component state keys".into(),
            ));
        }
        let row = typed_row_key(&self.eval_value(&key.0[0])?, self.type_runtimes)?;
        Ok(tabula_core::CellKey {
            table: table.into(),
            col: field.into(),
            row,
        })
    }

    fn field_type(
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

    fn take_state_effect(
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

    fn expect_no_state_effect(&self, op_index: usize) -> Result<(), TabulaError> {
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

    fn take_relation_effect(
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

    fn expect_no_relation_effect(&self, op_index: usize) -> Result<(), TabulaError> {
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
