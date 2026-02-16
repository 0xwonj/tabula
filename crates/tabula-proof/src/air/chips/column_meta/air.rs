//! ColumnMetaChip — AIR constraints for the ColumnMeta global table.
//!
//! The ColumnMeta table tracks per-column commitment transitions during a batch.
//! Each row is a `(table, col)` pair with old/new commitments and flags.
//!
//! Constraints (proof-spec §4.2 ColumnMeta):
//! 1. Boolean fields: `is_real`, `tag`, `is_empty_old`, `is_empty_new`, `is_touched` ∈ {0,1}
//! 2. `is_real` prefix: `is_real_{i+1} ≤ is_real_i`
//! 3. Strict sorted order: real rows have `(table_id, col_id)` strictly increasing
//! 4. Untouched binding: `is_touched=0 ⟹ com_new = com_old`
//!
//! M7 upgrade: lex ordering uses `IsZero` gadgets for sound uniqueness detection.
//! Range-checked positive direction deferred to M9 (LogUp wiring for RangeCheck bus).
//!
//! 5. Touched consistency: `is_touched=0 ⟹ is_empty_new = is_empty_old`
//! 6. Empty→non-empty transition: `is_empty_old=1 ∧ is_touched=1 ⟹ is_empty_new=0`
//!
//! Deferred to M9: `Com_empty` hash verification (PoseidonPermutation bus),
//! ColumnMeta join lookups (LogUp), range-check direction enforcement.

use p3_air::{Air, AirBuilder, BaseAir};
use p3_field::PrimeCharacteristicRing;
use p3_matrix::Matrix;

use crate::air::columns::borrow_cols;
use crate::air::gadgets::{constrain_is_real_prefix, constrain_is_zero};

use super::columns::{COLUMN_META_WIDTH, ColumnMetaCols, DIGEST_WIDTH};

/// The ColumnMeta AIR chip.
#[derive(Debug)]
pub struct ColumnMetaChip;

impl<F> BaseAir<F> for ColumnMetaChip {
    fn width(&self) -> usize {
        COLUMN_META_WIDTH
    }
}

impl<AB: AirBuilder> Air<AB> for ColumnMetaChip {
    fn eval(&self, builder: &mut AB) {
        let main = builder.main();
        let local_row = main.row_slice(0).expect("trace must have at least one row");
        let next_row = main
            .row_slice(1)
            .expect("trace must have at least two rows");
        let local: &ColumnMetaCols<AB::Var> = borrow_cols(&local_row);
        let next: &ColumnMetaCols<AB::Var> = borrow_cols(&next_row);

        // ── 1. Boolean constraints ──
        // Note: is_real boolean is asserted by constrain_is_real_prefix below.
        builder.assert_bool(local.tag.clone());
        builder.assert_bool(local.is_empty_old.clone());
        builder.assert_bool(local.is_empty_new.clone());
        builder.assert_bool(local.is_touched.clone());

        // ── 2. is_real prefix ──
        constrain_is_real_prefix(builder, local.is_real.clone(), next.is_real.clone());

        // ── 3. Strict lexicographic (table_id, col_id) ordering ──
        //
        // Uses IsZero gadgets (M7 upgrade) for sound same-key detection.
        //
        // Logic: When both rows are real:
        //   - table_diff = next.table_id - local.table_id
        //   - col_diff = next.col_id - local.col_id
        //   - IsZero constrains table_diff_iz.is_zero correctly
        //   - IsZero constrains col_diff_iz.is_zero correctly
        //   - If table_diff = 0: col_diff must be nonzero (col_diff_iz.is_zero = 0)
        //   - If table_diff ≠ 0: no constraint on col_diff
        //
        // Note: This enforces uniqueness but not direction (strictly increasing).
        // Full direction enforcement requires range checks on diffs (deferred to M9).
        {
            let table_diff: AB::Expr = next.table_id.clone().into() - local.table_id.clone().into();
            let col_diff: AB::Expr = next.col_id.clone().into() - local.col_id.clone().into();

            // Constrain IsZero gadgets on table_diff and col_diff.
            constrain_is_zero(builder, table_diff, &local.table_diff_iz);
            constrain_is_zero(builder, col_diff, &local.col_diff_iz);

            // When both rows are real and same table: col must differ.
            // both_real * table_same * col_same = 0
            let both_real: AB::Expr = local.is_real.clone().into() * next.is_real.clone().into();
            let table_same: AB::Expr = local.table_diff_iz.is_zero.clone().into();
            let col_same: AB::Expr = local.col_diff_iz.is_zero.clone().into();

            builder
                .when_transition()
                .assert_zero(both_real * table_same * col_same);
        }

        // ── 4. Untouched binding: is_touched=0 ⟹ com_new = com_old ──
        {
            let not_touched: AB::Expr = AB::Expr::ONE - local.is_touched.clone().into();
            for i in 0..DIGEST_WIDTH {
                builder
                    .when(local.is_real.clone())
                    .when(not_touched.clone())
                    .assert_eq(local.com_new[i].clone(), local.com_old[i].clone());
            }
        }

        // ── 5. Touched consistency: is_touched=0 ⟹ is_empty_new = is_empty_old ──
        {
            let not_touched: AB::Expr = AB::Expr::ONE - local.is_touched.clone().into();
            let empty_diff: AB::Expr =
                local.is_empty_new.clone().into() - local.is_empty_old.clone().into();
            builder
                .when(local.is_real.clone())
                .assert_zero(not_touched * empty_diff);
        }

        // ── 6. Empty→non-empty: is_empty_old=1 ∧ is_touched=1 ⟹ is_empty_new=0 ──
        //
        // Equivalently: is_real * is_empty_old * is_touched * is_empty_new = 0
        builder.assert_zero(
            local.is_real.clone().into()
                * local.is_empty_old.clone().into()
                * local.is_touched.clone().into()
                * local.is_empty_new.clone().into(),
        );
    }
}
