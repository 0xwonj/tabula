//! STARK prover and verifier for Tabula batch proofs.
//!
//! Uses Plonky3 primitives for per-chip STARK proofs with cross-chip
//! LogUp balance verification.
//!
//! # Soundness status (MVP / PoC)
//!
//! **Per-chip main constraints**: Sound — each chip is proven independently via
//! `p3-uni-stark`, which provides full FRI-based soundness for the AIR constraints.
//!
//! **Cross-chip LogUp**: NOT cryptographically sound yet. The current implementation
//! records interactions from concrete trace data (debug mode) and checks balance,
//! but the cumulative sums are **not committed via the PCS**. A malicious prover
//! could forge the `cumsum_final` values. Known gaps:
//!
//! - **C1**: `cumsum_final` is a bare field element in the proof, not bound to any
//!   Merkle commitment or FRI opening. Needs permutation trace columns committed
//!   and constrained by the AIR.
//! - **C2**: LogUp challenges (α, β) are deterministic constants, not derived from
//!   the Fiat-Shamir transcript. Needs challenger sampling after main trace commit.
//! - **M5**: Fingerprints are computed in the base field (BabyBear, ~31-bit) rather
//!   than the extension field (EF4, ~124-bit).
//!
//! These will be addressed when the full two-round IOP (commit main → sample
//! challenges → commit permutation trace → FRI) is implemented.

mod bridge;
mod config;
mod proof;
mod prover;
mod verifier;

pub use config::{EF4, TabulaStarkConfig, default_config};
pub use proof::{ChipProofEntry, TabulaProof, VerificationError};
pub use prover::{prove, prove_with_config};
pub use verifier::{verify, verify_with_config};
