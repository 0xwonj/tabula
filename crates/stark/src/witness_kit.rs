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
use std::marker::PhantomData;

use crate::chips::ChipId;
use crate::trace::WitnessStore;

/// Per-kit opaque scratchpad map. Each kit owns one entry keyed by its
/// own [`ChipId`]; the boxed value is downcast by the kit to its
/// private row buffer type.
pub type KitScratch = BTreeMap<ChipId, Box<dyn Any + Send>>;

/// Opaque borrow surfaced to a [`ChipWitnessKit::finalize`] call. In S1
/// this is a stub; later stages (starting S2) populate it with the
/// merged core lowering output, prepared relation proof handles, and
/// the kit's downcastable scratchpad entry.
pub struct KitFinalizeContext<'a> {
    _marker: PhantomData<&'a ()>,
}

impl<'a> KitFinalizeContext<'a> {
    /// Construct an empty finalize context. Used by internal drivers;
    /// kits receive this by borrow and never construct their own.
    #[doc(hidden)]
    pub fn new() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

impl Default for KitFinalizeContext<'_> {
    fn default() -> Self {
        Self::new()
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
