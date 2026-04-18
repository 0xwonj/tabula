//! Proof model types for the multi-proof Tabula STARK.

use std::collections::BTreeMap;

use p3_field::PrimeCharacteristicRing;
use p3_koala_bear::KoalaBear;
use tabula_stark::air::interaction::BusId;
use tabula_stark::chips::ChipId;

use crate::config::{EF4, PcsCommitment, PcsOpeningProof};
use crate::input::ColumnSlotKey;

/// Proof tier identifier for canonical ordering.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProofTier {
    /// Tier 1: Execution proof (global, processes all instructions).
    Execution,
    /// Tier 2: Column proof for a specific machine slot.
    Column {
        /// Stable slot key for this column proof.
        key: ColumnSlotKey,
    },
    /// Tier 3: Root proof (verifies cumsum balance + SMT paths).
    Root,
}

impl std::fmt::Display for ProofTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProofTier::Execution => write!(f, "execution"),
            ProofTier::Column { key } => write!(f, "column{key}"),
            ProofTier::Root => write!(f, "root"),
        }
    }
}

/// A single sub-proof within a multi-proof.
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
    pub exported_cumsums: BTreeMap<BusId, EF4>,
}

/// Column proof entry (wraps a [`SubProofEnvelope`] with column identity).
pub struct ColumnProofEntry {
    /// Stable machine slot key for this column proof.
    pub key: ColumnSlotKey,
    /// The sub-proof for this column.
    pub proof: SubProofEnvelope,
}

/// A complete Tabula proof: C+2 independent sub-proofs.
pub struct TabulaProof {
    /// Tier 1: Execution proof.
    pub execution: SubProofEnvelope,
    /// Tier 2: Column proofs in canonical machine proof-plan order (one per
    /// `(table_id, col_id)`).
    pub columns: Vec<ColumnProofEntry>,
    /// Tier 3: Root proof.
    pub root: SubProofEnvelope,
    /// Canonical artifact-bound public-statement digest bound into the transcript.
    pub binding_digest: [u8; 32],
}

impl TabulaProof {
    /// Borrow one execution-tier chip opening by chip id.
    #[must_use]
    pub fn execution_chip_opening(&self, chip_id: ChipId) -> Option<&ChipOpening> {
        self.execution
            .chip_openings
            .iter()
            .find(|opening| opening.chip_id == chip_id)
    }

    /// Borrow one execution-tier chip's public values by chip id.
    #[must_use]
    pub fn execution_chip_public_values(&self, chip_id: ChipId) -> Option<&[KoalaBear]> {
        self.execution_chip_opening(chip_id)
            .map(|opening| opening.public_values.as_slice())
    }
}

/// Per-chip out-of-domain evaluations and metadata.
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
    /// Preprocessed trace at zeta·g (None if next-row preprocessing is unused).
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
    pub public_values: Vec<KoalaBear>,
}

/// Check that cross-proof bus cumsums balance to zero.
pub(crate) fn check_cross_proof_bus_balance<'a>(
    cumsum_maps: impl Iterator<Item = &'a BTreeMap<BusId, EF4>>,
) -> Result<(), (BusId, [KoalaBear; 4])> {
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
