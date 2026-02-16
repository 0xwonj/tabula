//! Trace generation for the ColumnMeta chip.
//!
//! Converts `ColumnMeta` witness data into a `RowMajorMatrix<BabyBear>` trace.

use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;
use p3_matrix::dense::RowMajorMatrix;

use tabula_commitment::ColumnMeta;

use crate::air::columns::borrow_cols_mut;
use crate::air::gadgets::bool_fe;

use super::columns::{COLUMN_META_WIDTH, ColumnMetaCols};

/// Generate a ColumnMeta trace from witness data.
///
/// Rows are padded to the next power of two (Plonky3 requirement).
/// Padding rows have `is_real = 0`.
pub fn generate_column_meta_trace(metas: &[ColumnMeta]) -> RowMajorMatrix<BabyBear> {
    let width = COLUMN_META_WIDTH;
    let num_real = metas.len();
    let num_rows = (num_real + 1).next_power_of_two().max(2); // min 2 rows for transition
    let mut values = vec![BabyBear::ZERO; num_rows * width];

    for (i, meta) in metas.iter().enumerate() {
        let offset = i * width;
        let row: &mut [BabyBear] = &mut values[offset..offset + width];
        let cols: &mut ColumnMetaCols<BabyBear> = borrow_cols_mut(row);

        cols.is_real = BabyBear::ONE;
        cols.table_id = BabyBear::new(meta.table.0);
        cols.col_id = BabyBear::new(meta.col.0 as u32);
        cols.tag = match meta.tag {
            tabula_commitment::CommitmentStrategy::Ssmc => BabyBear::ZERO,
            tabula_commitment::CommitmentStrategy::Smt => BabyBear::ONE,
        };
        cols.com_old = meta.com_old.0;
        cols.com_new = meta.com_new.0;
        cols.is_empty_old = bool_fe(meta.is_empty_old);
        cols.is_empty_new = bool_fe(meta.is_empty_new);
        cols.is_touched = bool_fe(meta.is_touched);

        // Compute IsZero witness columns for lex ordering.
        if i + 1 < num_real {
            let next_meta = &metas[i + 1];

            let t_diff = BabyBear::new(next_meta.table.0) - BabyBear::new(meta.table.0);
            cols.table_diff_iz.populate(t_diff);

            let c_diff = BabyBear::new(next_meta.col.0 as u32) - BabyBear::new(meta.col.0 as u32);
            cols.col_diff_iz.populate(c_diff);
        } else {
            // Last real row or padding: IsZero witnesses for zero diffs
            // (transition to padding where both IDs are 0).
            cols.table_diff_iz
                .populate(BabyBear::ZERO - BabyBear::new(meta.table.0));
            cols.col_diff_iz
                .populate(BabyBear::ZERO - BabyBear::new(meta.col.0 as u32));
        }
    }

    // Padding rows: IsZero witnesses must be consistent with the actual diffs.
    // The debug checker (and real prover) wraps: the last row's "next" is row 0.
    // For non-last padding rows, the next row is also padding (diff = 0).
    // For the last row, the next row wraps to row 0 (diff = row_0_val - 0).
    let row_0_table = if num_real > 0 {
        BabyBear::new(metas[0].table.0)
    } else {
        BabyBear::ZERO
    };
    let row_0_col = if num_real > 0 {
        BabyBear::new(metas[0].col.0 as u32)
    } else {
        BabyBear::ZERO
    };

    for i in num_real..num_rows {
        let offset = i * width;
        let row: &mut [BabyBear] = &mut values[offset..offset + width];
        let cols: &mut ColumnMetaCols<BabyBear> = borrow_cols_mut(row);

        if i == num_rows - 1 {
            // Last row wraps to row 0.
            cols.table_diff_iz.populate(row_0_table);
            cols.col_diff_iz.populate(row_0_col);
        } else {
            // Non-last padding: next is also padding (diff = 0).
            cols.table_diff_iz.populate(BabyBear::ZERO);
            cols.col_diff_iz.populate(BabyBear::ZERO);
        }
    }

    RowMajorMatrix::new(values, width)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::air::debug::debug_check;
    use tabula_commitment::{CommitmentStrategy, NativeDigest};
    use tabula_core::{ColId, TableId};

    use super::super::air::ColumnMetaChip;

    fn meta(
        table: u32,
        col: u16,
        touched: bool,
        com_old: NativeDigest,
        com_new: NativeDigest,
    ) -> ColumnMeta {
        ColumnMeta {
            table: TableId(table),
            col: ColId(col),
            tag: CommitmentStrategy::Ssmc,
            com_old,
            com_new,
            is_empty_old: false,
            is_empty_new: false,
            is_touched: touched,
        }
    }

    fn distinct_digest(seed: u32) -> NativeDigest {
        let mut fes = [BabyBear::ZERO; 8];
        for (i, fe) in fes.iter_mut().enumerate() {
            *fe = BabyBear::new(seed * 100 + i as u32);
        }
        NativeDigest(fes)
    }

    // ── Valid traces ──

    #[test]
    fn valid_three_real_rows_plus_padding() {
        let d1 = distinct_digest(1);
        let d2 = distinct_digest(2);
        let d3 = distinct_digest(3);
        let d4 = distinct_digest(4);
        let d5 = distinct_digest(5);
        let d6 = distinct_digest(6);
        let metas = vec![
            meta(0, 0, true, d1, d2),
            meta(0, 1, true, d3, d4),
            meta(1, 0, true, d5, d6),
        ];
        let trace = generate_column_meta_trace(&metas);
        debug_check(&ColumnMetaChip, &trace).expect("valid trace should pass");
    }

    #[test]
    fn valid_single_real_row() {
        let d1 = distinct_digest(1);
        let d2 = distinct_digest(2);
        let metas = vec![meta(0, 0, true, d1, d2)];
        let trace = generate_column_meta_trace(&metas);
        debug_check(&ColumnMetaChip, &trace).expect("single row should pass");
    }

    #[test]
    fn valid_all_padding() {
        let metas: Vec<ColumnMeta> = vec![];
        let trace = generate_column_meta_trace(&metas);
        debug_check(&ColumnMetaChip, &trace).expect("all-padding should pass");
    }

    #[test]
    fn valid_untouched_com_equal() {
        let d1 = distinct_digest(1);
        // Untouched: com_new = com_old.
        let metas = vec![meta(0, 0, false, d1, d1)];
        let trace = generate_column_meta_trace(&metas);
        debug_check(&ColumnMetaChip, &trace).expect("untouched with equal coms should pass");
    }

    // ── Invalid traces ──

    #[test]
    fn invalid_boolean_tag_equals_2() {
        let d1 = distinct_digest(1);
        let d2 = distinct_digest(2);
        let metas = vec![meta(0, 0, true, d1, d2)];
        let mut trace = generate_column_meta_trace(&metas);
        // Set tag = 2 (invalid boolean).
        let tag_offset = 3; // is_real(0), table_id(1), col_id(2), tag(3)
        trace.values[tag_offset] = BabyBear::TWO;
        debug_check(&ColumnMetaChip, &trace).expect_err("tag=2 should fail boolean check");
    }

    #[test]
    fn invalid_is_real_prefix_zero_then_one() {
        let d1 = distinct_digest(1);
        let d2 = distinct_digest(2);
        let d3 = distinct_digest(3);
        let d4 = distinct_digest(4);
        let metas = vec![meta(0, 0, true, d1, d2), meta(0, 1, true, d3, d4)];
        let mut trace = generate_column_meta_trace(&metas);
        // Set row 0 is_real = 0, row 1 is_real = 1 -> violates prefix.
        trace.values[0] = BabyBear::ZERO; // row 0, is_real
        debug_check(&ColumnMetaChip, &trace).expect_err("0->1 should fail is_real prefix");
    }

    #[test]
    fn invalid_duplicate_table_col() {
        let d1 = distinct_digest(1);
        let d2 = distinct_digest(2);
        let d3 = distinct_digest(3);
        let d4 = distinct_digest(4);
        let metas = vec![meta(0, 0, true, d1, d2), meta(0, 0, true, d3, d4)];
        let trace = generate_column_meta_trace(&metas);
        debug_check(&ColumnMetaChip, &trace).expect_err("duplicate (t,c) should fail ordering");
    }

    #[test]
    fn invalid_untouched_com_mismatch() {
        let d1 = distinct_digest(1);
        let d2 = distinct_digest(2);
        // Untouched but com_new != com_old.
        let metas = vec![meta(0, 0, false, d1, d2)];
        let trace = generate_column_meta_trace(&metas);
        debug_check(&ColumnMetaChip, &trace)
            .expect_err("untouched with different coms should fail");
    }

    #[test]
    fn valid_smt_tag() {
        let d1 = distinct_digest(1);
        let d2 = distinct_digest(2);
        let metas = vec![ColumnMeta {
            table: TableId(0),
            col: ColId(0),
            tag: CommitmentStrategy::Smt,
            com_old: d1,
            com_new: d2,
            is_empty_old: false,
            is_empty_new: false,
            is_touched: true,
        }];
        let trace = generate_column_meta_trace(&metas);
        debug_check(&ColumnMetaChip, &trace).expect("SMT tag=1 is valid boolean");
    }

    #[test]
    fn valid_many_rows_different_tables() {
        let metas: Vec<ColumnMeta> = (0..7)
            .map(|i| {
                meta(
                    i,
                    0,
                    true,
                    distinct_digest(i * 2),
                    distinct_digest(i * 2 + 1),
                )
            })
            .collect();
        let trace = generate_column_meta_trace(&metas);
        debug_check(&ColumnMetaChip, &trace).expect("many different tables should pass");
    }

    #[test]
    fn valid_many_cols_same_table() {
        let metas: Vec<ColumnMeta> = (0..5)
            .map(|i| {
                meta(
                    0,
                    i as u16,
                    true,
                    distinct_digest(i * 2),
                    distinct_digest(i * 2 + 1),
                )
            })
            .collect();
        let trace = generate_column_meta_trace(&metas);
        debug_check(&ColumnMetaChip, &trace).expect("many cols same table should pass");
    }

    // ── M8-5: Touched consistency + empty transition tests ──

    #[test]
    fn valid_untouched_empty_preserved() {
        // Untouched column: is_empty_old = is_empty_new (both true)
        let d1 = distinct_digest(1);
        let metas = vec![ColumnMeta {
            table: TableId(0),
            col: ColId(0),
            tag: CommitmentStrategy::Ssmc,
            com_old: d1,
            com_new: d1,
            is_empty_old: true,
            is_empty_new: true,
            is_touched: false,
        }];
        let trace = generate_column_meta_trace(&metas);
        debug_check(&ColumnMetaChip, &trace).expect("untouched empty preserved");
    }

    #[test]
    fn valid_empty_to_nonempty_transition() {
        // Touched column: was empty, now non-empty
        let d_empty = NativeDigest([BabyBear::ZERO; 8]);
        let d_new = distinct_digest(1);
        let metas = vec![ColumnMeta {
            table: TableId(0),
            col: ColId(0),
            tag: CommitmentStrategy::Ssmc,
            com_old: d_empty,
            com_new: d_new,
            is_empty_old: true,
            is_empty_new: false,
            is_touched: true,
        }];
        let trace = generate_column_meta_trace(&metas);
        debug_check(&ColumnMetaChip, &trace).expect("empty→non-empty transition");
    }

    #[test]
    fn valid_nonempty_stays_nonempty() {
        // Touched, was non-empty, stays non-empty
        let d1 = distinct_digest(1);
        let d2 = distinct_digest(2);
        let metas = vec![ColumnMeta {
            table: TableId(0),
            col: ColId(0),
            tag: CommitmentStrategy::Ssmc,
            com_old: d1,
            com_new: d2,
            is_empty_old: false,
            is_empty_new: false,
            is_touched: true,
        }];
        let trace = generate_column_meta_trace(&metas);
        debug_check(&ColumnMetaChip, &trace).expect("non-empty stays non-empty");
    }

    #[test]
    fn invalid_untouched_empty_changed() {
        // Untouched but is_empty_new ≠ is_empty_old → should fail constraint 5
        let d1 = distinct_digest(1);
        let metas = vec![ColumnMeta {
            table: TableId(0),
            col: ColId(0),
            tag: CommitmentStrategy::Ssmc,
            com_old: d1,
            com_new: d1,
            is_empty_old: true,
            is_empty_new: false, // changed despite untouched!
            is_touched: false,
        }];
        let trace = generate_column_meta_trace(&metas);
        debug_check(&ColumnMetaChip, &trace).expect_err("untouched but empty flag changed");
    }

    #[test]
    fn invalid_empty_stays_empty_when_touched() {
        // is_empty_old=1 ∧ is_touched=1 ∧ is_empty_new=1 → should fail constraint 6
        let d1 = distinct_digest(1);
        let d2 = distinct_digest(2);
        let metas = vec![ColumnMeta {
            table: TableId(0),
            col: ColId(0),
            tag: CommitmentStrategy::Ssmc,
            com_old: d1,
            com_new: d2,
            is_empty_old: true,
            is_empty_new: true, // should be 0 since touched + was empty
            is_touched: true,
        }];
        let trace = generate_column_meta_trace(&metas);
        debug_check(&ColumnMetaChip, &trace)
            .expect_err("empty_old=1 ∧ touched=1 ⟹ empty_new must be 0");
    }

    #[test]
    fn invalid_is_zero_soundness_forged_table_diff() {
        // Construct a valid trace, then manually corrupt the IsZero witness
        // to claim table_diff is zero when it's not.
        let d1 = distinct_digest(1);
        let d2 = distinct_digest(2);
        let d3 = distinct_digest(3);
        let d4 = distinct_digest(4);
        let metas = vec![meta(0, 0, true, d1, d2), meta(1, 0, true, d3, d4)];
        let mut trace = generate_column_meta_trace(&metas);

        // Row 0's table_diff_iz: table_diff = 1 (nonzero), is_zero should be 0.
        // Forge: set is_zero = 1 (claiming they're the same table).
        // Column layout: is_real(0), table_id(1), col_id(2), tag(3),
        //   com_old[8](4-11), com_new[8](12-19),
        //   is_empty_old(20), is_empty_new(21), is_touched(22),
        //   table_diff_iz.inv(23), table_diff_iz.is_zero(24),
        //   col_diff_iz.inv(25), col_diff_iz.is_zero(26)
        let table_diff_iz_is_zero_offset = 24;
        trace.values[table_diff_iz_is_zero_offset] = BabyBear::ONE;
        debug_check(&ColumnMetaChip, &trace)
            .expect_err("forged is_zero should fail IsZero constraint (val*is_zero!=0)");
    }
}
