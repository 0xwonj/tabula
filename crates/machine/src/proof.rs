//! Proof types for the multi-proof Tabula STARK.
//!
//! A [`TabulaProof`] contains C+2 independent sub-proofs:
//! one execution proof, C column proofs, and one root proof.
//! Each sub-proof has its own PCS commitments and FRI opening proof.
//!
//! Cross-proof soundness is ensured by:
//! 1. Shared LogUp challenges (α, β) derived from all main commitments
//! 2. Per-bus cumsum exports verified by the root proof

use std::collections::BTreeMap;

use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;

use tabula_stark::air::interaction::BusId;
use tabula_stark::air::statement::PublicStatement;
use tabula_stark::chips::ChipId;

use crate::config::{EF4, PcsCommitment, PcsOpeningProof};

// ── Proof Tier ───────────────────────────────────────────────────────────────

/// Proof tier identifier for canonical ordering.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProofTier {
    /// Tier 1: Execution proof (global, processes all instructions).
    Execution,
    /// Tier 2: Column proof for a specific `(table_id, col_id)`.
    Column {
        /// Table identifier.
        table_id: u32,
        /// Column identifier.
        col_id: u16,
    },
    /// Tier 3: Root proof (verifies cumsum balance + SMT paths).
    Root,
}

impl std::fmt::Display for ProofTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProofTier::Execution => write!(f, "execution"),
            ProofTier::Column { table_id, col_id } => {
                write!(f, "column({table_id},{col_id})")
            }
            ProofTier::Root => write!(f, "root"),
        }
    }
}

// ── Column Identity ──────────────────────────────────────────────────────────

/// Column identity and commitment data for a column proof.
///
/// Pairs a `(table_id, col_id)` with old/new commitment digests.
/// Used during proving to tag each column sub-proof with its identity.
#[derive(Clone, Copy)]
pub struct ColumnIdentity {
    /// Table identifier.
    pub table_id: u32,
    /// Column identifier.
    pub col_id: u16,
    /// Old commitment digest.
    pub com_old: [BabyBear; 8],
    /// New commitment digest.
    pub com_new: [BabyBear; 8],
}

// ── Sub-Proof Types ──────────────────────────────────────────────────────────

/// A single sub-proof within a multi-proof.
///
/// Contains the standard STARK proof data (PCS commitments, opening proof,
/// per-chip evaluations) plus per-bus cumsum exports for cross-proof
/// verification.
pub struct SubProofEnvelope {
    /// Which tier this sub-proof belongs to.
    pub tier: ProofTier,
    /// Commitment to preprocessed traces (if any).
    pub preprocessed_commitment: Option<PcsCommitment>,
    /// Commitment to main traces.
    pub main_commitment: PcsCommitment,
    /// Commitment to permutation traces (if any).
    pub perm_commitment: Option<PcsCommitment>,
    /// Commitment to quotient polynomial chunks.
    pub quotient_commitment: PcsCommitment,
    /// FRI opening proof for this sub-proof's commitments.
    pub opening_proof: PcsOpeningProof,
    /// Per-chip OOD evaluations.
    pub chip_openings: Vec<ChipOpening>,
    /// Per-bus cumulative sums exported from this sub-proof.
    ///
    /// Internal buses (balanced within this proof) are not included.
    /// Only external buses (ReadAccess, WriteAccess) appear here.
    pub exported_cumsums: BTreeMap<BusId, EF4>,
}

/// Column proof entry (wraps a [`SubProofEnvelope`] with column identity).
pub struct ColumnProofEntry {
    /// Column identity and commitment data.
    pub identity: ColumnIdentity,
    /// The sub-proof for this column.
    pub proof: SubProofEnvelope,
}

// ── Tabula Proof ─────────────────────────────────────────────────────────────

/// A complete Tabula proof: C+2 independent sub-proofs.
///
/// The verifier reconstructs shared LogUp challenges from all main
/// commitments, verifies each sub-proof independently, then checks
/// cross-proof bus balance via the root proof.
pub struct TabulaProof {
    /// Tier 1: Execution proof.
    pub execution: SubProofEnvelope,
    /// Tier 2: Column proofs (one per `(table_id, col_id)`).
    pub columns: Vec<ColumnProofEntry>,
    /// Tier 3: Root proof.
    pub root: SubProofEnvelope,
    /// The public statement this proof attests to.
    pub statement: PublicStatement,
}

// ── Per-Chip Openings ────────────────────────────────────────────────────────

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

// ── Cross-Proof Balance ──────────────────────────────────────────────────────

/// Check that cross-proof bus cumsums balance to zero.
///
/// Accumulates cumsums from all tiers and verifies each bus total is zero.
/// Returns the first imbalanced `(BusId, coefficients)` if any.
pub(crate) fn check_cross_proof_bus_balance<'a>(
    cumsum_maps: impl Iterator<Item = &'a BTreeMap<BusId, EF4>>,
) -> Result<(), (BusId, [BabyBear; 4])> {
    let mut totals: BTreeMap<BusId, EF4> = BTreeMap::new();
    for map in cumsum_maps {
        for (&bus, &cs) in map {
            *totals.entry(bus).or_insert(EF4::ZERO) += cs;
        }
    }
    for (&bus_id, &total) in &totals {
        if total != EF4::ZERO {
            return Err((bus_id, tabula_stark::rap::ef4::ef4_coeffs(total)));
        }
    }
    Ok(())
}

// ── Errors ───────────────────────────────────────────────────────────────────

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
    /// An internal bus (should balance within one proof) has nonzero cumsum.
    #[error("internal bus imbalance in {tier} proof: {bus_id}, cumsum = {cumsum:?}")]
    InternalBusImbalance {
        /// Which proof tier has the imbalance.
        tier: ProofTier,
        /// Which bus has the imbalance.
        bus_id: BusId,
        /// The nonzero cumsum coefficients.
        cumsum: [BabyBear; 4],
    },
    /// Cross-proof bus cumsum does not balance across all proof instances.
    #[error("cross-proof bus imbalance: {bus_id}, total = {total:?}")]
    CrossProofBusImbalance {
        /// Which bus has the imbalance.
        bus_id: BusId,
        /// The nonzero total cumsum.
        total: [BabyBear; 4],
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
    /// Column proof identity does not match any verifier setup.
    #[error("column proof at index {index} has unknown identity ({proof_table},{proof_col})")]
    ColumnIdentityMismatch {
        /// Column proof index.
        index: usize,
        /// Table ID from the proof.
        proof_table: u32,
        /// Column ID from the proof.
        proof_col: u16,
    },
    /// Cross-proof bus cumsums do not balance.
    #[error("cross-proof bus imbalance: {bus_id}, total = {total:?}")]
    CrossProofBusImbalance {
        /// Which bus has the imbalance.
        bus_id: BusId,
        /// The nonzero total cumsum.
        total: [BabyBear; 4],
    },
    /// Internal bus within a sub-proof does not balance.
    #[error("internal bus imbalance in {tier}: {bus_id}, cumsum = {cumsum:?}")]
    InternalBusImbalance {
        /// Which proof tier has the imbalance.
        tier: ProofTier,
        /// Which bus has the imbalance.
        bus_id: BusId,
        /// The nonzero cumsum.
        cumsum: [BabyBear; 4],
    },
}
