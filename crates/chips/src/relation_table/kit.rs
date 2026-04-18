//! `ChipWitnessKit` implementation for the relation table chip.

use tabula_stark::chips::ChipId;
use tabula_stark::trace::WitnessStore;
use tabula_stark::witness_kit::{ChipWitnessKit, KitError, KitFinalizeContext, KitScratch};

use super::{RELATION_TABLE_CHIP_ID, RELATION_TABLE_WITNESS_LABEL, RelationTableWitnessRow};

/// Witness kit for [`super::RelationTableChip`]. Uses the
/// *runtime-pre-stuff* authoring pattern: the runtime materializes the
/// row buffer from the prepared relation proof and inserts it into
/// [`KitScratch`] via [`insert_rows`] before `finalize` runs; `finalize`
/// then drains the buffer and publishes it under
/// [`RELATION_TABLE_WITNESS_LABEL`].
#[derive(Clone, Copy, Debug, Default)]
pub struct RelationTableKit;

impl RelationTableKit {
    const CHIP_ID: ChipId = RELATION_TABLE_CHIP_ID;

    /// Install the runtime-prepared relation-table rows into the kit's
    /// scratchpad entry.
    pub fn insert_rows(scratch: &mut KitScratch, rows: Vec<RelationTableWitnessRow>) {
        scratch.insert(Self::CHIP_ID, Box::new(rows));
    }
}

impl ChipWitnessKit for RelationTableKit {
    fn chip_id(&self) -> ChipId {
        Self::CHIP_ID
    }

    fn witness_store_label(&self) -> &'static str {
        RELATION_TABLE_WITNESS_LABEL
    }

    fn finalize(
        &self,
        ctx: &mut KitFinalizeContext<'_>,
        store: &mut WitnessStore,
    ) -> Result<(), KitError> {
        let rows: Vec<RelationTableWitnessRow> = ctx.take_scratch(Self::CHIP_ID)?;
        store.put(RELATION_TABLE_WITNESS_LABEL, rows);
        Ok(())
    }
}
