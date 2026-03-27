//! Hash opcode lowering helpers.

use p3_koala_bear::KoalaBear;

use tabula_chips::execution::trace::Opcode;
use tabula_chips::ir_hash::IrHashCall;
use tabula_core::error::TabulaError;
use tabula_ir as ir;
use tabula_types::bytes32_typed;

use super::super::context::LoweringCx;

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
