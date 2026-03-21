//! High-level machine interface for multi-proof STARK proving and verification.
//!
//! [`TabulaMachine`] orchestrates the C+2 proof architecture:
//! 1 execution proof + C column proofs + 1 root proof.
//!
//! ```ignore
//! let machine = TabulaMachine::new(columns)?;
//! let prover = machine.prover();
//! let verifier = machine.verifier();
//! let traces = prepared_traces();
//! let proof = prover.prove(crate::MachineProofInput {
//!     traces,
//!     statement,
//!     statement_digest: [0u8; 32],
//! })?;
//! verifier.verify(&proof)?;
//! ```

use std::fmt;

use crate::columns::ProofColumn;
use crate::config::TabulaStarkConfig;
use crate::setup::MachineSetup;
use crate::setup::builder::MachineBuilder;
use crate::setup::registry::SetupError;
use std::sync::Arc;

/// A configured STARK machine for multi-proof proving and verification.
///
/// Owns per-tier setups (registries, keys, chips) for the C+2 proof
/// architecture. Created from column configuration, then used to build
/// traces, generate proofs, and verify proofs.
pub struct TabulaMachine {
    setup: MachineSetup,
}

impl fmt::Debug for TabulaMachine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TabulaMachine")
            .field(
                "exec_chips",
                &self.setup.proof_setups().execution.registry.chip_ids(),
            )
            .field("num_columns", &self.setup.proof_setups().columns.len())
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
    pub fn from_setup(setup: MachineSetup) -> Self {
        Self { setup }
    }

    /// The immutable backend setup owned by this machine.
    pub fn setup(&self) -> &MachineSetup {
        &self.setup
    }
}
