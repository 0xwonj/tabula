use std::sync::Arc;

use tabula_machine::ProofColumn;

use crate::columns::RuntimeColumn;
#[cfg(feature = "prove")]
use crate::columns::ProofInputBuilder;

/// Runtime/preparation result for one committed column.
#[derive(Clone)]
pub struct ColumnViews {
    runtime: Arc<dyn RuntimeColumn>,
    proof: Arc<dyn ProofColumn>,
    #[cfg(feature = "prove")]
    proof_input: Arc<dyn ProofInputBuilder>,
}

impl ColumnViews {
    /// Create runtime/proof views built from the same column plan.
    #[cfg(feature = "prove")]
    pub fn new(
        runtime: Arc<dyn RuntimeColumn>,
        proof: Arc<dyn ProofColumn>,
        proof_input: Arc<dyn ProofInputBuilder>,
    ) -> Self {
        Self {
            runtime,
            proof,
            proof_input,
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

    /// Borrow the proof-input builder view.
    #[cfg(feature = "prove")]
    pub fn proof_input(&self) -> &Arc<dyn ProofInputBuilder> {
        &self.proof_input
    }

    /// Consume into the column views.
    #[cfg(feature = "prove")]
    pub fn into_parts(
        self,
    ) -> (
        Arc<dyn RuntimeColumn>,
        Arc<dyn ProofColumn>,
        Arc<dyn ProofInputBuilder>,
    ) {
        (self.runtime, self.proof, self.proof_input)
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
            .field(
                "proof_input_scheme",
                &{
                    #[cfg(feature = "prove")]
                    {
                        self.proof_input.name()
                    }
                    #[cfg(not(feature = "prove"))]
                    {
                        self.proof.name()
                    }
                },
            )
            .field("table_id", &self.proof.table_id())
            .field("col_id", &self.proof.col_id())
            .field("scheme_id", &self.proof.scheme_id())
            .finish()
    }
}
