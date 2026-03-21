use std::sync::Arc;

use tabula_artifact::SchemeDescriptor;
#[cfg(feature = "prove")]
use tabula_commitment::ColumnMeta;
pub use tabula_core::SchemeId;
#[cfg(feature = "prove")]
use tabula_stark::trace::WitnessStore;
#[cfg(feature = "prove")]
use tabula_witness::{CommittedEntry, PreparedExecutionColumn, PropertyReadClaim};

use crate::backend::ProofColumn;
use crate::error::ExtResult;
use crate::scheme::ResolvedColumnPlan;

/// Backend-neutral per-column proof preparation context.
#[cfg(feature = "prove")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ColumnProofContext {
    /// Ordered logical column inputs for this proof slot.
    pub column: PreparedExecutionColumn,
    /// Old committed-state entries for the column.
    pub old_entries: Vec<CommittedEntry>,
    /// Structural property-read claims for this column.
    pub property_reads: Vec<PropertyReadClaim>,
}

/// Prepared backend-aware proof product for one column.
#[cfg(feature = "prove")]
pub struct PreparedColumnProof {
    /// Verifier-visible column metadata.
    pub meta: ColumnMeta,
    /// Backend witness store for this column tier.
    pub store: WitnessStore,
}

#[cfg(feature = "prove")]
impl std::fmt::Debug for PreparedColumnProof {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedColumnProof")
            .field("meta", &self.meta)
            .finish_non_exhaustive()
    }
}

/// Per-column proof preparer.
#[cfg(feature = "prove")]
pub trait ColumnProofPreparer: Send + Sync {
    /// Human-readable scheme name.
    fn name(&self) -> &str;

    /// Portable scheme identifier.
    fn scheme_id(&self) -> SchemeId;

    /// Prepare the final per-column proof store and metadata.
    fn prepare_column(&self, context: ColumnProofContext) -> ExtResult<PreparedColumnProof>;
}

/// Proof-extension factory for one scheme family.
pub trait ProofSchemeFactory: Send + Sync {
    /// Sealed descriptor for this scheme implementation.
    fn descriptor(&self) -> SchemeDescriptor;

    /// Portable protocol identifier implemented by this factory.
    fn scheme_id(&self) -> SchemeId {
        self.descriptor().scheme_id
    }

    /// Human-readable name.
    fn name(&self) -> &str;

    /// Build the proof-column setup view for one `(table, col)` pair.
    #[cfg(feature = "verify")]
    fn build_proof_column(&self, plan: &ResolvedColumnPlan) -> ExtResult<Arc<dyn ProofColumn>>;

    /// Build the proof preparer for one `(table, col)` pair.
    #[cfg(feature = "prove")]
    fn build_proof_preparer(
        &self,
        plan: &ResolvedColumnPlan,
    ) -> ExtResult<Arc<dyn ColumnProofPreparer>>;
}
