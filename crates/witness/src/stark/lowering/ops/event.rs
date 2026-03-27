//! Event emission lowering helpers.

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
        Ok(())
    }
}
