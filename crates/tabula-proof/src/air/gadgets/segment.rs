//! Segment detection operation: same-(t,c) detection via IsZero gadgets.
//!
//! Detects `(table_id, col_id)` changes between consecutive rows.
//! Bundles two IsZero gadgets + a derived `tc_changed` flag.
//!
//! Used by SSMC, Merge, and SortedMem chips.

use p3_air::AirBuilder;
use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;

use super::integer::{IsZero, constrain_is_zero};

/// Segment change detection: detects when `(table_id, col_id)` changes.
///
/// Columns: 5 (IsZero × 2 + tc_changed flag).
///
/// `tc_changed = 1 - table_same × col_same` where `*_same = IsZero.is_zero`.
#[repr(C)]
#[derive(Clone, Debug)]
pub struct SameKeyDetection<T> {
    /// IsZero for `(next.table_id - local.table_id)`.
    pub table_diff_iz: IsZero<T>,
    /// IsZero for `(next.col_id - local.col_id)`.
    pub col_diff_iz: IsZero<T>,
    /// 1 if `(table_id, col_id)` changes from this row to the next.
    pub tc_changed: T,
}

impl SameKeyDetection<BabyBear> {
    /// Populate witness columns from field-element diffs.
    pub fn populate(&mut self, table_diff: BabyBear, col_diff: BabyBear) {
        self.table_diff_iz.populate(table_diff);
        self.col_diff_iz.populate(col_diff);
        let table_same = table_diff == BabyBear::ZERO;
        let col_same = col_diff == BabyBear::ZERO;
        self.tc_changed = if table_same && col_same {
            BabyBear::ZERO
        } else {
            BabyBear::ONE
        };
    }
}

/// Constrain same-key detection: IsZero gadgets + tc_changed derivation.
///
/// Emits:
/// - IsZero constraints on `table_diff` and `col_diff`
/// - `tc_changed = 1 - table_same × col_same` (gated by `both_real`, transition only)
///
/// `table_diff` and `col_diff` are `next.id - local.id` expressions.
pub fn constrain_same_key_detection<AB: AirBuilder>(
    builder: &mut AB,
    segment: &SameKeyDetection<AB::Var>,
    table_diff: AB::Expr,
    col_diff: AB::Expr,
    both_real: AB::Expr,
) {
    constrain_is_zero(builder, table_diff, &segment.table_diff_iz);
    constrain_is_zero(builder, col_diff, &segment.col_diff_iz);

    let table_same: AB::Expr = segment.table_diff_iz.is_zero.clone().into();
    let col_same: AB::Expr = segment.col_diff_iz.is_zero.clone().into();
    let expected_tc_changed: AB::Expr = AB::Expr::ONE - table_same * col_same;
    builder
        .when_transition()
        .assert_zero(both_real * (segment.tc_changed.clone().into() - expected_tc_changed));
}

#[cfg(test)]
mod tests {
    use super::*;
    use p3_baby_bear::BabyBear;
    use p3_field::PrimeCharacteristicRing;

    #[test]
    fn same_key_populate_same_segment() {
        let mut seg = SameKeyDetection {
            table_diff_iz: IsZero {
                inv: BabyBear::ZERO,
                is_zero: BabyBear::ZERO,
            },
            col_diff_iz: IsZero {
                inv: BabyBear::ZERO,
                is_zero: BabyBear::ZERO,
            },
            tc_changed: BabyBear::ZERO,
        };
        seg.populate(BabyBear::ZERO, BabyBear::ZERO);
        assert_eq!(seg.tc_changed, BabyBear::ZERO);
        assert_eq!(seg.table_diff_iz.is_zero, BabyBear::ONE);
        assert_eq!(seg.col_diff_iz.is_zero, BabyBear::ONE);
    }

    #[test]
    fn same_key_populate_table_changed() {
        let mut seg = SameKeyDetection {
            table_diff_iz: IsZero {
                inv: BabyBear::ZERO,
                is_zero: BabyBear::ZERO,
            },
            col_diff_iz: IsZero {
                inv: BabyBear::ZERO,
                is_zero: BabyBear::ZERO,
            },
            tc_changed: BabyBear::ZERO,
        };
        seg.populate(BabyBear::new(3), BabyBear::ZERO);
        assert_eq!(seg.tc_changed, BabyBear::ONE);
        assert_eq!(seg.table_diff_iz.is_zero, BabyBear::ZERO);
    }

    #[test]
    fn same_key_populate_col_changed() {
        let mut seg = SameKeyDetection {
            table_diff_iz: IsZero {
                inv: BabyBear::ZERO,
                is_zero: BabyBear::ZERO,
            },
            col_diff_iz: IsZero {
                inv: BabyBear::ZERO,
                is_zero: BabyBear::ZERO,
            },
            tc_changed: BabyBear::ZERO,
        };
        seg.populate(BabyBear::ZERO, BabyBear::new(5));
        assert_eq!(seg.tc_changed, BabyBear::ONE);
        assert_eq!(seg.table_diff_iz.is_zero, BabyBear::ONE);
        assert_eq!(seg.col_diff_iz.is_zero, BabyBear::ZERO);
    }
}
