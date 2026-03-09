#![warn(missing_docs)]
#![deny(unused)]

//! STARK prover and verifier for Tabula batch proofs.
//!
//! Uses Plonky3 primitives for per-chip STARK proofs with cross-chip
//! LogUp balance verification.
//!
//! ```ignore
//! let machine = TabulaMachine::builder()
//!     .with_core_chips()
//!     .with_default_commitments()
//!     .build()?;
//! let proof = machine.prove(&traces, statement)?;
//! machine.verify(&proof)?;
//! ```
//!
//! # Soundness status
//!
//! **Batched PCS**: All chip traces are committed in shared PCS rounds and
//! opened via a single FRI proof, providing full soundness across all chips.
//!
//! **Cross-chip LogUp**: Sound.
//!
//! - **C2 (fixed)**: LogUp challenges (alpha, beta) are derived from a
//!   Fiat-Shamir transcript seeded with the PCS main trace commitment.
//! - **M5 (fixed)**: Fingerprints are computed in the extension field (EF4,
//!   ~124-bit security).
//! - **C1 (fixed)**: Permutation traces are PCS-committed in a separate round.
//!   RAP constraints (phi·f = m, cumsum transitions) are evaluated inline via
//!   a two-phase prover/verifier. A forged cumsum would fail FRI verification.

mod any_rap;
pub(crate) mod chip_ref;
pub mod composition;
pub mod config;
pub(crate) mod ef4;
pub mod keys;
mod machine;
pub(crate) mod permutation;
mod proof;
mod prove;
mod registry;
mod verify;

pub use any_rap::AnyRap;
pub use config::{EF4, TabulaStarkConfig, default_config};
pub use machine::{MachineBuilder, TabulaMachine};
pub use keys::{ChipVerifyInfo, TabulaProvingKey, TabulaVerifyingKey};
pub use proof::{ChipOpening, ProveError, TabulaProof, VerificationError};
pub use composition::{CommitmentScheme, SsmcScheme, SmtScheme};
pub use registry::{ChipRegistry, RegisteredChip, SetupError, core_chips, default_commitment_chips};
pub use tabula_stark::air::statement::PublicStatement;
