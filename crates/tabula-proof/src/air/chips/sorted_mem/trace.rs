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
    /// Whether the column was empty in the old state (for SortedMemMeta bus).
    /// Only meaningful for first-of-segment rows; ignored otherwise.
    pub meta_is_empty_old: bool,
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

        // Half-decomposition for range checks
        let r_l0 = (row.row_key & MASK_30) as u32;
        let r_l1 = ((row.row_key >> 30) & MASK_30) as u32;
        cols.r_l0_halves.populate(r_l0);
        cols.r_l1_halves.populate(r_l1);
        let tau_l0 = (row.timestamp & MASK_30) as u32;
        let tau_l1 = ((row.timestamp >> 30) & MASK_30) as u32;
        cols.tau_l0_halves.populate(tau_l0);
        cols.tau_l1_halves.populate(tau_l1);
        cols.is_init = bool_fe(row.is_init);
        cols.is_write = bool_fe(row.is_write);
        for (j, v) in row.val.iter().enumerate() {
            cols.val[j] = *v;
        }
        cols.val_is_null = bool_fe(row.val_is_null);

        // Determine if this is the start of a new (t,c) segment.
        let is_first_of_segment = if i == 0 {
            true
        } else {
            let prev = &rows[i - 1];
            row.table_id != prev.table_id || row.col_id != prev.col_id
        };
        cols.is_first_of_segment = bool_fe(is_first_of_segment);
        if is_first_of_segment {
            cols.meta_is_empty_old = bool_fe(row.meta_is_empty_old);
        }

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
