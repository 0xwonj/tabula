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
//! - [`InteractionKind`] — bus tag preventing cross-bus fingerprint collisions

use p3_field::Field;

/// Named interaction channels for cross-chip LogUp.
///
/// Each variant identifies a logical bus connecting two or more chips.
/// The integer discriminant is used as `kind_tag` in the RLC fingerprint
/// formula: `f = α + β⁰ · kind_tag + β¹ · values[0] + …`
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum InteractionKind {
    /// StateColumn/ColumnMeta hash chains ↔ PoseidonChip (permutation verification).
    PoseidonPermutation = 5,
    /// StateColumn segment hashes ↔ ColumnMeta (commitment verification).
    CommitmentVerification = 6,
    /// All chips → RangeCheckChip (u16 range proofs).
    RangeCheck = 8,
    /// ExecutionChip → StaticTableChip (static table lookups, receiver deferred to M11).
    StaticTableLookup = 9,
    /// Execution → InterTxOrder (read access tuple with `tx_index` anchor).
    ReadAccess = 10,
    /// Execution → InterTxOrder (write access tuple with `tx_index` anchor).
    WriteAccess = 11,
    /// Execution → ColumnMeta (read from empty column).
    EmptyColRead = 12,
    /// InterTxOrder → StateColumn (base state entries from init rows).
    BaseStateEntry = 13,
    /// InterTxOrder → StateColumn (coalesced writes from last-for-key rows).
    CoalescedWrite = 14,
    /// ColumnMeta → SmtColPathChip (leaf digests for SMT column paths).
    SmtLeafDigest = 15,
    /// SmtColPathChip → SmtTablePathChip (per-table SMT roots).
    SmtTableRoot = 16,
}

impl InteractionKind {
    /// Integer tag for use in RLC fingerprint computation.
    pub fn tag(self) -> u8 {
        self as u8
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
    pub kind: InteractionKind,
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
    pub kind: InteractionKind,
    /// Send or receive.
    pub direction: InteractionDirection,
}

#[cfg(test)]
mod tests {
    use super::*;
    use p3_baby_bear::BabyBear;
    use p3_field::PrimeCharacteristicRing;

    #[test]
    fn interaction_kind_tags_are_unique() {
        let kinds = [
            InteractionKind::PoseidonPermutation,
            InteractionKind::CommitmentVerification,
            InteractionKind::RangeCheck,
            InteractionKind::StaticTableLookup,
            InteractionKind::ReadAccess,
            InteractionKind::WriteAccess,
            InteractionKind::EmptyColRead,
            InteractionKind::BaseStateEntry,
            InteractionKind::CoalescedWrite,
            InteractionKind::SmtLeafDigest,
            InteractionKind::SmtTableRoot,
        ];
        let mut tags: Vec<u8> = kinds.iter().map(|k| k.tag()).collect();
        tags.sort();
        tags.dedup();
        assert_eq!(tags.len(), kinds.len(), "all tags must be unique");
    }

    #[test]
    fn interaction_kind_tags_are_nonzero() {
        // Tag 0 is reserved (unused) so kind_tag is always nonzero in fingerprints.
        assert_eq!(InteractionKind::PoseidonPermutation.tag(), 5);
        assert_eq!(InteractionKind::RangeCheck.tag(), 8);
        assert_eq!(InteractionKind::ReadAccess.tag(), 10);
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
