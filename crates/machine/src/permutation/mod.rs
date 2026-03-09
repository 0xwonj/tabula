//! Permutation trace infrastructure for cross-chip LogUp verification.
//!
//! Provides:
//! - Fiat-Shamir challenge derivation from main trace metadata
//! - Extension-field (EF4) fingerprint computation
//! - Descriptor-based permutation trace generation (PCS-committed cumsums)
//!
//! # Soundness
//!
//! **C1 (fixed)**: Permutation traces (phi values + running cumsums) are generated
//! as additional trace columns and committed via PCS alongside the main trace.
//! The RapAir wrapper constrains `phi·f = m` and cumsum transitions in the AIR.
//!
//! **C2 (fixed)**: Challenges (α, β) are derived from a Fiat-Shamir transcript
//! seeded with main trace metadata (heights, public values).
//!
//! **M5 (fixed)**: Fingerprints are computed in EF4 (~124-bit security).

mod challenges;
mod trace;

pub(crate) use trace::generate_permutation_trace_from_interactions;

#[cfg(test)]
mod tests;
