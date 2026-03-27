//! Control-only lowering helpers.

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
    pub(crate) fn lower_assert(
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
}
