// SP-5 §9 compile-pass probe: implementing ChipWitnessKit *with*
// impl sealed::Sealed must compile successfully. This guards against
// a trivially-unreachable seal (where the seal appears enforced
// because Sealed itself cannot be named, not because the supertrait
// requirement is real).
use tabula_stark::chips::ChipId;
use tabula_stark::trace::WitnessStore;
use tabula_stark::witness_kit::{ChipWitnessKit, KitError, KitFinalizeContext, sealed};

struct ExternalChip;

impl sealed::Sealed for ExternalChip {}

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
