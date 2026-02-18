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
use crate::air::gadgets::integer::MASK_30;

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
    /// 1 if this entry is looked up by a SortedMem init row (C2 receive).
    pub mult_witness: bool,
    /// 1 if this segment's column is touched in the batch (C3 send).
    pub segment_is_touched: bool,
}

/// Compose the 16-element Poseidon input for an SSMC hash chain step.
///
/// - First entry: `[0x00, table_id, col_id, key[3], value[W], 0..]`
/// - Continuation: `[prev_hash_acc[8], key[3], value[W], 0..]`
fn compose_ssmc_perm_input(
    entry: &SsmcEntry,
    is_first: bool,
    prev_hash_acc: Option<&[BabyBear; 8]>,
) -> [BabyBear; 16] {
    let key_limbs = [
        BabyBear::new((entry.key & MASK_30) as u32),
        BabyBear::new(((entry.key >> 30) & MASK_30) as u32),
        BabyBear::new((entry.key >> 60) as u32),
    ];

    let mut input = [BabyBear::ZERO; 16];
    if is_first {
        // [0x00, table_id, col_id, key[3], value[W], 0..]
        input[0] = BabyBear::ZERO; // domain tag 0x00
        input[1] = BabyBear::new(entry.table_id);
        input[2] = BabyBear::new(entry.col_id as u32);
        input[3] = key_limbs[0];
        input[4] = key_limbs[1];
        input[5] = key_limbs[2];
        for (i, v) in entry.value.iter().enumerate() {
            input[6 + i] = *v;
        }
    } else {
        // [prev_hash_acc[8], key[3], value[W], 0..]
        let prev = prev_hash_acc.expect("continuation row must have prev_hash_acc");
        input[..8].copy_from_slice(prev);
        input[8] = key_limbs[0];
        input[9] = key_limbs[1];
        input[10] = key_limbs[2];
        for (i, v) in entry.value.iter().enumerate() {
            input[11 + i] = *v;
        }
    }
    input
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

        // Compose perm_input for hash chain.
        let is_first_entry = if i == 0 {
            true
        } else {
            let prev = &entries[i - 1];
            entry.table_id != prev.table_id || entry.col_id != prev.col_id
        };
        let prev_hash = if is_first_entry {
            None
        } else {
            Some(&entries[i - 1].hash_acc)
        };
        cols.perm_input = compose_ssmc_perm_input(entry, is_first_entry, prev_hash);

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

        // LogUp witness columns.
        cols.mult_witness = bool_fe(entry.mult_witness);
        cols.segment_is_touched = bool_fe(entry.segment_is_touched);
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
