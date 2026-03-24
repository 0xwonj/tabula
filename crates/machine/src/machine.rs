//! High-level machine interface for multi-proof STARK proving and verification.
//!
//! [`TabulaMachine`] orchestrates the C+2 proof architecture:
//! 1 execution proof + C column proofs + 1 root proof.
//!
//! ```ignore
//! let machine = TabulaMachine::new(columns)?;
//! let proof = machine.prove(crate::PreparedMachineInput {
//!     execution,
//!     columns,
//!     root,
//!     statement,
//!     statement_digest: [0u8; 32],
//! })?;
//! machine.verify(&proof)?;
//! ```

use std::fmt;
use std::sync::Arc;

use crate::backend::ProofColumn;
use crate::config::TabulaStarkConfig;
use crate::input::PreparedMachineInput;
use crate::proof::errors::{ProveError, VerificationError};
use crate::proof::model::TabulaProof;
use crate::setup::MachineTopology;
use crate::setup::builder::MachineBuilder;
use crate::setup::registry::SetupError;

/// A configured STARK machine for multi-proof proving and verification.
///
/// Owns per-tier setups (registries, keys, chips) for the C+2 proof
/// architecture. Created from column configuration, then used to build
/// traces, generate proofs, and verify proofs.
pub struct TabulaMachine {
    pub(crate) topology: MachineTopology,
}

impl fmt::Debug for TabulaMachine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TabulaMachine")
            .field(
                "exec_chips",
                &self.topology.proof_topology().execution.registry.chip_ids(),
            )
            .field("num_columns", &self.topology.proof_topology().columns.len())
            .finish_non_exhaustive()
    }
}

impl TabulaMachine {
    /// Create a machine for the given column instances.
    ///
    /// Built-in and custom schemes are resolved before this layer. The machine
    /// receives only per-column scheme instances plus execution/root setup.
    pub fn new(
        columns: impl IntoIterator<Item = Arc<dyn ProofColumn>>,
    ) -> Result<Self, SetupError> {
        MachineBuilder::new().with_columns(columns).build()
    }

    /// Create a machine with a custom STARK configuration.
    pub fn with_config(
        columns: impl IntoIterator<Item = Arc<dyn ProofColumn>>,
        config: TabulaStarkConfig,
    ) -> Result<Self, SetupError> {
        MachineBuilder::new()
            .with_columns(columns)
            .with_config(config)
            .build()
    }

    /// Create a builder for customized machine construction.
    ///
    /// This is a backend-oriented escape hatch used after all domain-specific
    /// runtime planning is complete. Stable host integrations should prefer the
    /// higher-level runtime or SDK surfaces instead of constructing machines
    /// directly.
    pub fn builder() -> MachineBuilder {
        MachineBuilder::new()
    }

    /// Construct from pre-built backend setup.
    #[must_use]
    pub(crate) fn from_topology(topology: MachineTopology) -> Self {
        Self { topology }
    }

    /// Build traces and generate a proof from prepared backend input.
    pub fn prove(&self, input: PreparedMachineInput) -> Result<TabulaProof, ProveError> {
        crate::proof::prover::Prover::new(&self.topology).prove(input)
    }

    /// Verify a proof against this machine's configured backend setup.
    pub fn verify(&self, proof: &TabulaProof) -> Result<(), VerificationError> {
        crate::proof::verifier::Verifier::new(&self.topology).verify(proof)
    }
}
