//! Multi-proof STARK prover and verifier for Tabula batch proofs.
//!
//! Orchestrates a C+2 proof architecture (1 execution + C column + 1 root)
//! with shared Fiat-Shamir synchronization for LogUp challenges.
//! This is an advanced/backend API; most users should enter through `tabula-runtime`.
//!
//! ```ignore
//! let machine = TabulaMachine::new(&columns)?;
//! let proof = machine.prove(tabula_machine::PreparedMachineInput {
//!     execution,
//!     columns,
//!     root,
//!     air_statement,
//!     semantic_statement_digest: [0u8; 32],
//! })?;
//! machine.verify(&proof)?;
//! ```

/// Advanced backend-only APIs for execution-tier machine composition.
pub mod backend;
pub mod config;
pub mod input;
mod machine;
mod proof;
mod setup;
#[cfg(test)]
mod testing;

pub use config::{EF4, TabulaStarkConfig, default_config, make_config};
pub use input::{ColumnSlotKey, PreparedColumnInput, PreparedMachineInput, PreparedTierInput};
pub use machine::TabulaMachine;
pub use proof::errors::{ProveError, VerificationError};
pub use proof::model::{ChipOpening, ColumnProofEntry, ProofTier, SubProofEnvelope, TabulaProof};
pub use setup::builder::MachineBuilder;
pub use setup::registry::SetupError;
pub use setup::root::{RootProofBackend, SmtRootProofBackend};
pub use tabula_stark::air::statement::PublicStatement;
