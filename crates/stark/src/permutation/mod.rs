//! Permutation trace infrastructure for cross-chip LogUp verification.
//!
//! Provides:
//! - Extension-field (EF4) fingerprint computation
//! - Descriptor-based permutation trace generation (PCS-committed cumsums)
//!
//! # Soundness
//!
//! **C1 (fixed)**: Permutation traces (phi values + running cumsums) are generated
//! as additional trace columns and committed via PCS alongside the main trace.
//! The RapAir wrapper constrains `phi·f = m` and cumsum transitions in the AIR.
//!
//! **M5 (fixed)**: Fingerprints are computed in EF4 (~124-bit security).

mod trace;

pub use trace::{
    PermutationTraceOutput, compute_fingerprint_ef4, generate_permutation_trace_from_interactions,
};

#[cfg(test)]
mod challenges;

#[cfg(test)]
mod tests;

/// Errors during permutation trace generation.
#[derive(Debug, thiserror::Error)]
pub enum PermutationError {
    /// LogUp fingerprint evaluated to zero (division by zero).
    ///
    /// Probability ~2^{-124} with random challenges. If this occurs, retry
    /// with different randomness.
    #[error("LogUp fingerprint is zero at row {row}, interaction {interaction}")]
    FingerprintZero {
        /// Trace row where the zero fingerprint occurred.
        row: usize,
        /// Interaction index within that row.
        interaction: usize,
    },
}
