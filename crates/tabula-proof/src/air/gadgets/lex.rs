//! Lex ordering direction operation: enforce `(t,c)` strictly increases.
//!
//! At segment boundaries, proves that `(table_id, col_id)` strictly
//! increases by range-checking `next - local - 1 ∈ [0, 2^16)`.
//!
//! Used by SSMC, Merge, SortedMem, and ColumnMeta chips.

use p3_air::AirBuilder;
use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;

use crate::air::builder::InteractionAirBuilder;
use crate::air::interaction::{AirInteraction, InteractionKind};

/// Lex ordering direction for `(table_id, col_id)` segment boundaries.
///
/// Columns: 3 (selector + 2 diffs).
///
/// When the gate is active:
/// - `diff_is_table=1`: `table_diff = next.table - local.table - 1 ∈ [0, 2^16)`
/// - `diff_is_table=0`: tables equal, `col_diff = next.col - local.col - 1 ∈ [0, 2^16)`
#[repr(C)]
#[derive(Clone, Debug)]
pub struct LexOrderingDirection<T> {
    /// 1 if table_id changes at this transition; 0 if same table.
    pub diff_is_table: T,
    /// `next.table_id - local.table_id - 1` (range-checked).
    pub table_diff: T,
    /// `next.col_id - local.col_id - 1` (range-checked).
    pub col_diff: T,
}

impl LexOrderingDirection<BabyBear> {
    /// Populate witness columns from IDs.
    ///
    /// Only fills meaningful values when `tc_changed` is true.
    /// When `tc_changed` is false, columns stay at their default (zero).
    pub fn populate(
        &mut self,
        cur_table: u32,
        next_table: u32,
        cur_col: u32,
        next_col: u32,
        tc_changed: bool,
    ) {
        if !tc_changed {
            return; // leave as zeros — constraints gated off
        }
        if cur_table != next_table {
            self.diff_is_table = BabyBear::ONE;
            self.table_diff = BabyBear::new(next_table.wrapping_sub(cur_table).wrapping_sub(1));
        } else {
            // same table, col changed
            self.col_diff = BabyBear::new(next_col.wrapping_sub(cur_col).wrapping_sub(1));
        }
    }
}

/// Constrain lex ordering direction at segment boundaries.
///
/// `gate` should be the expression that activates at segment boundaries:
/// - SSMC/Merge/SortedMem: `both_real * tc_changed`
/// - ColumnMeta: `both_real` (every consecutive real pair is a boundary)
pub fn constrain_lex_direction<AB: AirBuilder>(
    builder: &mut AB,
    lex: &LexOrderingDirection<AB::Var>,
    next_table: AB::Expr,
    local_table: AB::Expr,
    next_col: AB::Expr,
    local_col: AB::Expr,
    gate: AB::Expr,
) {
    builder.assert_bool(lex.diff_is_table.clone());

    let is_table: AB::Expr = lex.diff_is_table.clone().into();
    let not_table: AB::Expr = AB::Expr::ONE - is_table.clone();

    // Case 1: table changed → table_diff = next.table - local.table - 1
    let expected_table_diff: AB::Expr = next_table.clone() - local_table.clone() - AB::Expr::ONE;
    builder.when_transition().assert_zero(
        gate.clone() * is_table * (lex.table_diff.clone().into() - expected_table_diff),
    );

    // Case 2: same table → tables must be equal + col_diff = next.col - local.col - 1
    builder
        .when_transition()
        .assert_zero(gate.clone() * not_table.clone() * (next_table - local_table));
    let expected_col_diff: AB::Expr = next_col - local_col - AB::Expr::ONE;
    builder
        .when_transition()
        .assert_zero(gate * not_table * (lex.col_diff.clone().into() - expected_col_diff));
}

/// Send range checks for lex ordering direction diffs.
///
/// `tc_mult` is the multiplicity expression for segment-boundary rows
/// (typically `is_real * tc_changed` or similar).
pub fn send_lex_range_checks<AB: InteractionAirBuilder>(
    builder: &mut AB,
    lex: &LexOrderingDirection<AB::Var>,
    tc_mult: AB::Expr,
) {
    let is_table: AB::Expr = lex.diff_is_table.clone().into();
    builder.send(AirInteraction {
        values: vec![lex.table_diff.clone().into()],
        multiplicity: tc_mult.clone() * is_table.clone(),
        kind: InteractionKind::RangeCheck,
    });
    builder.send(AirInteraction {
        values: vec![lex.col_diff.clone().into()],
        multiplicity: tc_mult * (AB::Expr::ONE - is_table),
        kind: InteractionKind::RangeCheck,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lex_populate_table_changed() {
        let mut lex = LexOrderingDirection {
            diff_is_table: BabyBear::ZERO,
            table_diff: BabyBear::ZERO,
            col_diff: BabyBear::ZERO,
        };
        lex.populate(1, 3, 0, 0, true);
        assert_eq!(lex.diff_is_table, BabyBear::ONE);
        assert_eq!(lex.table_diff, BabyBear::new(1)); // 3 - 1 - 1 = 1
        assert_eq!(lex.col_diff, BabyBear::ZERO);
    }

    #[test]
    fn lex_populate_col_changed() {
        let mut lex = LexOrderingDirection {
            diff_is_table: BabyBear::ZERO,
            table_diff: BabyBear::ZERO,
            col_diff: BabyBear::ZERO,
        };
        lex.populate(1, 1, 2, 5, true);
        assert_eq!(lex.diff_is_table, BabyBear::ZERO);
        assert_eq!(lex.table_diff, BabyBear::ZERO);
        assert_eq!(lex.col_diff, BabyBear::new(2)); // 5 - 2 - 1 = 2
    }

    #[test]
    fn lex_populate_no_change() {
        let mut lex = LexOrderingDirection {
            diff_is_table: BabyBear::ZERO,
            table_diff: BabyBear::ZERO,
            col_diff: BabyBear::ZERO,
        };
        lex.populate(1, 1, 2, 2, false);
        assert_eq!(lex.diff_is_table, BabyBear::ZERO);
        assert_eq!(lex.table_diff, BabyBear::ZERO);
        assert_eq!(lex.col_diff, BabyBear::ZERO);
    }

    #[test]
    fn lex_populate_adjacent_tables() {
        let mut lex = LexOrderingDirection {
            diff_is_table: BabyBear::ZERO,
            table_diff: BabyBear::ZERO,
            col_diff: BabyBear::ZERO,
        };
        lex.populate(5, 6, 10, 0, true);
        assert_eq!(lex.diff_is_table, BabyBear::ONE);
        assert_eq!(lex.table_diff, BabyBear::ZERO); // 6 - 5 - 1 = 0
    }
}
