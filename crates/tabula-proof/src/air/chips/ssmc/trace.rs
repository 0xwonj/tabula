//! Trace generation for the GlobalSSMC chip.
//!
//! Converts witness data (SSMC entries per column) into a
//! `RowMajorMatrix<BabyBear>` trace.

use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;
use p3_matrix::dense::RowMajorMatrix;

use super::columns::{GlobalSsmcCols, ssmc_width};
use crate::air::columns::borrow_cols_mut;
use crate::air::gadgets::bool_fe;

/// A flat row for building the SSMC trace.
///
/// One entry per SSMC-committed (key, value) pair, pre-sorted by
/// `(table_id, col_id, key)`.
pub struct SsmcEntry {
    /// Table identifier.
    pub table_id: u32,
    /// Column identifier.
    pub col_id: u16,
    /// Row key (u64).
    pub key: u64,
    /// Value field elements (length must equal W).
    pub value: Vec<BabyBear>,
    /// Running Poseidon hash chain accumulator (precomputed).
    pub hash_acc: [BabyBear; 8],
}

/// Generate a GlobalSSMC trace from pre-sorted SSMC entries.
///
/// `entries` must be sorted by `(table_id, col_id, key)`.
/// Keys must be strictly increasing within each `(table_id, col_id)` segment.
/// Padding rows have `is_real = 0`.
pub fn generate_ssmc_trace<const W: usize>(entries: &[SsmcEntry]) -> RowMajorMatrix<BabyBear> {
    debug_assert!(
        entries.windows(2).all(|w| {
            (w[0].table_id, w[0].col_id, w[0].key) < (w[1].table_id, w[1].col_id, w[1].key)
        }),
        "entries must be sorted by (table_id, col_id, key) with unique keys per segment"
    );

    let width = ssmc_width::<W>();
    let num_real = entries.len();
    let num_rows = (num_real + 1).next_power_of_two().max(2);
    let mut values = vec![BabyBear::ZERO; num_rows * width];

    for (i, entry) in entries.iter().enumerate() {
        assert_eq!(
            entry.value.len(),
            W,
            "value length mismatch: expected {W}, got {}",
            entry.value.len()
        );

        let offset = i * width;
        let cols: &mut GlobalSsmcCols<BabyBear, W> =
            borrow_cols_mut(&mut values[offset..offset + width]);

        cols.is_real = BabyBear::ONE;
        cols.table_id = BabyBear::new(entry.table_id);
        cols.col_id = BabyBear::new(entry.col_id as u32);
        cols.key.populate(entry.key);
        for (j, v) in entry.value.iter().enumerate() {
            cols.value[j] = *v;
        }
        cols.hash_acc = entry.hash_acc;

        // Determine segment boundaries.
        let is_first = if i == 0 {
            true
        } else {
            let prev = &entries[i - 1];
            entry.table_id != prev.table_id || entry.col_id != prev.col_id
        };

        let is_last = if i + 1 >= num_real {
            true
        } else {
            let nxt = &entries[i + 1];
            entry.table_id != nxt.table_id || entry.col_id != nxt.col_id
        };

        cols.is_first = bool_fe(is_first);
        cols.is_last = bool_fe(is_last);

        // tc_changed: compare with next row.
        if i + 1 < num_real {
            let nxt = &entries[i + 1];
            let tc_changed = entry.table_id != nxt.table_id || entry.col_id != nxt.col_id;
            cols.tc_changed = bool_fe(tc_changed);
        } else {
            // Last real row: next is padding.
            cols.tc_changed = BabyBear::ONE;
        }
    }

    populate_ordering_witnesses::<W>(entries, num_real, num_rows, width, &mut values);

    RowMajorMatrix::new(values, width)
}

/// Populate IsZero and StrictIneq ordering witnesses (second pass).
fn populate_ordering_witnesses<const W: usize>(
    entries: &[SsmcEntry],
    num_real: usize,
    num_rows: usize,
    width: usize,
    values: &mut [BabyBear],
) {
    for i in 0..num_rows {
        let next_idx = (i + 1) % num_rows;

        let (cur_table, cur_col) = if i < num_real {
            let e = &entries[i];
            (BabyBear::new(e.table_id), BabyBear::new(e.col_id as u32))
        } else {
            (BabyBear::ZERO, BabyBear::ZERO)
        };
        let (next_table, next_col) = if next_idx < num_real {
            let e = &entries[next_idx];
            (BabyBear::new(e.table_id), BabyBear::new(e.col_id as u32))
        } else {
            (BabyBear::ZERO, BabyBear::ZERO)
        };

        let table_diff = next_table - cur_table;
        let col_diff = next_col - cur_col;

        let cur_offset = i * width;
        let cols: &mut GlobalSsmcCols<BabyBear, W> =
            borrow_cols_mut(&mut values[cur_offset..cur_offset + width]);
        cols.table_diff_iz.populate(table_diff);
        cols.col_diff_iz.populate(col_diff);

        // StrictIneq for key ordering: only within same segment, real→real.
        let is_real_cur = i < num_real;
        let is_real_next = next_idx < num_real;
        if is_real_cur && is_real_next {
            let cur_entry = &entries[i];
            let next_entry = &entries[next_idx];
            let tc_changed =
                cur_entry.table_id != next_entry.table_id || cur_entry.col_id != next_entry.col_id;
            if !tc_changed {
                // Same segment: key must be strictly increasing.
                cols.key_ordering.populate(cur_entry.key, next_entry.key);
            }
            // Cross-segment: key ordering gated off in AIR, leave as zeros.
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::air::debug::debug_check;

    use super::super::air::GlobalSsmcChip;

    fn entry(t: u32, c: u16, key: u64, val: [u32; 3]) -> SsmcEntry {
        SsmcEntry {
            table_id: t,
            col_id: c,
            key,
            value: val.iter().map(|v| BabyBear::new(*v)).collect(),
            hash_acc: [BabyBear::ZERO; 8],
        }
    }

    // ── Valid traces ──

    #[test]
    fn valid_single_entry() {
        let entries = vec![entry(0, 0, 100, [1, 2, 3])];
        let trace = generate_ssmc_trace::<3>(&entries);
        debug_check(&GlobalSsmcChip::<3>, &trace).expect("single entry should pass");
    }

    #[test]
    fn valid_multiple_entries_same_segment() {
        let entries = vec![
            entry(0, 0, 10, [1, 0, 0]),
            entry(0, 0, 20, [2, 0, 0]),
            entry(0, 0, 30, [3, 0, 0]),
        ];
        let trace = generate_ssmc_trace::<3>(&entries);
        debug_check(&GlobalSsmcChip::<3>, &trace).expect("multi-entry same segment should pass");
    }

    #[test]
    fn valid_two_segments() {
        let entries = vec![
            entry(0, 0, 10, [1, 0, 0]),
            entry(0, 0, 20, [2, 0, 0]),
            entry(0, 1, 5, [3, 0, 0]),
            entry(0, 1, 15, [4, 0, 0]),
        ];
        let trace = generate_ssmc_trace::<3>(&entries);
        debug_check(&GlobalSsmcChip::<3>, &trace).expect("two segments should pass");
    }

    #[test]
    fn valid_different_tables() {
        let entries = vec![entry(0, 0, 100, [1, 0, 0]), entry(1, 0, 50, [2, 0, 0])];
        let trace = generate_ssmc_trace::<3>(&entries);
        debug_check(&GlobalSsmcChip::<3>, &trace).expect("different tables should pass");
    }

    #[test]
    fn valid_all_padding() {
        let entries: Vec<SsmcEntry> = vec![];
        let trace = generate_ssmc_trace::<3>(&entries);
        debug_check(&GlobalSsmcChip::<3>, &trace).expect("all-padding should pass");
    }

    #[test]
    fn valid_large_keys() {
        let entries = vec![
            entry(0, 0, 1 << 30, [1, 0, 0]),
            entry(0, 0, (1 << 60) + 1, [2, 0, 0]),
            entry(0, 0, u64::MAX - 1, [3, 0, 0]),
        ];
        let trace = generate_ssmc_trace::<3>(&entries);
        debug_check(&GlobalSsmcChip::<3>, &trace).expect("large keys should pass");
    }

    #[test]
    fn valid_single_entry_per_segment() {
        let entries = vec![
            entry(0, 0, 10, [1, 0, 0]),
            entry(0, 1, 20, [2, 0, 0]),
            entry(1, 0, 30, [3, 0, 0]),
        ];
        let trace = generate_ssmc_trace::<3>(&entries);
        debug_check(&GlobalSsmcChip::<3>, &trace).expect("single entry per segment should pass");
    }

    // ── Invalid traces ──

    #[test]
    fn invalid_broken_is_real_prefix() {
        // Corrupt: make row 0 non-real and row 1 real (breaks prefix invariant).
        let entries = vec![entry(0, 0, 10, [1, 0, 0])];
        let mut trace = generate_ssmc_trace::<3>(&entries);
        let width = ssmc_width::<3>();
        trace.values[0] = BabyBear::ZERO; // row 0: is_real = 0
        trace.values[width] = BabyBear::ONE; // row 1: is_real = 1
        debug_check(&GlobalSsmcChip::<3>, &trace).expect_err("broken is_real prefix should fail");
    }

    #[test]
    fn invalid_is_first_wrong() {
        // is_first = 0 on the first row.
        let entries = vec![entry(0, 0, 10, [1, 0, 0])];
        let mut trace = generate_ssmc_trace::<3>(&entries);
        // is_first is at offset: is_real(1) + table_id(1) + col_id(1) + key(3) + value(3) = 9
        let is_first_offset = 9;
        trace.values[is_first_offset] = BabyBear::ZERO; // corrupt is_first
        debug_check(&GlobalSsmcChip::<3>, &trace).expect_err("wrong is_first should fail");
    }

    #[test]
    fn invalid_is_last_wrong() {
        // is_last = 0 on the last real row (before padding).
        let entries = vec![entry(0, 0, 10, [1, 0, 0])];
        let mut trace = generate_ssmc_trace::<3>(&entries);
        // is_last is at offset: is_first(9) + 1 = 10
        let is_last_offset = 10;
        trace.values[is_last_offset] = BabyBear::ZERO; // corrupt is_last
        debug_check(&GlobalSsmcChip::<3>, &trace).expect_err("wrong is_last should fail");
    }

    #[test]
    fn invalid_boundary_flags_mismatch() {
        // Two entries in same segment, but is_last=1 on first entry.
        let entries = vec![entry(0, 0, 10, [1, 0, 0]), entry(0, 0, 20, [2, 0, 0])];
        let mut trace = generate_ssmc_trace::<3>(&entries);
        let is_last_offset = 10;
        trace.values[is_last_offset] = BabyBear::ONE; // corrupt: first entry claims is_last
        debug_check(&GlobalSsmcChip::<3>, &trace).expect_err("wrong boundary flag should fail");
    }
}
