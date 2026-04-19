// SP-5 §9 compile-fail probe: implementing ChipWitnessKit without
// impl sealed::Sealed must not compile. This fixture simulates an
// external author who adds impl ChipWitnessKit without the required
// impl sealed::Sealed marker.
use tabula_stark::chips::ChipId;
use tabula_stark::trace::WitnessStore;
use tabula_stark::witness_kit::{ChipWitnessKit, KitError, KitFinalizeContext};

struct ExternalChip;

impl ChipWitnessKit for ExternalChip {
    fn chip_id(&self) -> ChipId {
        ChipId(9999)
    }
    fn witness_store_label(&self) -> &'static str {
        "external"
    }
    fn finalize(
        &self,
        _ctx: &mut KitFinalizeContext<'_>,
        _store: &mut WitnessStore,
    ) -> Result<(), KitError> {
        Ok(())
    }
}

fn main() {}
