#[cfg(feature = "verify")]
use std::collections::BTreeSet;
#[cfg(feature = "verify")]
use std::sync::Arc;
use tabula_core::error::TabulaError;
pub use tabula_core::{ColId, ColumnLayoutKind, Digest, RootProfileId, SchemeId, TableId};
pub use tabula_ir::{PropertyQuery, PropertyQueryKind};
#[cfg(feature = "verify")]
use tabula_profile::{ResolvedColumnProfileRef, VerifierDigestFormat};
#[cfg(feature = "verify")]
use tabula_types::{EncodingRuntime, TypeRuntime};
use tabula_types::{TypedColumnEntry, TypedPropertyQueryResult};

#[cfg(feature = "verify")]
use crate::backend::ProofColumn;
#[cfg(feature = "prove")]
use crate::backend::scheme::ColumnProofBackend;
#[cfg(feature = "verify")]
use crate::error::ExtResult;

/// Execution-facing per-column view.
pub trait RuntimeColumn: Send + Sync {
    /// Human-readable scheme name.
    fn name(&self) -> &str;

    /// Structural property queries this column supports.
    fn supported_property_query_kinds(&self) -> &[PropertyQueryKind] {
        &[]
    }

    /// Resolve a structural property query over one committed column snapshot.
    fn resolve_property(
        &self,
        query: &PropertyQuery,
        state: &[TypedColumnEntry],
    ) -> Result<TypedPropertyQueryResult, TabulaError> {
        let _ = state;
        Err(TabulaError::InvalidIr(format!(
            "column scheme '{}' does not support property query {:?}",
            self.name(),
            query.kind(),
        )))
    }
}

/// Setup-time canonical input for one materialized column backend.
#[cfg(feature = "verify")]
#[derive(Clone)]
pub struct ColumnBackendSetup<'a> {
    /// Table identifier for the concrete column slot.
    pub table_id: TableId,
    /// Column identifier for the concrete column slot.
    pub col_id: ColId,
    /// Canonical resolved per-column profile.
    pub profile: ResolvedColumnProfileRef<'a>,
    /// Resolved runtime type behavior for this column.
    pub type_runtime: Arc<dyn TypeRuntime>,
    /// Resolved runtime encoding behavior for this column.
    pub encoding_runtime: Arc<dyn EncodingRuntime>,
    /// Exact structural property kinds required by the program for this slot.
    pub required_property_query_kinds: &'a BTreeSet<PropertyQueryKind>,
}

/// Verifier-visible contract exported by one materialized column backend.
#[cfg(feature = "verify")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnVerifierContract {
    /// Portable scheme family id implemented by this backend.
    pub scheme_id: SchemeId,
    /// Proof layout family expected by verifier and machine setup.
    pub proof_layout_family: ColumnLayoutKind,
    /// Canonical verifier-visible digest format.
    pub verifier_digest_format: VerifierDigestFormat,
}

/// Root-binding contract exported by one materialized column backend.
#[cfg(feature = "verify")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootBindingContract {
    /// Root binding family this backend serializes against.
    pub root_binding_family: RootProfileId,
    /// Sealed column profile hash bound into the leaf prefix.
    pub column_profile_hash: Digest,
    /// Precomputed binding prefix digest for this `(table, col, profile)` slot.
    pub binding_digest: tabula_commitment::NativeDigest,
    /// Whether this column participates in the root commitment.
    pub receives_commitment: bool,
}

/// Canonical materialized backend for one concrete column slot.
#[cfg(feature = "verify")]
#[derive(Clone)]
pub struct MaterializedColumnBackend {
    /// Table identifier for this slot.
    pub table_id: TableId,
    /// Column identifier for this slot.
    pub col_id: ColId,
    /// Exact structural property kinds required for this slot.
    pub required_property_query_kinds: BTreeSet<PropertyQueryKind>,
    /// Execution-facing runtime view.
    pub runtime_column: Arc<dyn RuntimeColumn>,
    /// Verifier/machine-facing proof column.
    pub proof_column: Arc<dyn ProofColumn>,
    /// Prover-facing proof backend.
    #[cfg(feature = "prove")]
    pub proof_backend: Arc<dyn ColumnProofBackend>,
    /// Sealed verifier-visible contract.
    pub verifier_contract: ColumnVerifierContract,
    /// Sealed root-binding contract.
    pub root_binding_contract: RootBindingContract,
}

/// Canonical setup-time materializer for one scheme family.
#[cfg(feature = "verify")]
pub trait ColumnBackendFactory: Send + Sync {
    /// Portable scheme family identifier.
    fn scheme_id(&self) -> SchemeId;

    /// Human-readable scheme name.
    fn name(&self) -> &str;

    /// Materialize one concrete column backend from the resolved profile contract.
    fn materialize_backend(
        &self,
        setup: ColumnBackendSetup<'_>,
    ) -> ExtResult<MaterializedColumnBackend>;
}

/// Canonical registration bundle for one column-backend family.
#[cfg(feature = "verify")]
#[derive(Clone)]
pub struct ColumnBackendFactoryBundle {
    factory: Arc<dyn ColumnBackendFactory>,
}

#[cfg(feature = "verify")]
impl ColumnBackendFactoryBundle {
    /// Build a canonical backend bundle from one materializer.
    pub fn new(factory: impl ColumnBackendFactory + 'static) -> Self {
        Self {
            factory: Arc::new(factory),
        }
    }

    /// Portable scheme family identifier carried by this bundle.
    pub fn scheme_id(&self) -> SchemeId {
        self.factory.scheme_id()
    }

    /// Clone the materializer.
    pub fn factory(&self) -> Arc<dyn ColumnBackendFactory> {
        Arc::clone(&self.factory)
    }

    /// Consume the bundle and return the materializer.
    pub fn into_factory(self) -> Arc<dyn ColumnBackendFactory> {
        self.factory
    }
}
