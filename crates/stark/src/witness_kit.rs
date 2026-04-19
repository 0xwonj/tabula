//! Chip-authoring protocol for witness-tier row contribution.
//!
//! Each concrete chip that contributes rows to the execution-tier
//! [`WitnessStore`] implements a [`ChipWitnessKit`]. The kit owns:
//!
//! - the chip's [`ChipId`] and the canonical witness-store label under
//!   which its rows live,
//! - a `finalize` step that drains the kit's accumulated rows from a
//!   [`KitScratch`] entry and writes them into the execution
//!   [`WitnessStore`].
//!
//! Opcode handlers in `tabula-witness` push rows into the kit through
//! kit-typed helpers the kit exposes on its own interface — the row
//! type itself stays private to the chip crate. See SP-3 design spec
//! `docs/superpowers/specs/2026-04-19-sp3-witness-chip-kit-design.md`.
//!
//! The trait lives in `tabula-stark` rather than `tabula-ext` (as the
//! SP-3 design originally proposed) because `tabula-ext` sits above
//! `tabula-machine` and `tabula-witness` in the dependency graph,
//! while both of those crates need to reference the trait. `tabula-stark`
//! already owns [`ChipId`] and [`WitnessStore`], so the chip-authoring
//! protocol seam belongs here alongside the rest of the chip
//! identification framework.

use std::any::Any;
use std::collections::BTreeMap;

use crate::chips::ChipId;
use crate::trace::WitnessStore;

/// Per-kit opaque scratchpad map. Each kit owns one entry keyed by its
/// own [`ChipId`]; the boxed value is downcast by the kit to its
/// private row buffer type.
pub type KitScratch = BTreeMap<ChipId, Box<dyn Any + Send>>;

/// Borrow surfaced to a [`ChipWitnessKit::finalize`] call. Currently
/// exposes the kit's scratchpad entry via a downcast-and-take accessor.
/// Later stages may surface additional read-only borrows (e.g. prepared
/// relation proof handles).
pub struct KitFinalizeContext<'a> {
    scratch: &'a mut KitScratch,
}

impl<'a> KitFinalizeContext<'a> {
    /// Construct a finalize context borrowing the driver's scratchpad.
    /// Intended to be called by the witness-tier lowering driver only;
    /// kits receive this by borrow and never construct their own.
    #[doc(hidden)]
    pub fn new(scratch: &'a mut KitScratch) -> Self {
        Self { scratch }
    }

    /// Remove the entry under `chip_id` and downcast to `T`.
    ///
    /// Returns `T::default()` when no entry is present. Suitable for
    /// *inline-push* kits: opcode handlers may run zero times in a tx,
    /// leaving no scratchpad entry, and publishing an empty row buffer
    /// is the correct behavior.
    pub fn take_scratch<T>(&mut self, chip_id: ChipId) -> Result<T, KitError>
    where
        T: Any + Default + Send,
    {
        match self.scratch.remove(&chip_id) {
            None => Ok(T::default()),
            Some(boxed) => boxed
                .downcast::<T>()
                .map(|b| *b)
                .map_err(|_| KitError::DowncastFailed(chip_id)),
        }
    }

    /// Remove the entry under `chip_id` and downcast to `T`, erroring
    /// with [`KitError::MissingScratch`] if absent.
    ///
    /// Suitable for *runtime-pre-stuff* kits: the runtime is expected
    /// to install the row buffer before `prepare_execution_store`
    /// runs, so a missing entry signals a wiring bug (the buffer
    /// wasn't pre-stuffed) that should surface loudly rather than
    /// silently publish an empty buffer.
    pub fn take_scratch_required<T>(&mut self, chip_id: ChipId) -> Result<T, KitError>
    where
        T: Any + Send,
    {
        match self.scratch.remove(&chip_id) {
            None => Err(KitError::MissingScratch(chip_id)),
            Some(boxed) => boxed
                .downcast::<T>()
                .map(|b| *b)
                .map_err(|_| KitError::DowncastFailed(chip_id)),
        }
    }
}

/// Errors a kit may raise during finalize.
#[derive(Debug, thiserror::Error)]
pub enum KitError {
    /// The kit's scratchpad entry was absent when finalize ran.
    #[error("kit scratchpad entry missing for chip {0}")]
    MissingScratch(ChipId),
    /// The kit's scratchpad entry existed but was the wrong concrete type.
    #[error("kit scratchpad downcast failed for chip {0}")]
    DowncastFailed(ChipId),
    /// Arbitrary kit-internal failure with a message.
    #[error("kit finalize failed for chip {chip}: {message}")]
    Internal {
        /// Chip raising the error.
        chip: ChipId,
        /// Kit-supplied message describing the failure.
        message: String,
    },
}

/// The chip-authoring protocol for witness-tier row contribution.
pub trait ChipWitnessKit: Send + Sync {
    /// Stable identifier this kit populates rows for. Must match the
    /// `ChipId` of the AIR its owning execution backend registers.
    fn chip_id(&self) -> ChipId;

    /// Canonical witness-store label under which this kit's rows live
    /// in the execution-tier [`WitnessStore`]. Matches the string the
    /// chip's AIR reads from.
    fn witness_store_label(&self) -> &'static str;

    /// Drain the kit's accumulated rows from the scratchpad and write
    /// them into the execution [`WitnessStore`] under
    /// `witness_store_label()`.
    fn finalize(
        &self,
        ctx: &mut KitFinalizeContext<'_>,
        store: &mut WitnessStore,
    ) -> Result<(), KitError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_CHIP: ChipId = ChipId(9999);

    #[test]
    fn take_scratch_returns_default_when_absent() {
        let mut scratch = KitScratch::new();
        let mut ctx = KitFinalizeContext::new(&mut scratch);
        let v: Vec<u32> = ctx.take_scratch(TEST_CHIP).expect("absent -> default");
        assert!(v.is_empty());
    }

    #[test]
    fn take_scratch_round_trips_stored_vector() {
        let mut scratch = KitScratch::new();
        scratch.insert(TEST_CHIP, Box::new(vec![1u32, 2, 3]));
        let mut ctx = KitFinalizeContext::new(&mut scratch);
        let v: Vec<u32> = ctx.take_scratch(TEST_CHIP).expect("present -> value");
        assert_eq!(v, vec![1, 2, 3]);
        assert!(scratch.get(&TEST_CHIP).is_none(), "entry consumed");
    }

    #[test]
    fn take_scratch_required_errors_when_absent() {
        let mut scratch = KitScratch::new();
        let mut ctx = KitFinalizeContext::new(&mut scratch);
        let err = ctx
            .take_scratch_required::<Vec<u32>>(TEST_CHIP)
            .expect_err("absent -> MissingScratch");
        assert!(matches!(err, KitError::MissingScratch(c) if c == TEST_CHIP));
    }

    #[test]
    fn take_scratch_reports_downcast_failure() {
        let mut scratch = KitScratch::new();
        scratch.insert(TEST_CHIP, Box::new(42u64));
        let mut ctx = KitFinalizeContext::new(&mut scratch);
        let err = ctx
            .take_scratch::<Vec<u32>>(TEST_CHIP)
            .expect_err("type mismatch");
        assert!(matches!(err, KitError::DowncastFailed(c) if c == TEST_CHIP));
    }
}
