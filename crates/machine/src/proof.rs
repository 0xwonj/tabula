//! Proof types for the multi-chip Tabula STARK.
//!
//! A [`TabulaProof`] wraps independent per-chip STARK proofs (produced by
//! `p3-uni-stark`) with cross-chip LogUp balance data.

use p3_baby_bear::BabyBear;
use p3_uni_stark::{PreprocessedVerifierKey, Proof};

use tabula_stark::air::statement::PublicStatement;
use tabula_stark::chips::ChipId;

use super::config::{EF4, TabulaStarkConfig};

/// A complete Tabula batch proof.
///
/// Contains one STARK proof per chip plus cross-chip LogUp metadata.
/// Verification checks:
/// 1. Each per-chip STARK proof is valid (main constraints hold).
/// 2. The LogUp challenges match the Fiat-Shamir transcript.
/// 3. The LogUp cumulative sums across all chips sum to zero in EF4.
pub struct TabulaProof {
    /// Per-chip STARK proofs.
    pub chip_proofs: Vec<ChipProofEntry>,
    /// Fiat-Shamir-derived LogUp challenges [α, β] in EF4.
    ///
    /// Bound to the proof instance via a Poseidon2 duplex challenger that
    /// observes chip trace heights and public values.
    pub logup_challenges: [EF4; 2],
    /// The public statement this proof attests to (state root transition).
    pub statement: PublicStatement,
}

/// A per-chip STARK proof with metadata.
pub struct ChipProofEntry {
    /// Type-safe chip identifier.
    pub chip_id: ChipId,
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
#[derive(Debug, thiserror::Error)]
pub enum VerificationError {
    /// A per-chip STARK proof failed verification.
    #[error("chip '{chip_id}' verification failed: {detail}")]
    ChipVerificationFailed {
        /// Which chip failed.
        chip_id: ChipId,
        /// The underlying p3-uni-stark verification error message.
        detail: String,
    },
    /// The cross-chip LogUp cumulative sums do not sum to zero.
    #[error("LogUp imbalance: cumsum total = {total:?} (expected zero)")]
    LogUpImbalance {
        /// The nonzero total cumsum.
        total: [BabyBear; 4],
    },
    /// The proof's chip manifest is invalid (missing, extra, or duplicate chips).
    #[error("invalid chip manifest: {detail}")]
    InvalidChipManifest {
        /// Description of the manifest error.
        detail: String,
    },
    /// The proof's LogUp challenges do not match the Fiat-Shamir transcript.
    #[error("LogUp challenges mismatch: expected {expected:?}, got {got:?}")]
    ChallengesMismatch {
        /// The challenges re-derived by the verifier.
        expected: [EF4; 2],
        /// The challenges found in the proof.
        got: [EF4; 2],
    },
}
