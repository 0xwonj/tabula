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
//! ## Authoring modes
//!
//! A kit populates its scratchpad in one of two ways, selected by the
//! chip author based on where the row data becomes available:
//!
//! - **inline-push.** Opcode handlers in `tabula-witness` call a
//!   kit-typed helper during lowering (e.g.
//!   `IrHashKit::push_from_inputs`). The kit owns the row type; the
//!   caller only supplies raw inputs and receives any derived data it
//!   needs. `finalize` drains via [`KitFinalizeContext::take_scratch`],
//!   which yields `T::default()` when no opcode handler ran in a tx —
//!   the correct behavior when zero calls is a valid state.
//!   Adding a new inline-push chip requires no witness-crate edits
//!   beyond calling the new helper from the relevant opcode handler,
//!   and no runtime-crate edits.
//!
//! - **runtime-pre-stuff.** The runtime computes the full row buffer
//!   at batch level and installs it via an `insert_*` helper on the
//!   kit (e.g. `RelationTableKit::insert_rows`) before
//!   `prepare_execution_store` runs. `finalize` drains via
//!   [`KitFinalizeContext::take_scratch_required`], which errors with
//!   [`KitError::MissingScratch`] on absence — a missing pre-stuff is
//!   a wiring bug, not a routine empty-batch case.
//!   Adding a new runtime-pre-stuff chip requires a runtime-crate
//!   edit to call the kit's `insert_*` helper; the SP-3 goal
//!   ("chip-agnostic witness") applies only to `tabula-witness`.
//!
//! Kits that need data only the runtime has (e.g. relation table rows
//! derived from a `PreparedRelationProof` that must not leak into
//! chips) use the runtime-pre-stuff pattern. Everything else uses
//! inline-push.
//!
//! ## Trait home
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

use p3_koala_bear::KoalaBear;

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
}

/// Convention-seal module for [`ChipWitnessKit`].
///
/// Third-party chip authoring is not a goal; this seal makes a new
/// `impl ChipWitnessKit` require an explicit `impl sealed::Sealed`
/// line that stands out in review and CI. A true private-supertrait
/// seal is not achievable because blessed chip impls live in a
/// separate crate (`tabula-chips`), so the module is intentionally
/// `pub`. See SP-5 §9 design rationale.
pub mod sealed {
    /// Marker supertrait required by [`super::ChipWitnessKit`].
    ///
    /// Implement this for a type (alongside `impl ChipWitnessKit`)
    /// only when the type is a blessed workspace chip. The
    /// requirement that this impl must appear in a non-`tabula-stark`
    /// crate makes any new chip visible as a distinct line in review.
    pub trait Sealed {}
}

/// Logical opcode tag for execution-prelude records constructed by the
/// runtime.
///
/// Each variant corresponds to the subset of chip `Opcode` variants that
/// the runtime prelude builder emits when staging context/tx items. The
/// chip-side `From` impl (in `tabula-chips`) maps each tag onto its
/// chip-internal counterpart.
///
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogicalOpcodeTag {
    /// Canonical public-context load item.
    LoadContext,
    /// Canonical transaction header item.
    TxBegin,
    /// Canonical transaction parameter load item.
    LoadParam,
}

/// Logical view of one execution-prelude row emitted by the runtime when
/// staging public-statement and tx-parameter items into the execution
/// witness store.
///
/// Only the fields the runtime actually populates live here; all other
/// chip-row fields inherit their current defaults when the row is lifted
/// into `InstructionRecord` at the chip boundary. This keeps the runtime
/// free of chip-internal layout while preserving byte-identity of the
/// produced chip rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogicalExecutionPrelude {
    /// Which prelude opcode this row stands for (context/tx-begin/param).
    pub opcode: LogicalOpcodeTag,
    /// Transaction index this prelude row belongs to (0 for context items).
    pub tx_index: u32,
    /// Public-statement item index; `None` when the row is not visible on
    /// the public-statement transcript.
    pub proof_meta0: Option<u32>,
    /// Opcode-specific metadata slot 1 (field id, entry id, or param index
    /// depending on the opcode).
    pub proof_meta1: Option<u32>,
    /// Opcode-specific metadata slot 2 (type id or param count, depending
    /// on the opcode).
    pub proof_meta2: Option<u32>,
    /// Reserved execution slots written by this prelude row.
    pub written_slots: Vec<usize>,
    /// Encoded field-element payload for this prelude row.
    pub src1_val: Vec<KoalaBear>,
    /// Per-slot writes: `(slot_index, value_fes, is_null)`.
    pub writes: Vec<(usize, Vec<KoalaBear>, bool)>,
}

/// Logical view of one static relation-table witness row.
///
/// Mirrors the chip-side `RelationTableWitnessRow` one-for-one. Runtime
/// code builds `Vec<LogicalRelationTableRow>` from prepared relation
/// proofs and hands them to the chip-side relation-table kit via a
/// `From`-based conversion so the runtime never names the chip row type
/// directly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogicalRelationTableRow {
    /// Relation identifier.
    pub relation_id: u32,
    /// Canonical input-tuple digest.
    pub input_digest: [u32; 8],
    /// Canonical output-tuple digest.
    pub output_digest: [u32; 8],
    /// Multiplicity on the relation lookup bus.
    pub lookup_mult: u32,
}

/// The chip-authoring protocol for witness-tier row contribution.
pub trait ChipWitnessKit: sealed::Sealed + Send + Sync {
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
        assert!(!scratch.contains_key(&TEST_CHIP), "entry consumed");
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
