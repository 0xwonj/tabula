//! Trace generation for the SMT state shard chip.

use p3_field::PrimeCharacteristicRing;
use p3_koala_bear::KoalaBear;
use p3_matrix::dense::RowMajorMatrix;

use tabula_commitment::DOMAIN_SMT;
use tabula_commitment::NativeDigest;
use tabula_gadgets::bool_fe;
use tabula_stark::air::columns::borrow_cols_mut;
use tabula_stark::trace::TraceGenerator;
use tabula_stark::trace::contributor::{TraceContributor, TracePhase, WitnessStore};
use tabula_stark::trace::trace_map::TraceMap;

use crate::ChipSpec;
use crate::poseidon::constants::poseidon2_permutation;

use super::air::SmtStateShardChip;
use super::columns::{
    DIGEST_WIDTH, HI_REGION_ROOT_POWER, LOW_REGION_SWITCH_POWER, SMT_DATA_DEPTH, SmtStateShardCols,
    smt_state_shard_width,
};

/// WitnessStore label for [`SmtStateWitness`].
pub const SMT_STATE_WITNESS_LABEL: &str = "smt_state_witness";

/// Witness for one row-level SMT path.
#[derive(Debug, Clone)]
pub struct SmtStatePathWitness<const W: usize> {
    /// Row key proved by this path.
    pub key: u64,
    /// Old value at the key, or zeros when absent.
    pub old_val: [KoalaBear; W],
    /// New value at the key, or zeros when absent.
    pub new_val: [KoalaBear; W],
    /// Whether the old key is absent.
    pub old_is_null: bool,
    /// Whether the new key is absent.
    pub new_is_null: bool,
    /// Whether this path corresponds to a final write.
    pub write_mult: bool,
    /// Old-tree sibling digests from leaf to root.
    pub old_siblings: Vec<NativeDigest>,
    /// New-tree sibling digests from leaf to root.
    pub new_siblings: Vec<NativeDigest>,
    /// Path direction bits from leaf to root.
    pub path_bits: Vec<bool>,
}

/// Per-column witness for the SMT state shard chip.
#[derive(Debug, Clone)]
pub struct SmtStateWitness<const W: usize> {
    /// Table identifier.
    pub table_id: u32,
    /// Column identifier.
    pub col_id: u16,
    /// Column root before the batch.
    pub column_old_root: NativeDigest,
    /// Column root after the batch.
    pub column_new_root: NativeDigest,
    /// Whether the column commitment before the batch is empty.
    pub column_is_empty_old: bool,
    /// Whether the column commitment after the batch is empty.
    pub column_is_empty_new: bool,
    /// Whether any final write touched this column.
    pub column_is_touched: bool,
    /// Row-level path witnesses for all touched keys in this column.
    pub paths: Vec<SmtStatePathWitness<W>>,
}

fn populate_leaf_input<const W: usize>(
    value: &[KoalaBear; W],
    is_null: bool,
) -> ([KoalaBear; 16], NativeDigest) {
    let mut input = [KoalaBear::ZERO; 16];
    if is_null {
        input[0] = KoalaBear::new(DOMAIN_SMT);
    } else {
        input[..W].copy_from_slice(value);
    }
    let (_rounds, out) = poseidon2_permutation(input);
    (input, NativeDigest(core::array::from_fn(|i| out[i])))
}

/// Generate an SMT state shard trace from witness data.
pub fn generate_smt_state_shard_trace<const W: usize>(
    witness: &SmtStateWitness<W>,
) -> RowMajorMatrix<KoalaBear> {
    let width = smt_state_shard_width::<W>();
    let num_real = witness.paths.len() * SMT_DATA_DEPTH;
    let num_rows = (num_real + 1).next_power_of_two().max(2);
    let mut values = vec![KoalaBear::ZERO; num_rows * width];

    let mut row_idx = 0;
    for (path_idx, path) in witness.paths.iter().enumerate() {
        assert_eq!(path.old_siblings.len(), SMT_DATA_DEPTH);
        assert_eq!(path.new_siblings.len(), SMT_DATA_DEPTH);
        assert_eq!(path.path_bits.len(), SMT_DATA_DEPTH);

        let (old_leaf_input, old_leaf_hash) = populate_leaf_input(&path.old_val, path.old_is_null);
        let (new_leaf_input, new_leaf_hash) = populate_leaf_input(&path.new_val, path.new_is_null);

        let mut old_node = old_leaf_hash;
        let mut new_node = new_leaf_hash;
        let mut low_acc = 0u32;
        let mut low_power = 1u32;
        let mut hi_acc = 0u32;
        let mut hi_power = 0u32;

        for level in 0..SMT_DATA_DEPTH {
            let offset = row_idx * width;
            let row = &mut values[offset..offset + width];
            let cols: &mut SmtStateShardCols<KoalaBear, W> = borrow_cols_mut(row);

            let path_bit = path.path_bits[level];
            let bit_u32 = u32::from(path_bit);
            let is_hi_region = level >= 30;

            cols.is_real = KoalaBear::ONE;
            cols.table_id = KoalaBear::new(witness.table_id);
            cols.col_id = KoalaBear::new(witness.col_id as u32);
            cols.key.populate(path.key);
            cols.old_val = path.old_val;
            cols.new_val = path.new_val;
            cols.old_is_null = bool_fe(path.old_is_null);
            cols.new_is_null = bool_fe(path.new_is_null);
            cols.read_mult_witness = KoalaBear::ONE;
            cols.write_mult_witness = bool_fe(path.write_mult);
            cols.column_is_empty_old = bool_fe(witness.column_is_empty_old);
            cols.column_is_empty_new = bool_fe(witness.column_is_empty_new);
            cols.column_is_touched = bool_fe(witness.column_is_touched);
            cols.column_old_root = witness.column_old_root.0;
            cols.column_new_root = witness.column_new_root.0;
            cols.path_bit = bool_fe(path_bit);
            cols.is_leaf = bool_fe(level == 0);
            cols.is_root = bool_fe(level + 1 == SMT_DATA_DEPTH);
            cols.is_hi_region = bool_fe(is_hi_region);
            cols.root_mult_witness =
                bool_fe(level + 1 == SMT_DATA_DEPTH && path_idx + 1 == witness.paths.len());

            if level == 0 {
                low_acc = bit_u32;
                low_power = 1;
                hi_acc = 0;
                hi_power = 0;
            } else if level < 30 {
                low_power <<= 1;
                low_acc += bit_u32 * low_power;
            } else if level == 30 {
                hi_power = 1;
                hi_acc = bit_u32;
            } else {
                hi_power <<= 1;
                hi_acc += bit_u32 * hi_power;
            }

            cols.low_key_acc = KoalaBear::new(low_acc);
            cols.low_level_power = KoalaBear::new(low_power);
            cols.hi_key_acc = KoalaBear::new(hi_acc);
            cols.hi_level_power = KoalaBear::new(hi_power);
            cols.switch_level_iz
                .populate(KoalaBear::new(low_power) - KoalaBear::new(LOW_REGION_SWITCH_POWER));
            cols.root_level_iz
                .populate(KoalaBear::new(hi_power) - KoalaBear::new(HI_REGION_ROOT_POWER));
            cols.next_is_new_path
                .populate(if level + 1 == SMT_DATA_DEPTH {
                    KoalaBear::ONE
                } else {
                    KoalaBear::ZERO
                });

            cols.old_leaf_perm_input = old_leaf_input;
            cols.old_leaf_hash = old_leaf_hash.0;
            cols.new_leaf_perm_input = new_leaf_input;
            cols.new_leaf_hash = new_leaf_hash.0;

            let old_sibling = path.old_siblings[level];
            let new_sibling = path.new_siblings[level];
            cols.old_node = old_node.0;
            cols.new_node = new_node.0;
            cols.old_sibling = old_sibling.0;
            cols.new_sibling = new_sibling.0;

            let mut old_perm_input = [KoalaBear::ZERO; 16];
            let mut new_perm_input = [KoalaBear::ZERO; 16];
            for i in 0..DIGEST_WIDTH {
                if path_bit {
                    old_perm_input[i] = old_sibling.0[i];
                    old_perm_input[8 + i] = old_node.0[i];
                    new_perm_input[i] = new_sibling.0[i];
                    new_perm_input[8 + i] = new_node.0[i];
                } else {
                    old_perm_input[i] = old_node.0[i];
                    old_perm_input[8 + i] = old_sibling.0[i];
                    new_perm_input[i] = new_node.0[i];
                    new_perm_input[8 + i] = new_sibling.0[i];
                }
            }
            cols.old_perm_input = old_perm_input;
            cols.new_perm_input = new_perm_input;

            let (_old_rounds, old_out) = poseidon2_permutation(old_perm_input);
            let (_new_rounds, new_out) = poseidon2_permutation(new_perm_input);
            let old_parent = NativeDigest(core::array::from_fn(|i| old_out[i]));
            let new_parent = NativeDigest(core::array::from_fn(|i| new_out[i]));
            cols.old_parent = old_parent.0;
            cols.new_parent = new_parent.0;

            old_node = old_parent;
            new_node = new_parent;
            row_idx += 1;
        }
    }

    for i in num_real..num_rows {
        let offset = i * width;
        let row = &mut values[offset..offset + width];
        let cols: &mut SmtStateShardCols<KoalaBear, W> = borrow_cols_mut(row);
        cols.next_is_new_path.populate(KoalaBear::ZERO);
        cols.switch_level_iz
            .populate(KoalaBear::ZERO - KoalaBear::new(LOW_REGION_SWITCH_POWER));
        cols.root_level_iz
            .populate(KoalaBear::ZERO - KoalaBear::new(HI_REGION_ROOT_POWER));
    }

    RowMajorMatrix::new(values, width)
}

impl<const W: usize> TraceGenerator for SmtStateShardChip<W> {
    type Input = SmtStateWitness<W>;

    fn generate_trace(&self, input: &SmtStateWitness<W>) -> RowMajorMatrix<KoalaBear> {
        generate_smt_state_shard_trace(input)
    }
}

impl<const W: usize> TraceContributor for SmtStateShardChip<W> {
    fn phase(&self) -> TracePhase {
        TracePhase::MEMORY
    }

    fn contribute(
        &self,
        store: &WitnessStore,
        map: &mut TraceMap,
    ) -> Result<(), tabula_core::error::TabulaError> {
        let witness = store.get::<SmtStateWitness<W>>(SMT_STATE_WITNESS_LABEL)?;
        let trace = generate_smt_state_shard_trace(witness);
        map.insert(self.chip_id(), trace);
        Ok(())
    }
}
