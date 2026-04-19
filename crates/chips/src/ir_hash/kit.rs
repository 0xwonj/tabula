//! `ChipWitnessKit` implementation for the canonical IR hash lane.

use tabula_core::PortableValue;
use tabula_core::error::TabulaError;
use tabula_stark::chips::ChipId;
use tabula_stark::trace::WitnessStore;
use tabula_stark::witness_kit::{ChipWitnessKit, KitError, KitFinalizeContext, KitScratch, sealed};

use super::{IR_HASH_CHIP_ID, IR_HASH_WITNESS_LABEL, IrHashCall};

/// Witness kit for [`super::IrHashChip`]. Accumulates [`IrHashCall`] rows
/// pushed inline by opcode handlers and publishes them under
/// [`IR_HASH_WITNESS_LABEL`] during finalize.
#[derive(Clone, Copy, Debug, Default)]
pub struct IrHashKit;

impl IrHashKit {
    const CHIP_ID: ChipId = IR_HASH_CHIP_ID;

    /// Build a canonical IR-hash call from the portable inputs and push
    /// it into the kit's scratchpad buffer. Returns the final digest as
    /// eight `u32` limbs so the caller can mirror it onto the execution
    /// lane without knowing the row layout.
    pub fn push_from_inputs(
        scratch: &mut KitScratch,
        tx_index: u32,
        instruction_index: u32,
        inputs: &[PortableValue],
    ) -> Result<[u32; 8], TabulaError> {
        let call = IrHashCall::from_inputs(tx_index, instruction_index, inputs)?;
        let digest = call.digest;
        let entry = scratch
            .entry(Self::CHIP_ID)
            .or_insert_with(|| Box::<Vec<IrHashCall>>::default());
        let buf =
            entry
                .downcast_mut::<Vec<IrHashCall>>()
                .ok_or_else(|| TabulaError::ProofError {
                    phase: "witness_kit_push",
                    detail: format!(
                        "IrHashKit scratch downcast failed for chip {}",
                        Self::CHIP_ID
                    ),
                })?;
        buf.push(call);
        Ok(digest)
    }
}

impl sealed::Sealed for IrHashKit {}

impl ChipWitnessKit for IrHashKit {
    fn chip_id(&self) -> ChipId {
        Self::CHIP_ID
    }

    fn witness_store_label(&self) -> &'static str {
        IR_HASH_WITNESS_LABEL
    }

    fn finalize(
        &self,
        ctx: &mut KitFinalizeContext<'_>,
        store: &mut WitnessStore,
    ) -> Result<(), KitError> {
        let calls: Vec<IrHashCall> = ctx.take_scratch(Self::CHIP_ID)?;
        store.put(IR_HASH_WITNESS_LABEL, calls);
        Ok(())
    }
}
