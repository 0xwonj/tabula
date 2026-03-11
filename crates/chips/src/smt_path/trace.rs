//! Trace generation for SmtPathChip.

use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;
use p3_matrix::dense::RowMajorMatrix;

use tabula_commitment::NativeDigest;

use crate::poseidon::constants::poseidon2_permutation;
use tabula_stark::air::columns::borrow_cols_mut;

use super::columns::{
    DIGEST_WIDTH, SMT_COL_PATH_WIDTH, SMT_TABLE_PATH_WIDTH, SmtPathCols, SmtTablePathCols,
};

/// A single SMT Merkle path for trace generation.
#[derive(Clone)]
pub struct SmtPathWitness {
    /// Table identifier.
    pub table_id: u32,
    /// Key being proven (col_id for col-level, table_id for table-level).
    pub key: u32,
    /// Old leaf digest (at level 0).
    pub old_leaf: NativeDigest,
    /// New leaf digest (at level 0).
    pub new_leaf: NativeDigest,
    /// Sibling digests from leaf to root in the old tree (length = depth).
    pub old_siblings: Vec<NativeDigest>,
    /// Sibling digests from leaf to root in the new tree (length = depth).
    pub new_siblings: Vec<NativeDigest>,
    /// Path bits from leaf to root (length = depth). bit_i = (key >> i) & 1.
    pub path_bits: Vec<bool>,
}

/// A witness for SmtTablePathChip (adds root_mult_witness).
#[derive(Clone)]
pub struct SmtTablePathWitness {
    /// Base path witness.
    pub path: SmtPathWitness,
    /// Multiplicity for C16 receive (N = number of columns for this table).
    pub root_mult: u32,
}

/// Populate shared SmtPathCols for one level (row) of a Merkle path.
fn populate_path_row(
    cols: &mut SmtPathCols<BabyBear>,
    level: usize,
    depth: usize,
    witness: &SmtPathWitness,
    old_node: NativeDigest,
    new_node: NativeDigest,
) -> (NativeDigest, NativeDigest) {
    cols.is_real = BabyBear::ONE;
    cols.path_bit = if witness.path_bits[level] {
        BabyBear::ONE
    } else {
        BabyBear::ZERO
    };
    cols.is_leaf = if level == 0 {
        BabyBear::ONE
    } else {
        BabyBear::ZERO
    };
    cols.is_root = if level == depth - 1 {
        BabyBear::ONE
    } else {
        BabyBear::ZERO
    };

    cols.bind_table_id = BabyBear::new(witness.table_id);
    cols.bind_key = BabyBear::new(witness.key);

    let old_sib = witness.old_siblings[level];
    let new_sib = witness.new_siblings[level];
    cols.old_sibling = old_sib.0;
    cols.new_sibling = new_sib.0;
    cols.old_node = old_node.0;
    cols.new_node = new_node.0;

    let bit = witness.path_bits[level];

    // Mux for old tree
    let mut old_perm_input = [BabyBear::ZERO; 16];
    for i in 0..DIGEST_WIDTH {
        if bit {
            old_perm_input[i] = old_sib.0[i]; // left = sibling
            old_perm_input[8 + i] = old_node.0[i]; // right = node
        } else {
            old_perm_input[i] = old_node.0[i]; // left = node
            old_perm_input[8 + i] = old_sib.0[i]; // right = sibling
        }
    }
    cols.old_perm_input = old_perm_input;

    let (_rounds, old_perm_out) = poseidon2_permutation(old_perm_input);
    let old_parent = NativeDigest(core::array::from_fn(|j| old_perm_out[j]));
    cols.old_parent = old_parent.0;

    // Mux for new tree
    let mut new_perm_input = [BabyBear::ZERO; 16];
    for i in 0..DIGEST_WIDTH {
        if bit {
            new_perm_input[i] = new_sib.0[i];
            new_perm_input[8 + i] = new_node.0[i];
        } else {
            new_perm_input[i] = new_node.0[i];
            new_perm_input[8 + i] = new_sib.0[i];
        }
    }
    cols.new_perm_input = new_perm_input;

    let (_rounds, new_perm_out) = poseidon2_permutation(new_perm_input);
    let new_parent = NativeDigest(core::array::from_fn(|j| new_perm_out[j]));
    cols.new_parent = new_parent.0;

    // Key reconstruction
    let power = 1u64 << level;
    let bit_val = if bit { 1u64 } else { 0u64 };
    // key_acc at this level = sum of bit_j * 2^j for j in 0..=level
    let key_acc: u64 = (0..=level)
        .map(|j| if witness.path_bits[j] { 1u64 << j } else { 0 })
        .sum();
    cols.key_acc = BabyBear::new(key_acc as u32);
    cols.level_power = BabyBear::new(power as u32);

    // Path boundary detection: is_root means next row is new path
    let is_root_val = if level == depth - 1 {
        BabyBear::ONE
    } else {
        BabyBear::ZERO
    };
    cols.next_is_new_path.populate(is_root_val);

    // Suppress unused variable warnings
    let _ = bit_val;

    (old_parent, new_parent)
}

/// Generate an SmtColPath trace from witness data.
pub fn generate_smt_col_path_trace(witnesses: &[SmtPathWitness]) -> RowMajorMatrix<BabyBear> {
    let width = SMT_COL_PATH_WIDTH;

    // Total real rows = sum of depths
    let num_real: usize = witnesses.iter().map(|w| w.old_siblings.len()).sum();
    let num_rows = (num_real + 1).next_power_of_two().max(2);
    let mut values = vec![BabyBear::ZERO; num_rows * width];

    let mut row_idx = 0;
    for witness in witnesses {
        let depth = witness.old_siblings.len();
        assert_eq!(witness.new_siblings.len(), depth);
        assert_eq!(witness.path_bits.len(), depth);

        let mut old_node = witness.old_leaf;
        let mut new_node = witness.new_leaf;

        for level in 0..depth {
            let offset = row_idx * width;
            let row = &mut values[offset..offset + width];
            let cols: &mut SmtPathCols<BabyBear> = borrow_cols_mut(row);

            let (old_parent, new_parent) =
                populate_path_row(cols, level, depth, witness, old_node, new_node);

            old_node = old_parent;
            new_node = new_parent;
            row_idx += 1;
        }
    }

    // Padding rows: populate IsZero witnesses for boundary detection.
    // Padding rows have is_real=0, so constraints are inactive.
    // We need consistent IsZero witnesses: is_root=0 → diff=0 → is_zero=1.
    for i in num_real..num_rows {
        let offset = i * width;
        let row = &mut values[offset..offset + width];
        let cols: &mut SmtPathCols<BabyBear> = borrow_cols_mut(row);
        cols.next_is_new_path.populate(BabyBear::ZERO);
    }

    RowMajorMatrix::new(values, width)
}

/// Generate an SmtTablePath trace from witness data.
pub fn generate_smt_table_path_trace(
    witnesses: &[SmtTablePathWitness],
) -> RowMajorMatrix<BabyBear> {
    let width = SMT_TABLE_PATH_WIDTH;

    let num_real: usize = witnesses.iter().map(|w| w.path.old_siblings.len()).sum();
    let num_rows = (num_real + 1).next_power_of_two().max(2);
    let mut values = vec![BabyBear::ZERO; num_rows * width];

    let mut row_idx = 0;
    for witness in witnesses {
        let depth = witness.path.old_siblings.len();
        assert_eq!(witness.path.new_siblings.len(), depth);
        assert_eq!(witness.path.path_bits.len(), depth);

        let mut old_node = witness.path.old_leaf;
        let mut new_node = witness.path.new_leaf;

        for level in 0..depth {
            let offset = row_idx * width;
            let row = &mut values[offset..offset + width];
            let cols: &mut SmtTablePathCols<BabyBear> = borrow_cols_mut(row);

            let (old_parent, new_parent) = populate_path_row(
                &mut cols.base,
                level,
                depth,
                &witness.path,
                old_node,
                new_node,
            );

            // root_mult_witness only matters at leaf level
            if level == 0 {
                cols.root_mult_witness = BabyBear::new(witness.root_mult);
            }

            old_node = old_parent;
            new_node = new_parent;
            row_idx += 1;
        }
    }

    // Padding
    for i in num_real..num_rows {
        let offset = i * width;
        let row = &mut values[offset..offset + width];
        let cols: &mut SmtTablePathCols<BabyBear> = borrow_cols_mut(row);
        cols.base.next_is_new_path.populate(BabyBear::ZERO);
    }

    RowMajorMatrix::new(values, width)
}

// ── TraceGenerator impls ────────────────────────────────────────────────────

use tabula_stark::trace::TraceGenerator;

impl TraceGenerator for super::air::SmtColPathChip {
    type Input = [SmtPathWitness];

    fn generate_trace(&self, input: &[SmtPathWitness]) -> RowMajorMatrix<BabyBear> {
        generate_smt_col_path_trace(input)
    }
}

impl TraceGenerator for super::air::SmtTablePathChip {
    type Input = [SmtTablePathWitness];

    fn generate_trace(&self, input: &[SmtTablePathWitness]) -> RowMajorMatrix<BabyBear> {
        generate_smt_table_path_trace(input)
    }
}

// ── TraceContributor impls ─────────────────────────────────────────────────

use crate::ChipSpec;
use tabula_core::error::TabulaError;
use tabula_stark::trace::contributor::{
    TraceContributor, TracePhase, WitnessStore, witness_labels,
};
use tabula_stark::trace::trace_map::TraceMap;

impl TraceContributor for super::air::SmtColPathChip {
    fn phase(&self) -> TracePhase {
        TracePhase::INDEPENDENT
    }

    fn contribute(&self, store: &WitnessStore, map: &mut TraceMap) -> Result<(), TabulaError> {
        let paths = store.get::<Vec<SmtPathWitness>>(witness_labels::SMT_COL_PATHS)?;
        let entry = self.build_entry(paths);
        map.insert_entry(self.chip_id(), entry);
        Ok(())
    }
}

impl TraceContributor for super::air::SmtTablePathChip {
    fn phase(&self) -> TracePhase {
        TracePhase::INDEPENDENT
    }

    fn contribute(&self, store: &WitnessStore, map: &mut TraceMap) -> Result<(), TabulaError> {
        let paths = store.get::<Vec<SmtTablePathWitness>>(witness_labels::SMT_TABLE_PATHS)?;
        let entry = self.build_entry(paths);
        map.insert_entry(self.chip_id(), entry);

        // SmtTablePath also carries public values (old/new state root).
        let pvs = store.get::<Vec<BabyBear>>(witness_labels::SMT_TABLE_PVS)?;
        map.set_public_values(self.chip_id(), pvs.clone());
        Ok(())
    }
}
