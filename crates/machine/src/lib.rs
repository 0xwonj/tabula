#![warn(missing_docs)]
#![deny(unused)]

//! STARK prover and verifier for Tabula batch proofs.
//!
//! Uses Plonky3 primitives for per-chip STARK proofs with cross-chip
//! LogUp balance verification.
//!
//! ```ignore
//! let machine = TabulaMachine::builder().with_core_chips().build()?;
//! let proof = machine.prove(&traces, statement)?;
//! machine.verify(&proof)?;
//! ```
//!
//! # Soundness status
//!
//! **Per-chip main constraints**: Sound — each chip is proven independently via
//! `p3-uni-stark`, which provides full FRI-based soundness for the AIR constraints.
//!
//! **Cross-chip LogUp**: Sound.
//!
//! - **C2 (fixed)**: LogUp challenges (alpha, beta) are derived from a Fiat-Shamir
//!   transcript seeded with chip proof metadata (trace heights, public values).
//! - **M5 (fixed)**: Fingerprints are computed in the extension field (EF4,
//!   ~124-bit security).
//! - **C1 (fixed)**: Permutation trace columns (phi, cumsum) are concatenated to
//!   the main trace and PCS-committed together. RAP constraints (phi·f = m,
//!   cumsum transitions) are evaluated inline via a two-phase prover/verifier.
//!   A forged cumsum would fail FRI verification.

mod any_rap;
mod chip_ref;
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
pub use chip_ref::ChipRef;
pub use config::{EF4, TabulaStarkConfig, default_config};
pub use machine::{MachineBuilder, TabulaMachine};
pub use keys::{ChipVerifyInfo, TabulaProvingKey, TabulaVerifyingKey};
pub use proof::{ChipProofEntry, ProveError, TabulaProof, VerificationError};
pub use prove::prove_with_key;
pub use registry::{ChipRegistry, RegisteredChip, SetupError, core_chips};
pub use tabula_stark::air::statement::PublicStatement;
pub use verify::verify_with_key;
