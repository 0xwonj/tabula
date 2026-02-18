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

        // tc_changed: compare with next row.
        if i + 1 < num_real {
            let nxt = &rows[i + 1];
            let tc_changed = row.table_id != nxt.table_id || row.col_id != nxt.col_id;
            cols.tc_changed = bool_fe(tc_changed);
        } else {
            cols.tc_changed = BabyBear::ONE;
        }
    }

    populate_ordering_witnesses::<W>(rows, num_real, num_rows, width, &mut values);

    RowMajorMatrix::new(values, width)
}

/// Populate IsZero and StrictIneq ordering witnesses (second pass).
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
            let r = &rows[i];
            (BabyBear::new(r.table_id), BabyBear::new(r.col_id as u32))
        } else {
            (BabyBear::ZERO, BabyBear::ZERO)
        };
        let (next_table, next_col) = if next_idx < num_real {
            let r = &rows[next_idx];
            (BabyBear::new(r.table_id), BabyBear::new(r.col_id as u32))
        } else {
            (BabyBear::ZERO, BabyBear::ZERO)
        };

        let table_diff = next_table - cur_table;
        let col_diff = next_col - cur_col;

        let cur_offset = i * width;
        let cols: &mut GlobalMergeCols<BabyBear, W> =
            borrow_cols_mut(&mut values[cur_offset..cur_offset + width]);
        cols.table_diff_iz.populate(table_diff);
        cols.col_diff_iz.populate(col_diff);

        // StrictIneq for key ordering: only within same segment, real→real.
        let is_real_cur = i < num_real;
        let is_real_next = next_idx < num_real;
        if is_real_cur && is_real_next {
            let cur_row = &rows[i];
            let next_row = &rows[next_idx];
            let tc_changed =
                cur_row.table_id != next_row.table_id || cur_row.col_id != next_row.col_id;
            if !tc_changed {
                cols.key_ordering.populate(cur_row.key, next_row.key);
            }
        }
    }
}
