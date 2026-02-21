//! Proof types for the multi-chip Tabula STARK.
//!
//! A [`TabulaProof`] wraps independent per-chip STARK proofs (produced by
//! `p3-uni-stark`) with cross-chip LogUp balance data.

use p3_baby_bear::BabyBear;
use p3_uni_stark::{PreprocessedVerifierKey, Proof};

use super::config::{EF4, TabulaStarkConfig};

/// A complete Tabula batch proof.
///
/// Contains one STARK proof per chip plus cross-chip LogUp metadata.
/// Verification checks:
/// 1. Each per-chip STARK proof is valid (main constraints hold).
/// 2. The LogUp cumulative sums across all chips sum to zero in EF4.
pub struct TabulaProof {
    /// Per-chip STARK proofs, indexed by [`ChipIndex`].
    pub chip_proofs: Vec<ChipProofEntry>,
    /// Sum of all chips' final cumulative sums (should be zero for a valid proof).
    /// Stored for diagnostic purposes; the verifier recomputes from chip data.
    pub cumsum_total: [BabyBear; 4],
}

/// A per-chip STARK proof with metadata.
pub struct ChipProofEntry {
    /// Human-readable chip name (for diagnostics).
    pub chip_name: &'static str,
    /// The p3-uni-stark proof for this chip's main constraints.
    pub proof: Proof<TabulaStarkConfig>,
    /// The chip's final LogUp cumulative sum (4 BabyBear elements = 1 EF4 element).
    /// Cross-chip check: Σ_chips cumsum_final = 0.
    pub cumsum_final: EF4,
    /// Trace height (number of rows).
    pub trace_height: usize,
    /// Public values used for this chip (may be empty).
    pub public_values: Vec<BabyBear>,
    /// Preprocessed verifier key for chips with preprocessed columns (e.g. Poseidon).
    /// `None` for chips without preprocessed data.
    pub preprocessed_vk: Option<PreprocessedVerifierKey<TabulaStarkConfig>>,
}

/// Errors during proof verification.
#[derive(Debug)]
pub enum VerificationError {
    /// A per-chip STARK proof failed verification.
    ChipVerificationFailed {
        /// Which chip failed.
        chip_name: &'static str,
        /// The underlying p3-uni-stark verification error message.
        detail: String,
    },
    /// The cross-chip LogUp cumulative sums do not sum to zero.
    LogUpImbalance {
        /// The nonzero total cumsum.
        total: [BabyBear; 4],
    },
    /// The proof's chip manifest is invalid (missing, extra, or duplicate chips).
    InvalidChipManifest {
        /// Description of the manifest error.
        detail: String,
    },
}

impl std::fmt::Display for VerificationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ChipVerificationFailed { chip_name, detail } => {
                write!(f, "chip '{chip_name}' verification failed: {detail}")
            }
            Self::LogUpImbalance { total } => {
                write!(
                    f,
                    "LogUp imbalance: cumsum total = {total:?} (expected zero)"
                )
            }
            Self::InvalidChipManifest { detail } => {
                write!(f, "invalid chip manifest: {detail}")
            }
        }
    }
}

impl std::error::Error for VerificationError {}
