//! Root backend authoring and configuration surface.

#[cfg(feature = "prove")]
use std::sync::Arc;

#[cfg(feature = "prove")]
use tabula_commitment::{
    ColumnRootBinding, NativeDigest, PoseidonHasher, compute_state_roots_from_bindings,
};
#[cfg(feature = "prove")]
use tabula_core::RootProfileId;
#[cfg(feature = "prove")]
use tabula_machine::PublicStatement;
#[cfg(feature = "verify")]
pub use tabula_machine::{RootProofBackend, SmtRootProofBackend};
#[cfg(feature = "prove")]
use tabula_stark::trace::WitnessStore;
#[cfg(feature = "prove")]
use tabula_witness::stark::{SmtRootStoreContext, prepare_smt_root_store};

#[cfg(feature = "prove")]
use crate::{ExtError, ExtResult};

/// Canonical batch root input prepared by the runtime for one root witness path.
#[cfg(feature = "prove")]
#[non_exhaustive]
#[derive(Clone, Copy)]
pub struct RootWitnessContext<'a> {
    column_root_bindings: &'a [ColumnRootBinding],
}

#[cfg(feature = "prove")]
impl<'a> RootWitnessContext<'a> {
    /// Build a canonical root witness context from prepared column root bindings.
    pub fn new(column_root_bindings: &'a [ColumnRootBinding]) -> Self {
        Self {
            column_root_bindings,
        }
    }

    /// Canonical column root bindings prepared for this batch.
    pub fn column_root_bindings(&self) -> &'a [ColumnRootBinding] {
        self.column_root_bindings
    }
}

/// Prepared root-tier witness bundle returned by one root witness preparer.
#[cfg(feature = "prove")]
pub struct PreparedRootWitness {
    public_statement: PublicStatement,
    store: WitnessStore,
}

#[cfg(feature = "prove")]
impl PreparedRootWitness {
    /// Build a prepared root witness bundle from one AIR statement and root-tier store.
    pub fn new(public_statement: PublicStatement, store: WitnessStore) -> Self {
        Self {
            public_statement,
            store,
        }
    }

    /// Borrow the proved public statement for this prepared witness.
    pub fn public_statement(&self) -> &PublicStatement {
        &self.public_statement
    }

    /// Borrow the root-tier witness store for this prepared witness.
    pub fn store(&self) -> &WitnessStore {
        &self.store
    }

    /// Consume the prepared witness and return its constituent parts.
    pub fn into_parts(self) -> (PublicStatement, WitnessStore) {
        (self.public_statement, self.store)
    }
}

/// Runtime-facing root witness materializer for one batch proof request.
#[cfg(feature = "prove")]
pub trait RootWitnessPreparer: Send + Sync {
    /// Human-readable preparer name.
    fn name(&self) -> &str;

    /// Prepare the root statement and root-tier witness store for one batch.
    fn prepare_root_witness(
        &self,
        context: RootWitnessContext<'_>,
    ) -> ExtResult<PreparedRootWitness>;
}

/// Canonical root backend family for one selected proving root path.
#[cfg(feature = "prove")]
pub trait RootBackend: Send + Sync {
    /// Human-readable root backend family name.
    fn name(&self) -> &str;

    /// Clone the proof-side root backend for this family.
    fn proof_backend(&self) -> Arc<dyn RootProofBackend>;

    /// Clone the runtime-facing root witness preparer for this family.
    fn witness_preparer(&self) -> Arc<dyn RootWitnessPreparer>;
}

/// Canonical bundle pairing proof-side root behavior with root witness preparation.
#[cfg(feature = "prove")]
#[derive(Clone)]
pub struct RootBackendBundle {
    backend: Arc<dyn RootBackend>,
}

#[cfg(feature = "prove")]
impl RootBackendBundle {
    /// Build a canonical bundle from one selected root backend family.
    pub fn new(backend: impl RootBackend + 'static) -> Self {
        Self {
            backend: Arc::new(backend),
        }
    }

    /// Standard built-in SMT root backend bundle.
    pub fn standard() -> Self {
        Self::new(SmtRootBackend)
    }

    /// Human-readable root backend family name.
    pub fn name(&self) -> &str {
        self.backend.name()
    }

    /// Clone the bundled proof-side root backend.
    pub fn proof_backend(&self) -> Arc<dyn RootProofBackend> {
        self.backend.proof_backend()
    }

    /// Root binding families accepted by the bundled proof-side root backend.
    pub fn supported_root_binding_families(&self) -> &'static [RootProfileId] {
        self.backend
            .proof_backend()
            .supported_root_binding_families()
    }

    /// Clone the bundled root witness preparer.
    pub fn witness_preparer(&self) -> Arc<dyn RootWitnessPreparer> {
        self.backend.witness_preparer()
    }
}

#[cfg(feature = "prove")]
impl Default for RootBackendBundle {
    fn default() -> Self {
        Self::standard()
    }
}

/// Built-in SMT root backend family.
#[cfg(feature = "prove")]
#[derive(Clone, Copy, Debug, Default)]
pub struct SmtRootBackend;

#[cfg(feature = "prove")]
impl RootBackend for SmtRootBackend {
    fn name(&self) -> &str {
        "smt_root"
    }

    fn proof_backend(&self) -> Arc<dyn RootProofBackend> {
        Arc::new(SmtRootProofBackend)
    }

    fn witness_preparer(&self) -> Arc<dyn RootWitnessPreparer> {
        Arc::new(SmtRootWitnessPreparer)
    }
}

/// Built-in SMT root witness preparer paired with [`SmtRootProofBackend`].
///
/// This remains intentionally SMT-shaped today: the generic runtime-facing root
/// input is lowered into the SMT-specific root-tier witness store here.
#[cfg(feature = "prove")]
#[derive(Clone, Copy, Debug, Default)]
pub struct SmtRootWitnessPreparer;

#[cfg(feature = "prove")]
impl RootWitnessPreparer for SmtRootWitnessPreparer {
    fn name(&self) -> &str {
        "smt_root"
    }

    fn prepare_root_witness(
        &self,
        context: RootWitnessContext<'_>,
    ) -> ExtResult<PreparedRootWitness> {
        let hasher = PoseidonHasher::new();
        let (old_state_root, new_state_root) =
            compute_state_roots_from_bindings(&hasher, context.column_root_bindings())
                .map_err(ExtError::proof_preparation)?;
        let public_statement = PublicStatement {
            old_root: old_state_root,
            new_root: new_state_root,
            public_context_digest: NativeDigest::ZERO,
            applied_tx_digest: NativeDigest::ZERO,
            event_digest: NativeDigest::ZERO,
        };
        let store = prepare_smt_root_store(
            SmtRootStoreContext::new(
                context.column_root_bindings(),
                &public_statement.old_root,
                &public_statement.new_root,
            ),
            hasher,
        )
        .map_err(ExtError::proof_preparation)?;

        Ok(PreparedRootWitness::new(public_statement, store))
    }
}
