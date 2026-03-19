use std::sync::Arc;

use tabula_machine::ProofColumn;

#[cfg(feature = "prove")]
use crate::columns::ColumnTransitionBackend;
use crate::columns::RuntimeColumn;

/// Runtime/preparation result for one committed column.
#[derive(Clone)]
pub struct ColumnViews {
    runtime: Arc<dyn RuntimeColumn>,
    proof: Arc<dyn ProofColumn>,
    #[cfg(feature = "prove")]
    transition: Arc<dyn ColumnTransitionBackend>,
}

impl ColumnViews {
    /// Create runtime/proof views built from the same column plan.
    #[cfg(feature = "prove")]
    pub fn new(
        runtime: Arc<dyn RuntimeColumn>,
        proof: Arc<dyn ProofColumn>,
        transition: Arc<dyn ColumnTransitionBackend>,
    ) -> Self {
        Self {
            runtime,
            proof,
            transition,
        }
    }

    /// Create runtime/proof views built from the same column plan.
    #[cfg(not(feature = "prove"))]
    pub fn new(runtime: Arc<dyn RuntimeColumn>, proof: Arc<dyn ProofColumn>) -> Self {
        Self { runtime, proof }
    }

    /// Borrow the execution-facing column view.
    pub fn runtime(&self) -> &Arc<dyn RuntimeColumn> {
        &self.runtime
    }

    /// Borrow the proving-facing column view.
    pub fn proof(&self) -> &Arc<dyn ProofColumn> {
        &self.proof
    }

    /// Borrow the column transition backend view.
    #[cfg(feature = "prove")]
    pub fn transition(&self) -> &Arc<dyn ColumnTransitionBackend> {
        &self.transition
    }

    /// Consume into the column views.
    #[cfg(feature = "prove")]
    pub(crate) fn into_parts(
        self,
    ) -> (
        Arc<dyn RuntimeColumn>,
        Arc<dyn ProofColumn>,
        Arc<dyn ColumnTransitionBackend>,
    ) {
        (self.runtime, self.proof, self.transition)
    }

    /// Consume into the column views.
    #[cfg(not(feature = "prove"))]
    pub fn into_parts(self) -> (Arc<dyn RuntimeColumn>, Arc<dyn ProofColumn>) {
        (self.runtime, self.proof)
    }
}

impl std::fmt::Debug for ColumnViews {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ColumnViews")
            .field("runtime_scheme", &self.runtime.name())
            .field("proof_scheme", &self.proof.name())
            .field("transition_scheme", &{
                #[cfg(feature = "prove")]
                {
                    self.transition.name()
                }
                #[cfg(not(feature = "prove"))]
                {
                    self.proof.name()
                }
            })
            .field("table_id", &self.proof.table_id())
            .field("col_id", &self.proof.col_id())
            .field("scheme_id", &self.proof.scheme_id())
            .field("has_transition_backend", &{
                #[cfg(feature = "prove")]
                {
                    true
                }
                #[cfg(not(feature = "prove"))]
                {
                    false
                }
            })
            .finish()
    }
}
