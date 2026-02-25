//! STARK prover and verifier for Tabula batch proofs.
//!
//! Uses Plonky3 primitives for per-chip STARK proofs with cross-chip
//! LogUp balance verification.
//!
//! Generic over `CS: ChipSet` — callers specify their chip set via the type parameter:
//! ```ignore
//! use tabula_proof::chips::TabulaAir;
//! let proof = stark::prove::<TabulaAir>(&config, &traces);
//! stark::verify::<TabulaAir>(&config, &proof)?;
//! ```
//!
//! # Soundness status
//!
//! **Per-chip main constraints**: Sound — each chip is proven independently via
//! `p3-uni-stark`, which provides full FRI-based soundness for the AIR constraints.
//!
//! **Cross-chip LogUp**: Partially sound.
//!
//! - **C2 (fixed)**: LogUp challenges (α, β) are derived from a Fiat-Shamir
//!   transcript seeded with chip proof metadata (trace heights, public values).
//! - **M5 (fixed)**: Fingerprints are computed in the extension field (EF4,
//!   ~124-bit security).
//! - **C1 (open)**: `cumsum_final` is a bare field element in the proof, not
//!   bound to any Merkle commitment or FRI opening. Needs permutation trace
//!   columns committed and constrained by the AIR. Requires a custom two-round
//!   prover (bypassing p3-uni-stark) to commit permutation traces.

mod bridge;
mod config;
pub(crate) mod permutation;
mod proof;
mod prover;
mod verifier;

pub use config::{EF4, TabulaStarkConfig, default_config};
pub use proof::{ChipProofEntry, TabulaProof, VerificationError};
pub use prover::{StarkAir, prove, prove_default};
pub use verifier::{verify, verify_with_config};
