//! Core types for cross-chip LogUp interactions.
//!
//! The LogUp argument proves multiset equality across separate trace tables.
//! Each chip declares `send()` and `receive()` interactions during `eval()`.
//! A shared random challenge pair `(α, β)` generates RLC fingerprints, and
//! the system verifies that `Σ_chips Σ_rows m_i / f_i = 0` (sends positive,
//! receives negative).
//!
//! # Type hierarchy
//!
//! - [`AirInteraction<E>`] — emitted during `Air::eval()`, carries expressions
//! - [`Interaction<F>`] — static descriptor with [`VirtualPairCol`] references
//! - [`BusId`] — open bus identifier preventing cross-bus fingerprint collisions

use p3_field::Field;

/// Open bus identifier for cross-chip LogUp interactions.
///
/// Unlike a closed enum, `BusId` is a transparent newtype that allows
/// downstream crates to define new buses without modifying Tabula.
/// The inner `u16` is used as `kind_tag` in the RLC fingerprint
/// formula: `f = α + β⁰ · kind_tag + β¹ · values[0] + …`
///
/// Core bus IDs are defined in [`core_buses`]. Application-specific
/// buses should use IDs >= 100 to avoid collisions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BusId(pub u16);

impl BusId {
    /// Integer tag for use in RLC fingerprint computation.
    pub const fn tag(self) -> u16 {
        self.0
    }
}

impl std::fmt::Display for BusId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(name) = core_buses::name(self) {
            write!(f, "{name}({})", self.0)
        } else {
            write!(f, "Bus({})", self.0)
        }
    }
}

/// Core bus identifiers for the Tabula proof system.
///
/// Each constant corresponds to a logical bus connecting two or more chips.
/// Application-specific buses should use IDs >= 100.
pub mod core_buses {
    use super::BusId;

    /// StateColumn/ColumnMeta hash chains ↔ PoseidonChip (permutation verification).
    pub const POSEIDON_PERM: BusId = BusId(5);
    /// StateColumn segment hashes ↔ ColumnMeta (commitment verification).
    pub const COMMITMENT_VERIF: BusId = BusId(6);
    /// All chips → RangeCheckChip (u16 range proofs).
    pub const RANGE_CHECK: BusId = BusId(8);
    /// ExecutionChip → StaticTableChip (static table lookups).
    pub const STATIC_TABLE_LOOKUP: BusId = BusId(9);
    /// Execution → InterTxOrder (read access tuple with `tx_index` anchor).
    pub const READ_ACCESS: BusId = BusId(10);
    /// Execution → InterTxOrder (write access tuple with `tx_index` anchor).
    pub const WRITE_ACCESS: BusId = BusId(11);
    /// Execution → ColumnMeta (read from empty column).
    pub const EMPTY_COL_READ: BusId = BusId(12);
    /// InterTxOrder → StateColumn (base state entries from init rows).
    pub const BASE_STATE_ENTRY: BusId = BusId(13);
    /// InterTxOrder → StateColumn (coalesced writes from last-for-key rows).
    pub const COALESCED_WRITE: BusId = BusId(14);
    /// ColumnMeta → SmtColPathChip (leaf digests for SMT column paths).
    pub const SMT_LEAF_DIGEST: BusId = BusId(15);
    /// SmtColPathChip → SmtTablePathChip (per-table SMT roots).
    pub const SMT_TABLE_ROOT: BusId = BusId(16);

    /// All core bus IDs, for iteration and validation.
    pub const ALL: [BusId; 11] = [
        POSEIDON_PERM,
        COMMITMENT_VERIF,
        RANGE_CHECK,
        STATIC_TABLE_LOOKUP,
        READ_ACCESS,
        WRITE_ACCESS,
        EMPTY_COL_READ,
        BASE_STATE_ENTRY,
        COALESCED_WRITE,
        SMT_LEAF_DIGEST,
        SMT_TABLE_ROOT,
    ];

    /// Human-readable name for a core bus ID, or `None` for app-defined buses.
    pub const fn name(id: &BusId) -> Option<&'static str> {
        match id.0 {
            5 => Some("PoseidonPerm"),
            6 => Some("CommitmentVerif"),
            8 => Some("RangeCheck"),
            9 => Some("StaticTableLookup"),
            10 => Some("ReadAccess"),
            11 => Some("WriteAccess"),
            12 => Some("EmptyColRead"),
            13 => Some("BaseStateEntry"),
            14 => Some("CoalescedWrite"),
            15 => Some("SmtLeafDigest"),
            16 => Some("SmtTableRoot"),
            _ => None,
        }
    }
}

/// Direction of an interaction in the LogUp argument.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InteractionDirection {
    /// Chip sends tuples into the bus (positive contribution to LogUp sum).
    Send,
    /// Chip receives tuples from the bus (negative contribution).
    Receive,
}

// ─── Constraint-time types ──────────────────────────────────────────────────

/// Interaction emitted during `Air::eval()`.
///
/// `E` is the expression type — `AB::Expr` for constraint evaluation,
/// or a concrete field element for debug checking.
#[derive(Clone, Debug)]
pub struct AirInteraction<E> {
    /// Tuple elements forming the fingerprint.
    pub values: Vec<E>,
    /// Multiplicity selector (typically 0 or 1 per row).
    pub multiplicity: E,
    /// Which LogUp bus this interaction belongs to.
    pub bus: BusId,
}

// ─── Static descriptor types (for permutation trace generation) ─────────────

/// Reference to a column in the trace matrix.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColumnRef {
    /// Column at the current (local) row.
    Local(usize),
    /// Column at the next row.
    Next(usize),
}

/// A trace value expressed as an affine combination of columns.
///
/// Evaluates to: `constant + Σᵢ weightᵢ · trace[refᵢ]`
///
/// Used by the permutation trace generator to compute fingerprints
/// from the main trace matrix without re-running `eval()`.
#[derive(Clone, Debug)]
pub struct VirtualPairCol<F: Field> {
    /// Linear combination terms: `(column_ref, weight)`.
    pub column_weights: Vec<(ColumnRef, F)>,
    /// Constant offset.
    pub constant: F,
}

impl<F: Field> VirtualPairCol<F> {
    /// Reference to a single column at the local row.
    pub fn single_local(col: usize) -> Self {
        Self {
            column_weights: vec![(ColumnRef::Local(col), F::ONE)],
            constant: F::ZERO,
        }
    }

    /// Reference to a single column at the next row.
    pub fn single_next(col: usize) -> Self {
        Self {
            column_weights: vec![(ColumnRef::Next(col), F::ONE)],
            constant: F::ZERO,
        }
    }

    /// A constant value (no column dependency).
    pub fn constant(val: F) -> Self {
        Self {
            column_weights: vec![],
            constant: val,
        }
    }

    /// Evaluate against concrete trace row data.
    pub fn eval(&self, local: &[F], next: &[F]) -> F {
        let mut result = self.constant;
        for &(col_ref, weight) in &self.column_weights {
            let col_val = match col_ref {
                ColumnRef::Local(i) => local[i],
                ColumnRef::Next(i) => next[i],
            };
            result += weight * col_val;
        }
        result
    }
}

/// Static interaction descriptor with column references.
///
/// Extracted from symbolic evaluation of `Air::eval()`. Used by the
/// permutation trace generator and future prover integration.
#[derive(Clone, Debug)]
pub struct Interaction<F: Field> {
    /// Tuple elements as column references.
    pub values: Vec<VirtualPairCol<F>>,
    /// Multiplicity selector as a column reference.
    pub multiplicity: VirtualPairCol<F>,
    /// Which LogUp bus.
    pub bus: BusId,
    /// Send or receive.
    pub direction: InteractionDirection,
}

#[cfg(test)]
mod tests {
    use super::*;
    use p3_baby_bear::BabyBear;
    use p3_field::PrimeCharacteristicRing;

    #[test]
    fn bus_id_tags_are_unique() {
        let mut tags: Vec<u16> = core_buses::ALL.iter().map(|k| k.tag()).collect();
        tags.sort();
        tags.dedup();
        assert_eq!(tags.len(), core_buses::ALL.len(), "all tags must be unique");
    }

    #[test]
    fn bus_id_tags_are_nonzero() {
        // Tag 0 is reserved (unused) so kind_tag is always nonzero in fingerprints.
        assert_eq!(core_buses::POSEIDON_PERM.tag(), 5);
        assert_eq!(core_buses::RANGE_CHECK.tag(), 8);
        assert_eq!(core_buses::READ_ACCESS.tag(), 10);
    }

    #[test]
    fn bus_id_display() {
        assert_eq!(format!("{}", core_buses::RANGE_CHECK), "RangeCheck(8)");
        assert_eq!(format!("{}", BusId(100)), "Bus(100)");
    }

    #[test]
    fn virtual_pair_col_single_local() {
        let vpc = VirtualPairCol::<BabyBear>::single_local(2);
        let local = [BabyBear::new(10), BabyBear::new(20), BabyBear::new(30)];
        let next = [BabyBear::ZERO; 3];
        assert_eq!(vpc.eval(&local, &next), BabyBear::new(30));
    }

    #[test]
    fn virtual_pair_col_single_next() {
        let vpc = VirtualPairCol::<BabyBear>::single_next(0);
        let local = [BabyBear::ZERO; 3];
        let next = [BabyBear::new(42), BabyBear::ZERO, BabyBear::ZERO];
        assert_eq!(vpc.eval(&local, &next), BabyBear::new(42));
    }

    #[test]
    fn virtual_pair_col_constant() {
        let vpc = VirtualPairCol::<BabyBear>::constant(BabyBear::new(99));
        assert_eq!(vpc.eval(&[], &[]), BabyBear::new(99));
    }

    #[test]
    fn virtual_pair_col_linear_combination() {
        // 5 * local[0] + 3 * next[1] + 7
        let vpc = VirtualPairCol {
            column_weights: vec![
                (ColumnRef::Local(0), BabyBear::new(5)),
                (ColumnRef::Next(1), BabyBear::new(3)),
            ],
            constant: BabyBear::new(7),
        };
        let local = [BabyBear::new(10)];
        let next = [BabyBear::ZERO, BabyBear::new(20)];
        // 5*10 + 3*20 + 7 = 50 + 60 + 7 = 117
        assert_eq!(vpc.eval(&local, &next), BabyBear::new(117));
    }
}
