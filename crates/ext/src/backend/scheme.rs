#[cfg(feature = "prove")]
use crate::error::ExtResult;
#[cfg(feature = "prove")]
use tabula_commitment::{ColumnRootBinding, NormalizedVerifierDigest};
pub use tabula_core::SchemeId;
#[cfg(feature = "prove")]
use tabula_stark::trace::WitnessStore;
#[cfg(feature = "prove")]
use tabula_witness::{CommittedEntry, PropertyReadClaim};

/// Canonical per-column delta handed to the profile-centric proof backend.
#[cfg(feature = "prove")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedColumnDelta {
    /// Table identifier.
    pub table: tabula_core::TableId,
    /// Column identifier.
    pub col: tabula_core::ColId,
    /// Base-state init cells grouped for this column.
    pub init_cells: Vec<tabula_witness::InitCell>,
    /// Execution access events for this column.
    pub access_events: Vec<tabula_witness::AccessEvent>,
    /// Final coalesced writes for this column.
    pub writes: Vec<tabula_witness::ColumnWrite>,
    /// Whether the batch contains at least one effective final write.
    pub is_touched: bool,
}

/// Canonical backend-neutral proof preparation context.
#[cfg(feature = "prove")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ColumnProofContext {
    /// Ordered logical delta for this column slot.
    pub column: PreparedColumnDelta,
    /// Old committed-state entries for the column.
    pub old_entries: Vec<CommittedEntry>,
    /// Structural property-read claims for this column.
    pub property_reads: Vec<PropertyReadClaim>,
}

/// Canonical prepared proof product for one materialized column backend.
#[cfg(feature = "prove")]
pub struct PreparedColumnProof {
    /// Verifier-visible digest before the batch.
    pub old_digest: NormalizedVerifierDigest,
    /// Verifier-visible digest after the batch.
    pub new_digest: NormalizedVerifierDigest,
    /// Optional canonical root binding for this column.
    pub root_binding: Option<ColumnRootBinding>,
    /// Backend witness store for this column tier.
    pub store: WitnessStore,
}

#[cfg(feature = "prove")]
impl std::fmt::Debug for PreparedColumnProof {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedColumnProof")
            .field("root_binding", &self.root_binding)
            .finish_non_exhaustive()
    }
}

/// Canonical profile-centric proof backend for one materialized column slot.
#[cfg(feature = "prove")]
pub trait ColumnProofBackend: Send + Sync {
    /// Human-readable scheme name.
    fn name(&self) -> &str;

    /// Portable scheme identifier.
    fn scheme_id(&self) -> SchemeId;

    /// Prepare the final per-column proof store and canonical root binding.
    fn prepare_column(&self, context: ColumnProofContext) -> ExtResult<PreparedColumnProof>;
}
