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

    // Running memory state per key.
    let mut running_mem: Vec<BabyBear> = vec![BabyBear::ZERO; W];
    let mut running_mem_is_null = BabyBear::ZERO;
    let mut has_written = false;
    let mut current_meta_is_empty_old = false;

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

        // Segment metadata.
        let is_first_of_segment = if i == 0 {
            true
        } else {
            let prev = &rows[i - 1];
            row.table_id != prev.table_id || row.col_id != prev.col_id
        };
        cols.is_first_of_segment = bool_fe(is_first_of_segment);
        if is_first_of_segment {
            current_meta_is_empty_old = row.meta_is_empty_old;
        }
        cols.meta_is_empty_old = bool_fe(current_meta_is_empty_old);

        // Key change detection.
        let is_new_key = if i == 0 {
            true
        } else {
            let prev = &rows[i - 1];
            row.table_id != prev.table_id
                || row.col_id != prev.col_id
                || row.row_key != prev.row_key
        };

        // Memory state.
        if is_new_key {
            running_mem.copy_from_slice(&row.val);
            running_mem_is_null = cols.val_is_null;
            has_written = false;
        } else if row.is_write {
            running_mem.copy_from_slice(&row.val);
            running_mem_is_null = cols.val_is_null;
            has_written = true;
        }

        for (j, m) in running_mem.iter().enumerate() {
            cols.mem[j] = *m;
        }
        cols.mem_is_null = running_mem_is_null;
        cols.has_written = bool_fe(has_written);

        // Look ahead for is_last_for_key.
        let next_is_different_key = if i + 1 < num_real {
            let nxt = &rows[i + 1];
            row.table_id != nxt.table_id || row.col_id != nxt.col_id || row.row_key != nxt.row_key
        } else {
            true
        };
        cols.is_last_for_key = bool_fe(next_is_different_key);

        // r_changed: compare with next row.
        if i + 1 < num_real {
            let nxt = &rows[i + 1];
            let tc_changed = row.table_id != nxt.table_id || row.col_id != nxt.col_id;
            let r_changed = tc_changed || row.row_key != nxt.row_key;
            cols.r_changed = bool_fe(r_changed);
        } else {
            cols.r_changed = BabyBear::ONE;
        }
    }

    populate_ordering_witnesses::<W>(rows, num_real, num_rows, width, &mut values);

    RowMajorMatrix::new(values, width)
}

/// Populate ordering witnesses (second pass over all rows including padding).
fn populate_ordering_witnesses<const W: usize>(
    rows: &[SortedMemRow],
    num_real: usize,
    num_rows: usize,
    width: usize,
    values: &mut [BabyBear],
) {
    for i in 0..num_rows {
        let next_idx = (i + 1) % num_rows;

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

        // Per-limb row key diffs for IsZero gadgets.
        let cur_r_l0 = BabyBear::new((cur_r & MASK_30) as u32);
        let cur_r_l1 = BabyBear::new(((cur_r >> 30) & MASK_30) as u32);
        let cur_r_l2 = BabyBear::new((cur_r >> 60) as u32);
        let next_r_l0 = BabyBear::new((next_r & MASK_30) as u32);
        let next_r_l1 = BabyBear::new(((next_r >> 30) & MASK_30) as u32);
        let next_r_l2 = BabyBear::new((next_r >> 60) as u32);

        let cur_offset = i * width;
        let cols: &mut GlobalSortedMemCols<BabyBear, W> =
            borrow_cols_mut(&mut values[cur_offset..cur_offset + width]);

        // Same-key (t,c) detection.
        cols.segment.populate(table_diff, col_diff);

        // Per-limb r key diff IsZero.
        cols.r_limb0_diff_iz.populate(next_r_l0 - cur_r_l0);
        cols.r_limb1_diff_iz.populate(next_r_l1 - cur_r_l1);
        cols.r_limb2_diff_iz.populate(next_r_l2 - cur_r_l2);

        let is_real_cur = i < num_real;
        let is_real_next = next_idx < num_real;

        if is_real_cur && is_real_next {
            let cur_row = &rows[i];
            let next_row = &rows[next_idx];
            let tc_changed =
                cur_row.table_id != next_row.table_id || cur_row.col_id != next_row.col_id;

            // Lex ordering direction at segment boundaries.
            cols.lex.populate(
                cur_row.table_id,
                next_row.table_id,
                cur_row.col_id as u32,
                next_row.col_id as u32,
                tc_changed,
            );

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
