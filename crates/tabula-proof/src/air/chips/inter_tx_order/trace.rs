//! Trace generation for the InterTxOrder chip.
//!
//! Converts witness data (sorted rows per key per tx) into a
//! `RowMajorMatrix<BabyBear>` trace.

use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;
use p3_matrix::dense::RowMajorMatrix;

use crate::air::columns::borrow_cols_mut;
use crate::air::gadgets::bool_fe;

use super::columns::{InterTxOrderCols, inter_tx_order_width};

/// A single row for building the InterTxOrder trace.
///
/// Pre-sorted by `(table_id, col_id, key, tx_index)`.
/// Init rows have `is_init=true` and appear first for each key.
#[derive(Debug, Clone)]
pub struct InterTxOrderRow {
    /// Table identifier.
    pub table_id: u32,
    /// Column identifier.
    pub col_id: u16,
    /// Row key (u64).
    pub key: u64,
    /// Transaction index within the batch (0 for init rows).
    pub tx_index: u32,
    /// True if this is an init row (base state seed).
    pub is_init: bool,
    /// True if this tx read the key.
    pub has_read: bool,
    /// True if this tx wrote the key.
    pub has_write: bool,
    /// Input value (base state for init; previous output for access).
    pub input_val: Vec<BabyBear>,
    /// Input is-null flag.
    pub input_is_null: bool,
    /// Output value (same as input for init/read-only; written value for write).
    pub output_val: Vec<BabyBear>,
    /// Output is-null flag.
    pub output_is_null: bool,
}

/// Generate an InterTxOrder trace from pre-sorted rows.
///
/// `rows` must be sorted by `(table_id, col_id, key, tx_index)`.
/// Each key must have exactly one init row first, followed by access rows
/// with strictly increasing tx_index.
pub fn generate_inter_tx_order_trace<const W: usize>(
    rows: &[InterTxOrderRow],
) -> RowMajorMatrix<BabyBear> {
    let width = inter_tx_order_width::<W>();
    let num_real = rows.len();
    let num_rows = (num_real + 1).next_power_of_two().max(2);
    let mut values = vec![BabyBear::ZERO; num_rows * width];

    // ── Pass 1: populate identity, key, values, flags ──
    for (i, row) in rows.iter().enumerate() {
        assert_eq!(row.input_val.len(), W, "input_val length mismatch");
        assert_eq!(row.output_val.len(), W, "output_val length mismatch");

        let offset = i * width;
        let cols: &mut InterTxOrderCols<BabyBear, W> =
            borrow_cols_mut(&mut values[offset..offset + width]);

        cols.is_real = BabyBear::ONE;
        cols.table_id = BabyBear::new(row.table_id);
        cols.col_id = BabyBear::new(row.col_id as u32);
        cols.key.populate(row.key);
        cols.tx_index = BabyBear::new(row.tx_index);
        cols.is_init = bool_fe(row.is_init);
        cols.has_read = bool_fe(row.has_read);
        cols.has_write = bool_fe(row.has_write);

        for (j, v) in row.input_val.iter().enumerate() {
            cols.input_val[j] = *v;
        }
        cols.input_is_null = bool_fe(row.input_is_null);

        for (j, v) in row.output_val.iter().enumerate() {
            cols.output_val[j] = *v;
        }
        cols.output_is_null = bool_fe(row.output_is_null);
    }

    // ── Pass 2: chain tracking (is_last_for_key, has_ever_written) ──
    populate_chain_tracking::<W>(rows, num_real, width, &mut values);

    // ── Pass 3: ordering witnesses, segment detection, lex, tx_diff ──
    populate_ordering_witnesses::<W>(rows, num_real, num_rows, width, &mut values);

    RowMajorMatrix::new(values, width)
}

/// Compute chain tracking flags using look-ahead.
fn populate_chain_tracking<const W: usize>(
    rows: &[InterTxOrderRow],
    num_real: usize,
    width: usize,
    values: &mut [BabyBear],
) {
    // has_ever_written: forward scan within key chains
    let mut ever_written = false;
    for i in 0..num_real {
        let row = &rows[i];

        // Detect key change
        let is_new_key = if i == 0 {
            true
        } else {
            let prev = &rows[i - 1];
            row.table_id != prev.table_id || row.col_id != prev.col_id || row.key != prev.key
        };
        if is_new_key {
            ever_written = false;
        }

        if row.has_write {
            ever_written = true;
        }

        let offset = i * width;
        let cols: &mut InterTxOrderCols<BabyBear, W> =
            borrow_cols_mut(&mut values[offset..offset + width]);
        cols.has_ever_written = bool_fe(ever_written);
    }

    // is_last_for_key: backward scan
    for i in 0..num_real {
        let is_last = if i + 1 >= num_real {
            true
        } else {
            let next = &rows[i + 1];
            rows[i].table_id != next.table_id
                || rows[i].col_id != next.col_id
                || rows[i].key != next.key
        };

        let offset = i * width;
        let cols: &mut InterTxOrderCols<BabyBear, W> =
            borrow_cols_mut(&mut values[offset..offset + width]);
        cols.is_last_for_key = bool_fe(is_last);
    }
}

/// Populate ordering witnesses: segment detection, lex, key ordering, tx_diff.
///
/// Iterates ALL rows including padding — IsZero gadgets are unconditionally
/// constrained in the AIR and must have valid witnesses everywhere.
fn populate_ordering_witnesses<const W: usize>(
    rows: &[InterTxOrderRow],
    num_real: usize,
    num_rows: usize,
    width: usize,
    values: &mut [BabyBear],
) {
    for i in 0..num_rows {
        let next_idx = (i + 1) % num_rows;

        let (cur_table, cur_col, cur_key) = if i < num_real {
            (rows[i].table_id, rows[i].col_id as u32, rows[i].key)
        } else {
            (0, 0, 0)
        };
        let (next_table, next_col, next_key) = if next_idx < num_real {
            (
                rows[next_idx].table_id,
                rows[next_idx].col_id as u32,
                rows[next_idx].key,
            )
        } else {
            (0, 0, 0)
        };

        let table_diff = BabyBear::new(next_table) - BabyBear::new(cur_table);
        let col_diff = BabyBear::new(next_col) - BabyBear::new(cur_col);

        let cur_offset = i * width;
        let cols: &mut InterTxOrderCols<BabyBear, W> =
            borrow_cols_mut(&mut values[cur_offset..cur_offset + width]);

        // Same-tc detection
        cols.same_tc.populate(table_diff, col_diff);

        // Key limb IsZero gadgets
        let limb0_diff = BabyBear::new((next_key & 0x3FFFFFFF) as u32)
            - BabyBear::new((cur_key & 0x3FFFFFFF) as u32);
        let limb1_diff = BabyBear::new(((next_key >> 30) & 0x3FFFFFFF) as u32)
            - BabyBear::new(((cur_key >> 30) & 0x3FFFFFFF) as u32);
        let limb2_diff =
            BabyBear::new((next_key >> 60) as u32) - BabyBear::new((cur_key >> 60) as u32);
        cols.r_limb0_iz.populate(limb0_diff);
        cols.r_limb1_iz.populate(limb1_diff);
        cols.r_limb2_iz.populate(limb2_diff);

        let is_real_cur = i < num_real;
        let is_real_next = next_idx < num_real;

        if is_real_cur && is_real_next {
            let tc_changed = cur_table != next_table || cur_col != next_col;
            let same_key_bool = !tc_changed && cur_key == next_key;

            cols.lex_dir
                .populate(cur_table, next_table, cur_col, next_col, tc_changed);

            if !tc_changed && !same_key_bool {
                // Different key in same segment → ordering
                cols.key_ordering.populate(cur_key, next_key);
            }

            if same_key_bool && !rows[i].is_init && !rows[next_idx].is_init {
                // tx_diff between consecutive access rows
                let cur_tx = rows[i].tx_index;
                let next_tx = rows[next_idx].tx_index;
                debug_assert!(
                    next_tx > cur_tx,
                    "tx_index must strictly increase: {} -> {}",
                    cur_tx,
                    next_tx
                );
                let diff = next_tx - cur_tx - 1;
                cols.tx_diff = BabyBear::new(diff);
            }
        }
    }
}
