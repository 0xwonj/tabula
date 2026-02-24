//! Trace generation for the ColumnMeta chip.
//!
//! Converts `ColumnMeta` witness data into a `RowMajorMatrix<BabyBear>` trace.

use std::collections::BTreeMap;

use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;
use p3_matrix::dense::RowMajorMatrix;

use tabula_commitment::ColumnMeta;

use crate::air::chips::poseidon::constants::poseidon2_permutation;
use crate::air::columns::borrow_cols_mut;
use crate::air::gadgets::bool_fe;

use super::columns::{COLUMN_META_WIDTH, ColumnMetaCols};

/// Generate a ColumnMeta trace from witness data.
///
/// `empty_read_counts` maps `(table, col)` to the number of Execution
/// empty-col reads targeting that column. This gates the EmptyColRead bus receive.
///
/// Rows are padded to the next power of two (Plonky3 requirement).
/// Padding rows have `is_real = 0`.
pub fn generate_column_meta_trace(
    metas: &[ColumnMeta],
    empty_read_counts: &BTreeMap<(u32, u16), u32>,
) -> RowMajorMatrix<BabyBear> {
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
        cols.empty_read_mult = BabyBear::new(
            *empty_read_counts
                .get(&(meta.table.0, meta.col.0))
                .unwrap_or(&0),
        );

        // Compute IsZero witness columns for lex ordering.
        if i + 1 < num_real {
            let next_meta = &metas[i + 1];

            let t_diff = BabyBear::new(next_meta.table.0) - BabyBear::new(meta.table.0);
            cols.table_diff_iz.populate(t_diff);

            let c_diff = BabyBear::new(next_meta.col.0 as u32) - BabyBear::new(meta.col.0 as u32);
            cols.col_diff_iz.populate(c_diff);

            // Lex ordering direction (A2)
            cols.lex.populate(
                meta.table.0,
                next_meta.table.0,
                meta.col.0 as u32,
                next_meta.col.0 as u32,
                true, // every real→real pair is a (t,c) boundary in ColumnMeta
            );
        } else {
            // Last real row or padding: IsZero witnesses for zero diffs
            // (transition to padding where both IDs are 0).
            cols.table_diff_iz
                .populate(BabyBear::ZERO - BabyBear::new(meta.table.0));
            cols.col_diff_iz
                .populate(BabyBear::ZERO - BabyBear::new(meta.col.0 as u32));
        }

        // Com_empty verification (B4)
        let has_empty = meta.is_empty_old || meta.is_empty_new;
        cols.has_empty_check = bool_fe(has_empty);
        if has_empty {
            // Compose: [0x00, table_id, col_id, 0..]
            let mut perm_input = [BabyBear::ZERO; 16];
            perm_input[1] = BabyBear::new(meta.table.0);
            perm_input[2] = BabyBear::new(meta.col.0 as u32);
            cols.empty_perm_input = perm_input;

            let (_rounds, perm_output_full) = poseidon2_permutation(perm_input);
            let perm_output: [BabyBear; 8] = core::array::from_fn(|j| perm_output_full[j]);
            cols.empty_perm_output = perm_output;
        }

        // Leaf digest (M11 Phase 3)
        {
            let tag_fe = match meta.tag {
                tabula_commitment::CommitmentStrategy::Ssmc => BabyBear::ZERO,
                tabula_commitment::CommitmentStrategy::Smt => BabyBear::ONE,
            };

            // Old leaf perm input: [0x10, t, c, tag, 0,0,0,0, com_old[8]]
            let mut leaf_input_old = [BabyBear::ZERO; 16];
            leaf_input_old[0] = BabyBear::new(0x10);
            leaf_input_old[1] = BabyBear::new(meta.table.0);
            leaf_input_old[2] = BabyBear::new(meta.col.0 as u32);
            leaf_input_old[3] = tag_fe;
            leaf_input_old[8..16].copy_from_slice(&meta.com_old.0);
            cols.leaf_perm_input_old = leaf_input_old;
            let (_rounds, perm_out_old) = poseidon2_permutation(leaf_input_old);
            cols.leaf_digest_old = core::array::from_fn(|j| perm_out_old[j]);

            // New leaf perm input: [0x10, t, c, tag, 0,0,0,0, com_new[8]]
            let mut leaf_input_new = [BabyBear::ZERO; 16];
            leaf_input_new[0] = BabyBear::new(0x10);
            leaf_input_new[1] = BabyBear::new(meta.table.0);
            leaf_input_new[2] = BabyBear::new(meta.col.0 as u32);
            leaf_input_new[3] = tag_fe;
            leaf_input_new[8..16].copy_from_slice(&meta.com_new.0);
            cols.leaf_perm_input_new = leaf_input_new;
            let (_rounds, perm_out_new) = poseidon2_permutation(leaf_input_new);
            cols.leaf_digest_new = core::array::from_fn(|j| perm_out_new[j]);
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

// ── TraceGenerator impl ─────────────────────────────────────────────────────

use crate::trace_builder::TraceGenerator;

/// Input struct for `ColumnMetaChip` trace generation.
pub struct ColumnMetaInput {
    /// Column metadata entries.
    pub metas: Vec<ColumnMeta>,
    /// Empty-column read counts: `(table, col) -> count`.
    pub empty_read_counts: BTreeMap<(u32, u16), u32>,
}

impl TraceGenerator for super::air::ColumnMetaChip {
    type Input = ColumnMetaInput;

    fn generate_trace(&self, input: &ColumnMetaInput) -> RowMajorMatrix<BabyBear> {
        generate_column_meta_trace(&input.metas, &input.empty_read_counts)
    }
}
