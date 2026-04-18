//! `ChipWitnessKit` implementation for the relation transcript lane.

use p3_koala_bear::KoalaBear;

use tabula_contract::format::typed_tuple::{
    TYPED_TUPLE_MAX_SLOTS, TYPED_TUPLE_VALUE_WIDTH, TupleEncodingDefaults, TypedTupleRole,
};
use tabula_core::error::TabulaError;
use tabula_stark::chips::ChipId;
use tabula_stark::trace::WitnessStore;
use tabula_stark::witness_kit::{ChipWitnessKit, KitError, KitFinalizeContext, KitScratch};
use tabula_types::{EncodingRuntimeRegistry, TypedValue};

use super::{
    RELATION_TRANSCRIPT_CHIP_ID, RELATION_TRANSCRIPT_WITNESS_LABEL, RelationTranscriptCall,
};

/// Digest + padded tuple-value projection returned to the caller when a
/// relation transcript call is pushed into the kit scratchpad. The
/// caller mirrors these onto the execution record without needing to
/// name [`RelationTranscriptCall`].
#[derive(Clone, Copy, Debug)]
pub struct RelationTranscriptDigest {
    /// Final transcript digest as eight `u32` limbs.
    pub digest: [u32; 8],
    /// Padded field-element encodings per tuple position.
    pub tuple_values: [[KoalaBear; TYPED_TUPLE_VALUE_WIDTH]; TYPED_TUPLE_MAX_SLOTS],
}

/// Witness kit for [`super::RelationTranscriptChip`]. Accumulates
/// [`RelationTranscriptCall`] rows pushed inline by opcode handlers
/// and publishes them under [`RELATION_TRANSCRIPT_WITNESS_LABEL`]
/// during finalize.
#[derive(Clone, Copy, Debug, Default)]
pub struct RelationTranscriptKit;

impl RelationTranscriptKit {
    const CHIP_ID: ChipId = RELATION_TRANSCRIPT_CHIP_ID;

    /// Build one canonical relation transcript call and push it into
    /// the kit's scratchpad buffer. Returns the transcript digest and
    /// padded tuple values so the caller can stamp them onto the
    /// execution record without naming the row struct.
    #[allow(clippy::too_many_arguments)]
    pub fn push_from_typed_values(
        scratch: &mut KitScratch,
        tx_index: u32,
        effect_ordinal_in_tx: u32,
        instruction_index: u32,
        role: TypedTupleRole,
        values: &[TypedValue],
        tuple_encoding_defaults: &TupleEncodingDefaults,
        encoding_runtimes: &EncodingRuntimeRegistry,
    ) -> Result<RelationTranscriptDigest, TabulaError> {
        let call = RelationTranscriptCall::from_typed_values(
            tx_index,
            effect_ordinal_in_tx,
            instruction_index,
            role,
            values,
            tuple_encoding_defaults,
            encoding_runtimes,
        )?;
        let projection = RelationTranscriptDigest {
            digest: call.digest,
            tuple_values: call.tuple_values,
        };
        let entry = scratch
            .entry(Self::CHIP_ID)
            .or_insert_with(|| Box::<Vec<RelationTranscriptCall>>::default());
        entry
            .downcast_mut::<Vec<RelationTranscriptCall>>()
            .expect("RelationTranscriptKit scratch type mismatch")
            .push(call);
        Ok(projection)
    }
}

impl ChipWitnessKit for RelationTranscriptKit {
    fn chip_id(&self) -> ChipId {
        Self::CHIP_ID
    }

    fn witness_store_label(&self) -> &'static str {
        RELATION_TRANSCRIPT_WITNESS_LABEL
    }

    fn finalize(
        &self,
        ctx: &mut KitFinalizeContext<'_>,
        store: &mut WitnessStore,
    ) -> Result<(), KitError> {
        let calls: Vec<RelationTranscriptCall> = ctx.take_scratch(Self::CHIP_ID)?;
        store.put(RELATION_TRANSCRIPT_WITNESS_LABEL, calls);
        Ok(())
    }
}
