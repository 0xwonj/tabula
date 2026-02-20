//! Trace generation for the GlobalMerge chip.
//!
//! Converts witness data (MergeTrace entries per column) into a
//! `RowMajorMatrix<BabyBear>` trace.

use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;
use p3_matrix::dense::RowMajorMatrix;

use crate::air::columns::borrow_cols_mut;
use crate::air::gadgets::bool_fe;

use super::columns::{GlobalMergeCols, merge_width};

/// Source type for a merge row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MergeSource {
    /// Key exists only in OldList: (s1,s0) = (0,0).
    OldOnly,
    /// Key exists only in WriteSet: (s1,s0) = (0,1).
    WriteOnly,
    /// Key exists in both OldList and WriteSet: (s1,s0) = (1,0).
    Both,
    /// Key deleted (write null): (s1,s0) = (1,1).
    Delete,
}

impl MergeSource {
    /// Encode as (s1, s0) pair.
    fn encode(self) -> (bool, bool) {
        match self {
            Self::OldOnly => (false, false),
            Self::WriteOnly => (false, true),
            Self::Both => (true, false),
            Self::Delete => (true, true),
        }
    }
}

/// A flat row for building the merge trace.
///
/// One entry per merge step, pre-sorted by `(table_id, col_id, key)`.
pub struct MergeRow {
    /// Table identifier.
    pub table_id: u32,
    /// Column identifier.
    pub col_id: u16,
    /// Row key (u64).
    pub key: u64,
    /// Source type (old_only, write_only, both, delete).
    pub source: MergeSource,
    /// Old value from OldList (zeros if write_only).
    pub old_val: Vec<BabyBear>,
    /// Write value from WriteSet (zeros if old_only, canonical null if delete).
    pub write_val: Vec<BabyBear>,
    /// New value for NewList (result of merge).
    pub new_val: Vec<BabyBear>,
    /// True if entry is in NewList.
    pub in_new: bool,
    /// Running Poseidon hash chain accumulator (precomputed).
    pub hash_acc: [BabyBear; 8],
}

/// Generate a GlobalMerge trace from pre-sorted merge rows.
///
/// `rows` must be sorted by `(table_id, col_id, key)`.
/// Keys must be strictly increasing within each `(table_id, col_id)` segment.
pub fn generate_merge_trace<const W: usize>(rows: &[MergeRow]) -> RowMajorMatrix<BabyBear> {
    debug_assert!(
        rows.windows(2).all(|w| {
            (w[0].table_id, w[0].col_id, w[0].key) < (w[1].table_id, w[1].col_id, w[1].key)
        }),
        "rows must be sorted by (table_id, col_id, key) with unique keys per segment"
    );

    let width = merge_width::<W>();
    let num_real = rows.len();
    let num_rows = (num_real + 1).next_power_of_two().max(2);
    let mut values = vec![BabyBear::ZERO; num_rows * width];

    // Track whether we've seen an in_new=1 row in the current segment.
    let mut seen_in_new_in_segment = false;
    // Track the previous hash_acc for continuation rows.
    let mut prev_hash_acc: Option<[BabyBear; 8]> = None;

    for (i, row) in rows.iter().enumerate() {
        assert_eq!(row.old_val.len(), W, "old_val length mismatch");
        assert_eq!(row.write_val.len(), W, "write_val length mismatch");
        assert_eq!(row.new_val.len(), W, "new_val length mismatch");

        let offset = i * width;
        let cols: &mut GlobalMergeCols<BabyBear, W> =
            borrow_cols_mut(&mut values[offset..offset + width]);

        cols.is_real = BabyBear::ONE;
        cols.table_id = BabyBear::new(row.table_id);
        cols.col_id = BabyBear::new(row.col_id as u32);
        cols.key.populate(row.key);

        let (s1, s0) = row.source.encode();
        cols.s1 = bool_fe(s1);
        cols.s0 = bool_fe(s0);

        for (j, v) in row.old_val.iter().enumerate() {
            cols.old_val[j] = *v;
        }
        for (j, v) in row.write_val.iter().enumerate() {
            cols.write_val[j] = *v;
        }
        for (j, v) in row.new_val.iter().enumerate() {
            cols.new_val[j] = *v;
        }
        cols.in_new = bool_fe(row.in_new);
        cols.hash_acc = row.hash_acc;

        // Detect segment boundary.
        let is_new_segment = if i == 0 {
            true
        } else {
            let prev = &rows[i - 1];
            row.table_id != prev.table_id || row.col_id != prev.col_id
        };
        if is_new_segment {
            seen_in_new_in_segment = false;
            prev_hash_acc = None;
        }

        // has_prev_in_new: 1 if any prior row in this segment had in_new=1.
        cols.has_prev_in_new = bool_fe(seen_in_new_in_segment);

        // is_first_in_new: first in_new=1 row in segment.
        let is_first_in_new = row.in_new && !seen_in_new_in_segment;
        cols.is_first_in_new = bool_fe(is_first_in_new);

        // Compose perm_input for rows that participate in hashing (in_new=1).
        if row.in_new {
            if is_first_in_new {
                cols.hash_chain.populate_first(
                    row.table_id,
                    row.col_id as u32,
                    row.key,
                    &row.new_val,
                );
            } else {
                cols.hash_chain.populate_continuation(
                    prev_hash_acc.as_ref().expect("continuation must have prev"),
                    row.key,
                    &row.new_val,
                );
            }
            prev_hash_acc = Some(row.hash_acc);
            seen_in_new_in_segment = true;
        }
        // Rows with in_new=0: perm_input stays zero (unconstrained, mult=0).

        // tc_changed and is_last_segment: compare with next row.
        let is_last_segment = if i + 1 < num_real {
            let nxt = &rows[i + 1];
            row.table_id != nxt.table_id || row.col_id != nxt.col_id
        } else {
            true // last real row is always last of segment
        };
        cols.is_last_segment = bool_fe(is_last_segment);
    }

    populate_ordering_witnesses::<W>(rows, num_real, num_rows, width, &mut values);

    RowMajorMatrix::new(values, width)
}

/// Populate ordering witnesses (second pass over all rows including padding).
fn populate_ordering_witnesses<const W: usize>(
    rows: &[MergeRow],
    num_real: usize,
    num_rows: usize,
    width: usize,
    values: &mut [BabyBear],
) {
    for i in 0..num_rows {
        let next_idx = (i + 1) % num_rows;

        let (cur_table, cur_col) = if i < num_real {
            (rows[i].table_id, rows[i].col_id as u32)
        } else {
            (0, 0)
        };
        let (next_table, next_col) = if next_idx < num_real {
            (rows[next_idx].table_id, rows[next_idx].col_id as u32)
        } else {
            (0, 0)
        };

        let table_diff = BabyBear::new(next_table) - BabyBear::new(cur_table);
        let col_diff = BabyBear::new(next_col) - BabyBear::new(cur_col);

        let cur_offset = i * width;
        let cols: &mut GlobalMergeCols<BabyBear, W> =
            borrow_cols_mut(&mut values[cur_offset..cur_offset + width]);

        // Same-key detection
        cols.segment.populate(table_diff, col_diff);

        let is_real_cur = i < num_real;
        let is_real_next = next_idx < num_real;

        if is_real_cur && is_real_next {
            let cur_row = &rows[i];
            let next_row = &rows[next_idx];
            let tc_changed =
                cur_row.table_id != next_row.table_id || cur_row.col_id != next_row.col_id;

            // Lex ordering direction at segment boundaries
            cols.lex.populate(
                cur_row.table_id,
                next_row.table_id,
                cur_row.col_id as u32,
                next_row.col_id as u32,
                tc_changed,
            );

            if !tc_changed {
                // Same segment: key ordering + half-decomposition
                cols.key_ordering.populate(cur_row.key, next_row.key);
            }
        }
    }
}
