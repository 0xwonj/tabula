//! Trace generation for the GlobalSortedMem chip.
//!
//! Converts witness data (ColumnWitness init/access rows) into a
//! `RowMajorMatrix<BabyBear>` trace.

use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;
use p3_matrix::dense::RowMajorMatrix;

use crate::air::columns::borrow_cols_mut;
use crate::air::gadgets::bool_fe;
use crate::air::gadgets::integer::MASK_30;

use super::columns::{GlobalSortedMemCols, sorted_mem_width};

/// A flat row for building the sorted memory trace.
///
/// This is the input format — one entry per init or access row,
/// pre-sorted by `(table_id, col_id, row_key, timestamp)`.
pub struct SortedMemRow {
    /// Table identifier.
    pub table_id: u32,
    /// Column identifier.
    pub col_id: u16,
    /// Row key (u64).
    pub row_key: u64,
    /// Timestamp (u64). 0 for init rows, clk+1 for access rows.
    pub timestamp: u64,
    /// Init row flag.
    pub is_init: bool,
    /// Write flag.
    pub is_write: bool,
    /// Value field elements (length must equal W).
    pub val: Vec<BabyBear>,
    /// Value null flag.
    pub val_is_null: bool,
}

/// Generate a GlobalSortedMem trace from pre-sorted witness rows.
///
/// `rows` must be sorted by `(table_id, col_id, row_key, timestamp)`.
/// Padding rows have `is_real = 0`.
pub fn generate_sorted_mem_trace<const W: usize>(
    rows: &[SortedMemRow],
) -> RowMajorMatrix<BabyBear> {
    debug_assert!(
        rows.windows(2).all(|w| {
            (w[0].table_id, w[0].col_id, w[0].row_key, w[0].timestamp)
                <= (w[1].table_id, w[1].col_id, w[1].row_key, w[1].timestamp)
        }),
        "rows must be sorted by (table_id, col_id, row_key, timestamp)"
    );

    let width = sorted_mem_width::<W>();
    let num_real = rows.len();
    let num_rows = (num_real + 1).next_power_of_two().max(2);
    let mut values = vec![BabyBear::ZERO; num_rows * width];

    // Running memory state per key: (t, c, r) -> (mem[W], mem_is_null)
    // We track this using the running memory columns.
    let mut running_mem: Vec<BabyBear> = vec![BabyBear::ZERO; W];
    let mut running_mem_is_null = BabyBear::ZERO;
    let mut has_written = false;

    for (i, row) in rows.iter().enumerate() {
        assert_eq!(
            row.val.len(),
            W,
            "val length mismatch: expected {W}, got {}",
            row.val.len()
        );

        let offset = i * width;
        let cols: &mut GlobalSortedMemCols<BabyBear, W> =
            borrow_cols_mut(&mut values[offset..offset + width]);

        cols.is_real = BabyBear::ONE;
        cols.table_id = BabyBear::new(row.table_id);
        cols.col_id = BabyBear::new(row.col_id as u32);
        cols.r.populate(row.row_key);
        cols.tau.populate(row.timestamp);
        cols.is_init = bool_fe(row.is_init);
        cols.is_write = bool_fe(row.is_write);
        for (j, v) in row.val.iter().enumerate() {
            cols.val[j] = *v;
        }
        cols.val_is_null = bool_fe(row.val_is_null);

        // Determine if this is the start of a new key.
        let is_new_key = if i == 0 {
            true
        } else {
            let prev = &rows[i - 1];
            row.table_id != prev.table_id
                || row.col_id != prev.col_id
                || row.row_key != prev.row_key
        };

        // Memory state
        if is_new_key {
            // Init row: mem = val
            running_mem.copy_from_slice(&row.val);
            running_mem_is_null = cols.val_is_null;
            has_written = false;
        } else if row.is_write {
            // Write: update running memory
            running_mem.copy_from_slice(&row.val);
            running_mem_is_null = cols.val_is_null;
            has_written = true;
        } else {
            // Read: running memory unchanged (already set by witness).
        }

        for (j, m) in running_mem.iter().enumerate() {
            cols.mem[j] = *m;
        }
        cols.mem_is_null = running_mem_is_null;
        cols.has_written = bool_fe(has_written);

        // Look ahead for is_last_for_key and same-key detection.
        let next_is_different_key = if i + 1 < num_real {
            let nxt = &rows[i + 1];
            row.table_id != nxt.table_id || row.col_id != nxt.col_id || row.row_key != nxt.row_key
        } else {
            true // Last real row: always the last for its key
        };
        cols.is_last_for_key = bool_fe(next_is_different_key);

        // tc_changed and r_changed: compare with next row
        if i + 1 < num_real {
            let nxt = &rows[i + 1];
            let tc_changed = row.table_id != nxt.table_id || row.col_id != nxt.col_id;
            let r_changed = tc_changed || row.row_key != nxt.row_key;
            cols.tc_changed = bool_fe(tc_changed);
            cols.r_changed = bool_fe(r_changed);
        } else {
            // Last real row: next is padding (all zeros), so everything changes.
            cols.tc_changed = BabyBear::ONE;
            cols.r_changed = BabyBear::ONE;
        }
    }

    populate_ordering_witnesses::<W>(rows, num_real, num_rows, width, &mut values);

    RowMajorMatrix::new(values, width)
}

/// Populate IsZero and StrictIneq ordering witnesses (second pass).
///
/// This needs both current and next row data, so it runs as a separate pass
/// after the primary row fields have been filled. Source data is read from
/// `rows` (not from `values`) to avoid layout-dependent offsets.
fn populate_ordering_witnesses<const W: usize>(
    rows: &[SortedMemRow],
    num_real: usize,
    num_rows: usize,
    width: usize,
    values: &mut [BabyBear],
) {
    let shift_30 = BabyBear::new(1 << 30);
    let shift_60 = shift_30 * shift_30;
    let encode_r = |r: u64| -> BabyBear {
        let l0 = BabyBear::new((r & MASK_30) as u32);
        let l1 = BabyBear::new(((r >> 30) & MASK_30) as u32);
        let l2 = BabyBear::new((r >> 60) as u32);
        l0 + l1 * shift_30 + l2 * shift_60
    };

    for i in 0..num_rows {
        let next_idx = (i + 1) % num_rows;

        // Read fields from source rows (padding rows are all-zero).
        let (cur_table, cur_col, cur_r) = if i < num_real {
            let r = &rows[i];
            (
                BabyBear::new(r.table_id),
                BabyBear::new(r.col_id as u32),
                r.row_key,
            )
        } else {
            (BabyBear::ZERO, BabyBear::ZERO, 0u64)
        };
        let (next_table, next_col, next_r) = if next_idx < num_real {
            let r = &rows[next_idx];
            (
                BabyBear::new(r.table_id),
                BabyBear::new(r.col_id as u32),
                r.row_key,
            )
        } else {
            (BabyBear::ZERO, BabyBear::ZERO, 0u64)
        };

        let table_diff = next_table - cur_table;
        let col_diff = next_col - cur_col;
        let r_combined_diff = encode_r(next_r) - encode_r(cur_r);

        // Write IsZero witnesses.
        let cur_offset = i * width;
        let cols: &mut GlobalSortedMemCols<BabyBear, W> =
            borrow_cols_mut(&mut values[cur_offset..cur_offset + width]);
        cols.table_diff_iz.populate(table_diff);
        cols.col_diff_iz.populate(col_diff);
        cols.r_diff_iz.populate(r_combined_diff);

        // StrictIneq (ordering) witness: only meaningful for real→real transitions.
        let is_real_cur = i < num_real;
        let is_real_next = next_idx < num_real;
        if is_real_cur && is_real_next {
            let cur_row = &rows[i];
            let next_row = &rows[next_idx];
            let tc_changed =
                cur_row.table_id != next_row.table_id || cur_row.col_id != next_row.col_id;
            let r_changed = tc_changed || cur_row.row_key != next_row.row_key;

            if !r_changed {
                // Same key: ordering on timestamp.
                cols.ordering
                    .populate(cur_row.timestamp, next_row.timestamp);
            } else if !tc_changed {
                // Same (t,c), different r: ordering on row key.
                cols.ordering.populate(cur_row.row_key, next_row.row_key);
            }
            // Cross-(t,c) transitions: ordering gated off in AIR, leave as zeros.
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::air::debug::debug_check;

    use super::super::air::GlobalSortedMemChip;

    fn init_row(t: u32, c: u16, r: u64, val: [u32; 3], is_null: bool) -> SortedMemRow {
        SortedMemRow {
            table_id: t,
            col_id: c,
            row_key: r,
            timestamp: 0,
            is_init: true,
            is_write: false,
            val: val.iter().map(|v| BabyBear::new(*v)).collect(),
            val_is_null: is_null,
        }
    }

    fn read_row(t: u32, c: u16, r: u64, tau: u64, val: [u32; 3], is_null: bool) -> SortedMemRow {
        SortedMemRow {
            table_id: t,
            col_id: c,
            row_key: r,
            timestamp: tau,
            is_init: false,
            is_write: false,
            val: val.iter().map(|v| BabyBear::new(*v)).collect(),
            val_is_null: is_null,
        }
    }

    fn write_row(t: u32, c: u16, r: u64, tau: u64, val: [u32; 3], is_null: bool) -> SortedMemRow {
        SortedMemRow {
            table_id: t,
            col_id: c,
            row_key: r,
            timestamp: tau,
            is_init: false,
            is_write: true,
            val: val.iter().map(|v| BabyBear::new(*v)).collect(),
            val_is_null: is_null,
        }
    }

    // ── Valid traces ──

    #[test]
    fn valid_single_init_only() {
        // Single init row: read-only key with no accesses.
        let rows = vec![init_row(0, 0, 100, [1, 2, 3], false)];
        let trace = generate_sorted_mem_trace::<3>(&rows);
        debug_check(&GlobalSortedMemChip::<3>, &trace).expect("single init should pass");
    }

    #[test]
    fn valid_init_then_read() {
        // Init row followed by a read.
        let rows = vec![
            init_row(0, 0, 100, [1, 2, 3], false),
            read_row(0, 0, 100, 1, [1, 2, 3], false),
        ];
        let trace = generate_sorted_mem_trace::<3>(&rows);
        debug_check(&GlobalSortedMemChip::<3>, &trace).expect("init+read should pass");
    }

    #[test]
    fn valid_init_read_write_read() {
        // Init, read, write, read (reads after write see new value).
        let rows = vec![
            init_row(0, 0, 100, [1, 2, 3], false),
            read_row(0, 0, 100, 1, [1, 2, 3], false),
            write_row(0, 0, 100, 2, [4, 5, 6], false),
            read_row(0, 0, 100, 3, [4, 5, 6], false),
        ];
        let trace = generate_sorted_mem_trace::<3>(&rows);
        debug_check(&GlobalSortedMemChip::<3>, &trace).expect("init+read+write+read should pass");
    }

    #[test]
    fn valid_two_keys_same_column() {
        // Two different row keys in the same (t,c).
        let rows = vec![
            init_row(0, 0, 10, [1, 0, 0], false),
            read_row(0, 0, 10, 1, [1, 0, 0], false),
            init_row(0, 0, 20, [2, 0, 0], false),
            read_row(0, 0, 20, 1, [2, 0, 0], false),
        ];
        let trace = generate_sorted_mem_trace::<3>(&rows);
        debug_check(&GlobalSortedMemChip::<3>, &trace).expect("two keys same col should pass");
    }

    #[test]
    fn valid_null_init_and_write() {
        // Init with null, then write non-null.
        let rows = vec![
            init_row(0, 0, 100, [0, 0, 0], true),
            write_row(0, 0, 100, 1, [1, 2, 3], false),
        ];
        let trace = generate_sorted_mem_trace::<3>(&rows);
        debug_check(&GlobalSortedMemChip::<3>, &trace).expect("null init + write should pass");
    }

    #[test]
    fn valid_two_segments_different_tc() {
        // Two different (t,c) segments: r can decrease across segment boundary.
        let rows = vec![
            init_row(0, 0, 100, [1, 0, 0], false),
            read_row(0, 0, 100, 1, [1, 0, 0], false),
            init_row(0, 1, 10, [2, 0, 0], false), // r=10 < previous r=100
            read_row(0, 1, 10, 1, [2, 0, 0], false),
        ];
        let trace = generate_sorted_mem_trace::<3>(&rows);
        debug_check(&GlobalSortedMemChip::<3>, &trace)
            .expect("cross-(t,c) with decreasing r should pass");
    }

    #[test]
    fn valid_all_padding() {
        let rows: Vec<SortedMemRow> = vec![];
        let trace = generate_sorted_mem_trace::<3>(&rows);
        debug_check(&GlobalSortedMemChip::<3>, &trace).expect("all-padding should pass");
    }

    #[test]
    fn valid_write_set_extraction() {
        // Verify has_written is correctly set.
        let rows = vec![
            init_row(0, 0, 100, [1, 2, 3], false),
            read_row(0, 0, 100, 1, [1, 2, 3], false), // has_written = 0
            write_row(0, 0, 100, 2, [4, 5, 6], false), // has_written = 1
            read_row(0, 0, 100, 3, [4, 5, 6], false), // has_written = 1
        ];
        let trace = generate_sorted_mem_trace::<3>(&rows);
        debug_check(&GlobalSortedMemChip::<3>, &trace).expect("write-set should pass");
    }

    // ── Invalid traces ──

    #[test]
    fn invalid_missing_init() {
        // Access without init row should fail.
        let mut rows = vec![read_row(0, 0, 100, 1, [1, 2, 3], false)];
        rows[0].is_init = false; // force no init
        let trace = generate_sorted_mem_trace::<3>(&rows);
        debug_check(&GlobalSortedMemChip::<3>, &trace)
            .expect_err("missing init should fail (first row must be init)");
    }

    #[test]
    fn invalid_init_with_nonzero_tau() {
        // Init row with tau != 0.
        let mut rows = vec![init_row(0, 0, 100, [1, 2, 3], false)];
        rows[0].timestamp = 5; // nonzero tau for init
        let trace = generate_sorted_mem_trace::<3>(&rows);
        debug_check(&GlobalSortedMemChip::<3>, &trace)
            .expect_err("init with nonzero tau should fail");
    }

    #[test]
    fn invalid_init_with_write() {
        // Init row with is_write = 1.
        let mut rows = vec![init_row(0, 0, 100, [1, 2, 3], false)];
        rows[0].is_write = true;
        let trace = generate_sorted_mem_trace::<3>(&rows);
        debug_check(&GlobalSortedMemChip::<3>, &trace)
            .expect_err("init with is_write=1 should fail");
    }

    #[test]
    fn invalid_read_wrong_value() {
        // Read returns a value different from memory.
        let rows = vec![
            init_row(0, 0, 100, [1, 2, 3], false),
            SortedMemRow {
                table_id: 0,
                col_id: 0,
                row_key: 100,
                timestamp: 1,
                is_init: false,
                is_write: false,
                val: vec![BabyBear::new(999), BabyBear::new(2), BabyBear::new(3)],
                val_is_null: false,
            },
        ];
        let trace = generate_sorted_mem_trace::<3>(&rows);
        debug_check(&GlobalSortedMemChip::<3>, &trace)
            .expect_err("read with wrong value should fail");
    }

    #[test]
    fn invalid_null_canon_violation() {
        // val_is_null=1 but val != 0.
        let rows = vec![init_row(0, 0, 100, [1, 2, 3], true)];
        // The trace generator will set val = [1,2,3] and val_is_null = 1.
        // This violates null canonicality.
        let trace = generate_sorted_mem_trace::<3>(&rows);
        debug_check(&GlobalSortedMemChip::<3>, &trace)
            .expect_err("null canon violation should fail");
    }

    // ── Ordering violation tests ──

    #[test]
    fn invalid_tau_regression() {
        // Valid trace: init(tau=0) → read(tau=1) → read(tau=2), same key.
        // Corrupt: swap tau of rows 1 and 2, creating tau=2 → tau=1 regression.
        use crate::air::chips::sorted_mem::columns::SORTED_MEM_STANDARD_WIDTH;
        use crate::air::columns::{borrow_cols, borrow_cols_mut};

        let rows = vec![
            init_row(0, 0, 100, [1, 2, 3], false),
            read_row(0, 0, 100, 1, [1, 2, 3], false),
            read_row(0, 0, 100, 2, [1, 2, 3], false),
        ];
        let mut trace = generate_sorted_mem_trace::<3>(&rows);
        let width = SORTED_MEM_STANDARD_WIDTH;

        // Save tau from row 1 (tau=1) and row 2 (tau=2).
        let (tau1_l0, tau1_l1, tau1_l2) = {
            let cols: &GlobalSortedMemCols<BabyBear, 3> =
                borrow_cols(&trace.values[width..2 * width]);
            (cols.tau.limb0, cols.tau.limb1, cols.tau.limb2)
        };
        let (tau2_l0, tau2_l1, tau2_l2) = {
            let cols: &GlobalSortedMemCols<BabyBear, 3> =
                borrow_cols(&trace.values[2 * width..3 * width]);
            (cols.tau.limb0, cols.tau.limb1, cols.tau.limb2)
        };

        // Swap: row 1 gets tau=2, row 2 gets tau=1.
        {
            let cols: &mut GlobalSortedMemCols<BabyBear, 3> =
                borrow_cols_mut(&mut trace.values[width..2 * width]);
            cols.tau.limb0 = tau2_l0;
            cols.tau.limb1 = tau2_l1;
            cols.tau.limb2 = tau2_l2;
        }
        {
            let cols: &mut GlobalSortedMemCols<BabyBear, 3> =
                borrow_cols_mut(&mut trace.values[2 * width..3 * width]);
            cols.tau.limb0 = tau1_l0;
            cols.tau.limb1 = tau1_l1;
            cols.tau.limb2 = tau1_l2;
        }

        debug_check(&GlobalSortedMemChip::<3>, &trace)
            .expect_err("tau regression (2 → 1) should fail ordering constraint");
    }

    #[test]
    fn invalid_ordering_witness_corrupted() {
        // Valid data ordering (r=10 < r=20), but corrupt the StrictIneq gap witness.
        // The AIR constraint gap = next_r - cur_r - 1 will not hold.
        use crate::air::chips::sorted_mem::columns::SORTED_MEM_STANDARD_WIDTH;
        use crate::air::columns::borrow_cols_mut;

        let rows = vec![
            init_row(0, 0, 10, [1, 0, 0], false),
            init_row(0, 0, 20, [2, 0, 0], false),
        ];
        let mut trace = generate_sorted_mem_trace::<3>(&rows);
        let width = SORTED_MEM_STANDARD_WIDTH;

        // Corrupt the ordering gap witness for row 0 (should prove r=10 < r=20).
        {
            let cols: &mut GlobalSortedMemCols<BabyBear, 3> =
                borrow_cols_mut(&mut trace.values[0..width]);
            cols.ordering.diff0 = BabyBear::new(999);
        }

        debug_check(&GlobalSortedMemChip::<3>, &trace)
            .expect_err("corrupted ordering gap should fail");
    }
}
