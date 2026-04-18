//! Multi-proof STARK prover and verifier for Tabula batch proofs.
//!
//! Orchestrates a C+2 proof architecture (1 execution + C column + 1 root)
//! with shared Fiat-Shamir synchronization for LogUp challenges.
//! This is an advanced/backend API; most users should enter through `tabula-runtime`.
//!
//! The canonical backend primitive surface is
//! [`backend::BackendProver::prove_envelope`] /
//! [`backend::BackendVerifier::verify_envelope`], which produce or consume a
//! [`tabula_contract::ProofEnvelope`] around a decoded [`TabulaProof`]. The
//! caller is responsible for threading the batch's `PublicStatement` beside
//! the proof; the machine binds only the 32-byte `binding_digest` into its
//! Fiat-Shamir transcript.
//!
//! ```ignore
//! let machine = TabulaMachine::new(&columns)?;
//! let binding_digest = [0u8; 32];
//! let (proof, envelope) = tabula_machine::backend::BackendProver::new(&machine)
//!     .prove_envelope(tabula_machine::PreparedMachineInput {
//!         execution,
//!         columns,
//!         root,
//!         binding_digest,
//!     })?;
//! tabula_machine::backend::BackendVerifier::new(&machine)
//!     .verify_envelope(&envelope, binding_digest)?;
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

pub use backend::{BackendProver, BackendVerifier};
pub use config::{EF4, TabulaStarkConfig, default_config, make_config};
pub use input::{ColumnSlotKey, PreparedColumnInput, PreparedMachineInput, PreparedTierInput};
pub use machine::TabulaMachine;
pub use proof::codec::decode_proof_bytes;
pub use proof::errors::ProofCodecError;
pub use proof::errors::{ProveError, VerificationError};
pub use proof::model::{ChipOpening, ColumnProofEntry, ProofTier, SubProofEnvelope, TabulaProof};
pub use setup::builder::MachineBuilder;
pub use setup::registry::SetupError;
pub use setup::root::{RootProofBackend, SmtRootProofBackend};
