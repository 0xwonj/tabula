//! Proving and verification error types.

use p3_koala_bear::KoalaBear;
use tabula_stark::air::interaction::BusId;
use tabula_stark::chips::ChipId;

use crate::input::ColumnSlotKey;
use crate::proof::model::ProofTier;

/// Errors during proof generation.
#[derive(Debug, thiserror::Error)]
pub enum ProveError {
    /// The machine proof input is malformed or inconsistent with the machine setup.
    #[error("invalid proof input: {detail}")]
    InvalidProofInput {
        /// Human-readable validation detail.
        detail: String,
    },
    /// A chip's trace height is not a power of two.
    #[error("chip '{chip_id}' trace height {height} is not a power of two")]
    InvalidTraceHeight {
        /// Which chip has the invalid trace.
        chip_id: ChipId,
        /// The actual (non-power-of-two) height.
        height: usize,
    },
    /// No keygen info found for a chip in the proving metadata.
    #[error("no keygen info for chip '{chip_id}'")]
    MissingKeygenInfo {
        /// Which chip is missing.
        chip_id: ChipId,
    },
    /// No chip traces were provided to the prover.
    #[error("no chip traces to prove")]
    NoChips,
    /// LogUp fingerprint evaluated to zero (division by zero).
    #[error("LogUp fingerprint is zero at row {row}, interaction {interaction}")]
    FingerprintZero {
        /// Trace row where the zero fingerprint occurred.
        row: usize,
        /// Interaction index within that row.
        interaction: usize,
    },
    /// An internal bus (should balance within one proof) has nonzero cumsum.
    #[error("internal bus imbalance in {tier} proof: {bus_id}, cumsum = {cumsum:?}")]
    InternalBusImbalance {
        /// Which proof tier has the imbalance.
        tier: ProofTier,
        /// Which bus has the imbalance.
        bus_id: BusId,
        /// The nonzero cumsum coefficients.
        cumsum: [KoalaBear; 4],
    },
    /// Cross-proof bus cumsum does not balance across all proof instances.
    #[error("cross-proof bus imbalance: {bus_id}, total = {total:?}")]
    CrossProofBusImbalance {
        /// Which bus has the imbalance.
        bus_id: BusId,
        /// The nonzero total cumsum.
        total: [KoalaBear; 4],
    },
}

impl From<tabula_stark::permutation::PermutationError> for ProveError {
    fn from(err: tabula_stark::permutation::PermutationError) -> Self {
        match err {
            tabula_stark::permutation::PermutationError::FingerprintZero { row, interaction } => {
                ProveError::FingerprintZero { row, interaction }
            }
        }
    }
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
    /// Column proof key does not match any verifier setup.
    #[error("column proof at index {index} has unknown slot {proof_key}")]
    ColumnKeyMismatch {
        /// Column proof index.
        index: usize,
        /// Stable slot key from the proof.
        proof_key: ColumnSlotKey,
    },
    /// Cross-proof bus cumsums do not balance.
    #[error("cross-proof bus imbalance: {bus_id}, total = {total:?}")]
    CrossProofBusImbalance {
        /// Which bus has the imbalance.
        bus_id: BusId,
        /// The nonzero total cumsum.
        total: [KoalaBear; 4],
    },
    /// Internal bus within a sub-proof does not balance.
    #[error("internal bus imbalance in {tier}: {bus_id}, cumsum = {cumsum:?}")]
    InternalBusImbalance {
        /// Which proof tier has the imbalance.
        tier: ProofTier,
        /// Which bus has the imbalance.
        bus_id: BusId,
        /// The nonzero cumsum.
        cumsum: [KoalaBear; 4],
    },
}
