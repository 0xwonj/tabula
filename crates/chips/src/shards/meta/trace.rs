//! Trace generation for the MetaShard chip.
//!
//! Converts a single `MetaShardRow` into a `RowMajorMatrix<BabyBear>` trace.
//! Each MetaShard has at most 1 real row (one `(t, c)` column metadata entry).

use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;
use p3_matrix::dense::RowMajorMatrix;

use tabula_commitment::{DOMAIN_LEAF, NativeDigest};
use tabula_gadgets::bool_fe;
use tabula_stark::air::columns::borrow_cols_mut;

use crate::poseidon::constants::poseidon2_permutation;

use super::columns::{META_SHARD_WIDTH, MetaShardCols};

/// A single row of MetaShard input data.
///
/// Does not include `table_id`/`col_id` (the chip carries those) or
/// `scheme_tag` (passed separately to trace generation).
#[derive(Debug, Clone)]
pub struct MetaShardRow {
    /// Commitment before the batch (8 FE).
    pub com_old: NativeDigest,
    /// Commitment after the batch (8 FE).
    pub com_new: NativeDigest,
    /// Column was empty before the batch.
    pub is_empty_old: bool,
    /// Column is empty after the batch.
    pub is_empty_new: bool,
    /// Column was modified in this batch.
    pub is_touched: bool,
    /// Number of Execution empty-col reads targeting this `(t,c)`.
    pub empty_read_count: u32,
}

/// Generate a MetaShard trace from an optional row.
///
/// `table_id` and `col_id` are baked into every real row as constant identity.
/// `scheme_tag` is used for leaf digest domain separation.
/// If `row` is `None`, the trace contains only padding (all zeros).
pub fn generate_meta_shard_trace(
    table_id: u32,
    col_id: u16,
    scheme_tag: u16,
    row: Option<&MetaShardRow>,
) -> RowMajorMatrix<BabyBear> {
    let width = META_SHARD_WIDTH;
    // Minimum 2 rows for Plonky3 transition constraints.
    let num_rows = 2;
    let mut values = vec![BabyBear::ZERO; num_rows * width];

    if let Some(r) = row {
        let cols: &mut MetaShardCols<BabyBear> = borrow_cols_mut(&mut values[0..width]);

        cols.is_real = BabyBear::ONE;
        cols.table_id = BabyBear::new(table_id);
        cols.col_id = BabyBear::new(col_id as u32);
        cols.com_old = r.com_old.0;
        cols.com_new = r.com_new.0;
        cols.is_empty_old = bool_fe(r.is_empty_old);
        cols.is_empty_new = bool_fe(r.is_empty_new);
        cols.is_touched = bool_fe(r.is_touched);
        cols.empty_read_mult = BabyBear::new(r.empty_read_count);

        // Com_empty verification
        let has_empty = r.is_empty_old || r.is_empty_new;
        cols.has_empty_check = bool_fe(has_empty);
        if has_empty {
            let mut perm_input = [BabyBear::ZERO; 16];
            perm_input[1] = BabyBear::new(table_id);
            perm_input[2] = BabyBear::new(col_id as u32);
            cols.empty_perm_input = perm_input;

            let (_rounds, perm_output_full) = poseidon2_permutation(perm_input);
            cols.empty_perm_output = core::array::from_fn(|j| perm_output_full[j]);
        }

        // Leaf digest
        {
            let tag_fe = BabyBear::new(scheme_tag as u32);

            // Old leaf: [0x10, t, c, tag, 0,0,0,0, com_old[8]]
            let mut leaf_input_old = [BabyBear::ZERO; 16];
            leaf_input_old[0] = BabyBear::new(DOMAIN_LEAF);
            leaf_input_old[1] = BabyBear::new(table_id);
            leaf_input_old[2] = BabyBear::new(col_id as u32);
            leaf_input_old[3] = tag_fe;
            leaf_input_old[8..16].copy_from_slice(&r.com_old.0);
            cols.leaf_perm_input_old = leaf_input_old;
            let (_rounds, perm_out_old) = poseidon2_permutation(leaf_input_old);
            cols.leaf_digest_old = core::array::from_fn(|j| perm_out_old[j]);

            // New leaf: [0x10, t, c, tag, 0,0,0,0, com_new[8]]
            let mut leaf_input_new = [BabyBear::ZERO; 16];
            leaf_input_new[0] = BabyBear::new(DOMAIN_LEAF);
            leaf_input_new[1] = BabyBear::new(table_id);
            leaf_input_new[2] = BabyBear::new(col_id as u32);
            leaf_input_new[3] = tag_fe;
            leaf_input_new[8..16].copy_from_slice(&r.com_new.0);
            cols.leaf_perm_input_new = leaf_input_new;
            let (_rounds, perm_out_new) = poseidon2_permutation(leaf_input_new);
            cols.leaf_digest_new = core::array::from_fn(|j| perm_out_new[j]);
        }
    }

    RowMajorMatrix::new(values, width)
}

// ── TraceGenerator impl ─────────────────────────────────────────────────────

use tabula_stark::trace::TraceGenerator;

impl TraceGenerator for super::air::MetaShardChip {
    type Input = Option<MetaShardRow>;

    fn generate_trace(&self, input: &Option<MetaShardRow>) -> RowMajorMatrix<BabyBear> {
        generate_meta_shard_trace(
            self.table_id(),
            self.col_id(),
            self.scheme_tag(),
            input.as_ref(),
        )
    }
}

// ── TraceContributor impl ──────────────────────────────────────────────────

use crate::ChipSpec;
use tabula_core::error::TabulaError;
use tabula_core::{ColId, TableId};
use tabula_stark::trace::contributor::{TraceContributor, TracePhase, WitnessStore};
use tabula_stark::trace::trace_map::TraceMap;

use super::super::ssmc::{SSMC_WITNESS_LABEL, SsmcWitness};

impl TraceContributor for super::air::MetaShardChip {
    fn phase(&self) -> TracePhase {
        TracePhase::MEMORY
    }

    fn contribute(&self, store: &WitnessStore, map: &mut TraceMap) -> Result<(), TabulaError> {
        let witness = store.get::<SsmcWitness>(SSMC_WITNESS_LABEL)?;
        let meta_row = witness
            .get(TableId(self.table_id()), ColId(self.col_id()))
            .and_then(|data| data.meta_row.clone());
        let trace = generate_meta_shard_trace(
            self.table_id(),
            self.col_id(),
            self.scheme_tag(),
            meta_row.as_ref(),
        );
        map.insert(self.chip_id(), trace);
        Ok(())
    }
}
