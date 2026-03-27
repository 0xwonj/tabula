//! Hash opcode lowering helpers.

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
    pub(crate) fn lower_hash(
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
}
