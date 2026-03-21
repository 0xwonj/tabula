//! Multi-proof STARK prover and verifier for Tabula batch proofs.
//!
//! Orchestrates a C+2 proof architecture (1 execution + C column + 1 root)
//! with shared Fiat-Shamir synchronization for LogUp challenges.
//! This is an advanced/backend API; most users should enter through `tabula-runtime`.
//!
//! ```ignore
//! let machine = TabulaMachine::new(&columns)?;
//! let prover = machine.prover();
//! let verifier = machine.verifier();
//! let traces = prepared_traces();
//! let proof = prover.prove(tabula_machine::MachineProofInput {
//!     traces,
//!     statement,
//!     statement_digest: [0u8; 32],
//! })?;
//! verifier.verify(&proof)?;
//! ```

/// Advanced backend-only APIs for execution-tier machine composition.
pub mod backend;
mod columns;
pub mod config;
mod machine;
mod proof;
mod setup;
#[cfg(test)]
mod testing;

pub use config::{EF4, TabulaStarkConfig, default_config, make_config};
pub use machine::TabulaMachine;
pub use proof::types::{
    ChipOpening, ColumnIdentity, ColumnProofEntry, ColumnProofTrace, MachineProofInput, ProofTier,
    ProveError, SubProofEnvelope, TabulaProof, VerificationError,
};
pub use proof::{Prover, Verifier};
pub use setup::builder::MachineBuilder;
pub use setup::keys::{TabulaProvingKey, TabulaVerifyingKey, compute_external_buses};
pub use setup::registry::{ChipRegistry, RegisteredChip, SetupError};
pub use setup::root::{RootProof, SmtRootProof};
pub use setup::{MachineSetup, ProofSetups, ProofTraces, TierSetup};
pub use tabula_stark::air::statement::PublicStatement;
