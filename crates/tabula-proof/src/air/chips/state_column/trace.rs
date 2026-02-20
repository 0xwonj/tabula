//! Trace generation for the StateColumn chip.
//!
//! Converts witness data (sorted rows per column) into a
//! `RowMajorMatrix<BabyBear>` trace.

use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;
use p3_matrix::dense::RowMajorMatrix;

use crate::air::columns::borrow_cols_mut;
use crate::air::gadgets::bool_fe;

use super::columns::{StateColumnCols, state_column_width};

/// Source type for a state column entry row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntrySource {
    /// Key exists only in old state: (s1,s0) = (0,0).
    OldOnly,
    /// Key exists only in write set: (s1,s0) = (0,1).
    WriteOnly,
    /// Key exists in both old state and write set: (s1,s0) = (1,0).
    Both,
    /// Key deleted (write null): (s1,s0) = (1,1).
    Delete,
}

impl EntrySource {
    /// Encode as (s1, s0) pair.
    fn encode(self) -> (bool, bool) {
        match self {
            Self::OldOnly => (false, false),
            Self::WriteOnly => (false, true),
            Self::Both => (true, false),
            Self::Delete => (true, true),
        }
    }

    /// Whether this entry is in the old set.
    pub fn in_old(self) -> bool {
        matches!(self, Self::OldOnly | Self::Both | Self::Delete)
    }

    /// Whether this entry is in the new set.
    pub fn in_new(self) -> bool {
        matches!(self, Self::OldOnly | Self::WriteOnly | Self::Both)
    }

    /// Whether this entry is a write (write_only, both, or delete).
    pub fn in_write(self) -> bool {
        matches!(self, Self::WriteOnly | Self::Both | Self::Delete)
    }
}

/// A single row for building the StateColumn trace.
///
/// Pre-sorted by `(table_id, col_id, key)`.
pub struct StateColumnRow {
    /// Table identifier.
    pub table_id: u32,
    /// Column identifier.
    pub col_id: u16,
    /// Row key (u64).
    pub key: u64,
    /// True if this is a gap row (non-membership proof).
    pub is_gap: bool,
    /// Source type (meaningful only for entry rows).
    pub source: EntrySource,
    /// Old value (zeros for write_only/gap).
    pub old_val: Vec<BabyBear>,
    /// New value (zeros for delete/gap).
    pub new_val: Vec<BabyBear>,
    /// Per-segment: 1 if this column is touched in the batch.
    pub segment_is_touched: bool,
    /// Precomputed old hash chain accumulator.
    pub old_hash_acc: [BabyBear; 8],
    /// Precomputed new hash chain accumulator.
    pub new_hash_acc: [BabyBear; 8],
    /// Multiplicity for ReadAccess bus (C1 receive).
    pub read_mult: bool,
    /// Multiplicity for WriteAccess bus (C4 receive).
    pub write_mult: bool,
}

/// Generate a StateColumn trace from pre-sorted rows.
///
/// `rows` must be sorted by `(table_id, col_id, key)`.
/// Keys must be strictly increasing within each `(table_id, col_id)` segment.
pub fn generate_state_column_trace<const W: usize>(
    rows: &[StateColumnRow],
) -> RowMajorMatrix<BabyBear> {
    debug_assert!(
        rows.windows(2).all(|w| {
            (w[0].table_id, w[0].col_id, w[0].key) < (w[1].table_id, w[1].col_id, w[1].key)
        }),
        "rows must be sorted by (table_id, col_id, key)"
    );

    let width = state_column_width::<W>();
    let num_real = rows.len();
    let num_rows = (num_real + 1).next_power_of_two().max(2);
    let mut values = vec![BabyBear::ZERO; num_rows * width];

    // Per-segment running state for old/new chains.
    let mut seen_old_in_segment = false;
    let mut seen_new_in_segment = false;
    let mut seen_write_in_segment = false;
    let mut prev_old_hash_acc: Option<[BabyBear; 8]> = None;
    let mut prev_new_hash_acc: Option<[BabyBear; 8]> = None;

    for (i, row) in rows.iter().enumerate() {
        assert_eq!(row.old_val.len(), W, "old_val length mismatch");
        assert_eq!(row.new_val.len(), W, "new_val length mismatch");

        let offset = i * width;
        let cols: &mut StateColumnCols<BabyBear, W> =
            borrow_cols_mut(&mut values[offset..offset + width]);

        cols.is_real = BabyBear::ONE;
        cols.table_id = BabyBear::new(row.table_id);
        cols.col_id = BabyBear::new(row.col_id as u32);
        cols.key.populate(row.key);

        cols.is_gap = bool_fe(row.is_gap);
        if row.is_gap {
            // Gap rows: s1=s0=0, values=0 (already zero-initialized)
        } else {
            let (s1, s0) = row.source.encode();
            cols.s1 = bool_fe(s1);
            cols.s0 = bool_fe(s0);
            for (j, v) in row.old_val.iter().enumerate() {
                cols.old_val[j] = *v;
            }
            for (j, v) in row.new_val.iter().enumerate() {
                cols.new_val[j] = *v;
            }
        }

        cols.segment_is_touched = bool_fe(row.segment_is_touched);
        cols.old_hash_acc = row.old_hash_acc;
        cols.new_hash_acc = row.new_hash_acc;
        cols.read_mult_witness = bool_fe(row.read_mult);
        cols.write_mult_witness = bool_fe(row.write_mult);

        // Detect segment boundary.
        let is_new_segment = if i == 0 {
            true
        } else {
            let prev = &rows[i - 1];
            row.table_id != prev.table_id || row.col_id != prev.col_id
        };
        if is_new_segment {
            seen_old_in_segment = false;
            seen_new_in_segment = false;
            seen_write_in_segment = false;
            prev_old_hash_acc = None;
            prev_new_hash_acc = None;
        }

        let in_old = !row.is_gap && row.source.in_old();
        let in_new = !row.is_gap && row.source.in_new();
        let in_write = !row.is_gap && row.source.in_write();
        seen_write_in_segment |= in_write;
        cols.write_seen_prefix = bool_fe(seen_write_in_segment);

        // ── Old chain tracking ──
        cols.has_prev_old_entry = bool_fe(seen_old_in_segment);

        if in_old {
            let is_first_old = !seen_old_in_segment;
            if is_first_old {
                cols.old_hash_chain.populate_first(
                    row.table_id,
                    row.col_id as u32,
                    row.key,
                    &row.old_val,
                );
            } else {
                cols.old_hash_chain.populate_continuation(
                    prev_old_hash_acc
                        .as_ref()
                        .expect("continuation must have prev"),
                    row.key,
                    &row.old_val,
                );
            }
            prev_old_hash_acc = Some(row.old_hash_acc);
            seen_old_in_segment = true;
        }

        // ── New chain tracking ──
        cols.has_prev_new_entry = bool_fe(seen_new_in_segment);

        if in_new {
            let is_first_new = !seen_new_in_segment;
            if is_first_new {
                cols.new_hash_chain.populate_first(
                    row.table_id,
                    row.col_id as u32,
                    row.key,
                    &row.new_val,
                );
            } else {
                cols.new_hash_chain.populate_continuation(
                    prev_new_hash_acc
                        .as_ref()
                        .expect("continuation must have prev"),
                    row.key,
                    &row.new_val,
                );
            }
            prev_new_hash_acc = Some(row.new_hash_acc);
            seen_new_in_segment = true;
        }

        // ── is_last_old_entry / is_last_new_entry / past_last_old_entry ──
        // These require look-ahead; computed in second pass.
    }

    // Second pass: compute is_last_old_entry, is_last_new_entry, past_last_old_entry.
    populate_chain_tracking_flags::<W>(rows, num_real, width, &mut values);

    // Third pass: ordering witnesses, segment detection, lex direction.
    populate_ordering_witnesses::<W>(rows, num_real, num_rows, width, &mut values);

    RowMajorMatrix::new(values, width)
}

/// Compute chain tracking flags (requires look-ahead).
fn populate_chain_tracking_flags<const W: usize>(
    rows: &[StateColumnRow],
    num_real: usize,
    width: usize,
    values: &mut [BabyBear],
) {
    // Iterate backwards through each segment to find last old/new entries.
    let mut past_old = false;
    let mut past_new = false;

    for i in (0..num_real).rev() {
        let is_new_segment_boundary = if i + 1 < num_real {
            let next = &rows[i + 1];
            rows[i].table_id != next.table_id || rows[i].col_id != next.col_id
        } else {
            true // last row is segment end
        };

        if is_new_segment_boundary {
            past_old = false;
            past_new = false;
        }

        let row = &rows[i];
        let in_old = !row.is_gap && row.source.in_old();
        let in_new = !row.is_gap && row.source.in_new();

        let offset = i * width;
        let cols: &mut StateColumnCols<BabyBear, W> =
            borrow_cols_mut(&mut values[offset..offset + width]);

        if in_old && !past_old {
            cols.is_last_old_entry = BabyBear::ONE;
            past_old = true;
        }
        // Note: past_last_old_entry is set forward. After finding last old,
        // subsequent rows (lower index) are NOT past. We need forward pass.

        if in_new && !past_new {
            cols.is_last_new_entry = BabyBear::ONE;
            past_new = true;
        }
    }

    // Forward pass for past_last_old_entry.
    let mut past_last_old = false;
    for i in 0..num_real {
        let is_new_segment = if i == 0 {
            true
        } else {
            let prev = &rows[i - 1];
            rows[i].table_id != prev.table_id || rows[i].col_id != prev.col_id
        };
        if is_new_segment {
            past_last_old = false;
        }

        let offset = i * width;
        let cols: &mut StateColumnCols<BabyBear, W> =
            borrow_cols_mut(&mut values[offset..offset + width]);

        if cols.is_last_old_entry == BabyBear::ONE {
            // Next row starts past_last_old
            past_last_old = true;
            // This row itself is NOT past
            cols.past_last_old_entry = BabyBear::ZERO;
        } else {
            cols.past_last_old_entry = bool_fe(past_last_old);
        }
    }
}

/// Populate ordering witnesses (pass over all rows including padding).
fn populate_ordering_witnesses<const W: usize>(
    rows: &[StateColumnRow],
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
        let cols: &mut StateColumnCols<BabyBear, W> =
            borrow_cols_mut(&mut values[cur_offset..cur_offset + width]);

        cols.segment.populate(table_diff, col_diff);

        let is_real_cur = i < num_real;
        let is_real_next = next_idx < num_real;

        if is_real_cur && is_real_next {
            let tc_changed = cur_table != next_table || cur_col != next_col;

            cols.lex
                .populate(cur_table, next_table, cur_col, next_col, tc_changed);

            if !tc_changed {
                let cur_key = rows[i].key;
                let next_key = rows[next_idx].key;
                cols.key_ordering.populate(cur_key, next_key);
            }
        }
    }
}
