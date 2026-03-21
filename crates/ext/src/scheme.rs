use std::collections::BTreeSet;
use std::sync::Arc;

pub use tabula_artifact::SchemeDescriptor;
pub use tabula_core::{
    ColId, ColumnLayoutKind, RootProfileId, RowKey, SchemeId, TableId, Value, ValueType,
};
use tabula_core::{PropertyQueryResult, error::TabulaError};
pub use tabula_ir::{PropertyQuery, PropertyQueryKind};

use crate::backend::scheme::ProofSchemeFactory;
use crate::error::{ExtError, ExtResult};

/// Compiler/runtime-owned plan for one committed column.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedColumnPlan {
    /// Table identifier.
    pub table_id: TableId,
    /// Column identifier.
    pub col_id: ColId,
    /// Portable scheme identifier selected by the compiler.
    pub scheme_id: SchemeId,
    /// Sealed scheme descriptor selected by the compiler.
    pub scheme_descriptor: SchemeDescriptor,
    /// Column value type from the sealed schema surface.
    pub value_type: ValueType,
    /// Whether this column participates in the root commitment.
    pub receives_commitment: bool,
    /// Exact structural property kinds required for this column by the program.
    pub required_property_query_kinds: BTreeSet<PropertyQueryKind>,
}

impl ResolvedColumnPlan {
    /// Whether this column needs any scheme-backed property support.
    pub fn requires_property_support(&self) -> bool {
        !self.required_property_query_kinds.is_empty()
    }
}

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
        state: &[(RowKey, Value, bool)],
    ) -> Result<PropertyQueryResult, TabulaError> {
        let _ = state;
        Err(TabulaError::InvalidIr(format!(
            "column scheme '{}' does not support property query {:?}",
            self.name(),
            query.kind(),
        )))
    }
}

/// Registry-facing factory for a column commitment scheme family.
pub trait ColumnSchemeFactory: Send + Sync {
    /// Sealed descriptor for this scheme implementation.
    fn descriptor(&self) -> SchemeDescriptor;

    /// Portable protocol identifier implemented by this factory.
    fn scheme_id(&self) -> SchemeId {
        self.descriptor().scheme_id
    }

    /// Human-readable name.
    fn name(&self) -> &str;

    /// Build the execution-facing runtime view for one `(table, col)` pair.
    fn build_runtime_column(&self, plan: &ResolvedColumnPlan) -> ExtResult<Arc<dyn RuntimeColumn>>;
}

/// Canonical bundle for one custom column-scheme family.
#[derive(Clone)]
pub struct SchemeBundle {
    descriptor: SchemeDescriptor,
    runtime_factory: Arc<dyn ColumnSchemeFactory>,
    proof_factory: Arc<dyn ProofSchemeFactory>,
}

impl SchemeBundle {
    /// Build a bundle from matching runtime and proof factories.
    pub fn new(
        runtime_factory: impl ColumnSchemeFactory + 'static,
        proof_factory: impl ProofSchemeFactory + 'static,
    ) -> ExtResult<Self> {
        let runtime_factory: Arc<dyn ColumnSchemeFactory> = Arc::new(runtime_factory);
        let proof_factory: Arc<dyn ProofSchemeFactory> = Arc::new(proof_factory);
        let runtime_descriptor = runtime_factory.descriptor();
        let proof_descriptor = proof_factory.descriptor();

        if runtime_descriptor.scheme_id != proof_descriptor.scheme_id {
            return Err(ExtError::validation(format!(
                "scheme bundle requires identical scheme ids, got runtime={} proof={}",
                runtime_descriptor.scheme_id.0, proof_descriptor.scheme_id.0
            )));
        }
        if runtime_descriptor != proof_descriptor {
            return Err(ExtError::validation(format!(
                "scheme bundle requires identical descriptors for id {}",
                runtime_descriptor.scheme_id.0
            )));
        }

        Ok(Self {
            descriptor: runtime_descriptor,
            runtime_factory,
            proof_factory,
        })
    }

    /// The compiler-visible scheme descriptor carried by this bundle.
    pub fn descriptor(&self) -> &SchemeDescriptor {
        &self.descriptor
    }

    /// Portable scheme identifier carried by this bundle.
    pub fn scheme_id(&self) -> SchemeId {
        self.descriptor.scheme_id
    }

    /// Clone the runtime-facing factory.
    pub fn runtime_factory(&self) -> Arc<dyn ColumnSchemeFactory> {
        Arc::clone(&self.runtime_factory)
    }

    /// Consume the bundle and return the runtime-facing factory.
    pub fn into_runtime_factory(self) -> Arc<dyn ColumnSchemeFactory> {
        self.runtime_factory
    }

    /// Clone the proof-facing factory.
    pub fn proof_factory(&self) -> Arc<dyn ProofSchemeFactory> {
        Arc::clone(&self.proof_factory)
    }

    /// Consume the bundle and return the proof-facing factory.
    pub fn into_proof_factory(self) -> Arc<dyn ProofSchemeFactory> {
        self.proof_factory
    }

    /// Consume the bundle and return all owned parts.
    pub fn into_parts(
        self,
    ) -> (
        SchemeDescriptor,
        Arc<dyn ColumnSchemeFactory>,
        Arc<dyn ProofSchemeFactory>,
    ) {
        (self.descriptor, self.runtime_factory, self.proof_factory)
    }
}
