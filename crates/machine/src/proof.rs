//! Proof types for the batched multi-chip Tabula STARK.
//!
//! A [`TabulaProof`] contains shared PCS commitments across all chips
//! with a single FRI opening proof, plus per-chip OOD evaluations.

use p3_baby_bear::BabyBear;

use tabula_stark::air::statement::PublicStatement;
use tabula_stark::chips::ChipId;

use crate::config::{EF4, PcsCommitment, PcsOpeningProof};

/// A complete Tabula batch proof with shared PCS.
///
/// Uses a 3-round protocol:
/// 1. **Round 1**: Commit all main traces → sample LogUp challenges
/// 2. **Round 2**: Commit all permutation traces → sample alpha
/// 3. **Round 3**: Commit all quotient polynomials → sample zeta
///
/// A single FRI opening proof covers all committed data.
pub struct TabulaProof {
    /// Commitment to preprocessed traces (e.g., Poseidon round constants).
    /// `None` if no chip requires preprocessing.
    pub preprocessed_commitment: Option<PcsCommitment>,
    /// Round 1: shared commitment to all chip main traces.
    pub main_commitment: PcsCommitment,
    /// Round 2: shared commitment to all chip permutation traces.
    /// `None` if no chip has LogUp interactions (unlikely in practice).
    pub perm_commitment: Option<PcsCommitment>,
    /// Round 3: shared commitment to all quotient polynomial chunks.
    pub quotient_commitment: PcsCommitment,
    /// Single FRI opening proof for all commitments.
    pub opening_proof: PcsOpeningProof,
    /// Per-chip OOD evaluations and metadata.
    pub chip_openings: Vec<ChipOpening>,
    /// The public statement this proof attests to.
    pub statement: PublicStatement,
}

/// Per-chip out-of-domain evaluations and metadata.
///
/// Contains the polynomial evaluations at the random point `zeta`
/// and `zeta_next` for each chip's traces.
pub struct ChipOpening {
    /// Type-safe chip identifier.
    pub chip_id: ChipId,
    /// Main trace evaluated at zeta (one value per column).
    pub main_local: Vec<EF4>,
    /// Main trace evaluated at zeta·g (next row).
    pub main_next: Vec<EF4>,
    /// Permutation trace evaluated at zeta (empty if no interactions).
    pub perm_local: Vec<EF4>,
    /// Permutation trace evaluated at zeta·g (empty if no interactions).
    pub perm_next: Vec<EF4>,
    /// Preprocessed trace at zeta (None if no preprocessing).
    pub preprocessed_local: Option<Vec<EF4>>,
    /// Preprocessed trace at zeta·g (None if no preprocessing).
    pub preprocessed_next: Option<Vec<EF4>>,
    /// Quotient polynomial chunks evaluated at zeta.
    pub quotient_chunks: Vec<Vec<EF4>>,
    /// log2(trace_height).
    pub degree_bits: usize,
    /// Width of the main trace.
    pub main_width: usize,
    /// Width of the permutation trace (0 if no interactions).
    pub perm_width: usize,
    /// Final LogUp cumulative sum for this chip.
    pub cumsum_final: EF4,
    /// log2(number of quotient chunks).
    pub log_quotient_chunks: usize,
    /// Public values for this chip.
    pub public_values: Vec<BabyBear>,
}

/// Errors during proof generation.
#[derive(Debug, thiserror::Error)]
pub enum ProveError {
    /// A chip's trace height is not a power of two.
    #[error("chip '{chip_id}' trace height {height} is not a power of two")]
    InvalidTraceHeight {
        /// Which chip has the invalid trace.
        chip_id: ChipId,
        /// The actual (non-power-of-two) height.
        height: usize,
    },
    /// No keygen info found for a chip in the proving key.
    #[error("no keygen info for chip '{chip_id}'")]
    MissingKeygenInfo {
        /// Which chip is missing.
        chip_id: ChipId,
    },
    /// No chip traces were provided to the prover.
    #[error("no chip traces to prove")]
    NoChips,
    /// Cross-chip LogUp cumulative sums do not balance to zero.
    #[error("LogUp imbalance during proving: cumsum total = {total:?}")]
    LogUpImbalance {
        /// The nonzero total cumsum (4 BabyBear coefficients).
        total: [BabyBear; 4],
    },
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

/// Errors during proof verification.
#[derive(Debug, thiserror::Error)]
pub enum VerificationError {
    /// A per-chip constraint check failed.
    #[error("chip '{chip_id}' verification failed: {detail}")]
    ChipVerificationFailed {
        /// Which chip failed.
        chip_id: ChipId,
        /// The underlying verification error message.
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
    /// PCS verification failed.
    #[error("PCS verification failed: {detail}")]
    PcsVerificationFailed {
        /// Error from the PCS verify call.
        detail: String,
    },
}
