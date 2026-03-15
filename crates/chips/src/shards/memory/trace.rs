//! Trace generation for the MemoryShard chip.
//!
//! Converts per-column witness data (sorted rows by key, then tx_index)
//! into a `RowMajorMatrix<KoalaBear>` trace.

use p3_field::PrimeCharacteristicRing;
use p3_koala_bear::KoalaBear;
use p3_matrix::dense::RowMajorMatrix;

use tabula_gadgets::bool_fe;
use tabula_stark::air::columns::borrow_cols_mut;
use tabula_stark::trace::generator::TraceGenerator;

use super::air::MemoryShardChip;
use super::columns::{MemoryShardCols, memory_shard_width};

/// A single row for building the MemoryShard trace.
///
/// Pre-sorted by `(key, tx_index)` within the column.
/// Init rows have `is_init=true` and appear first for each key.
#[derive(Debug, Clone)]
pub struct MemoryShardRow {
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
    pub input_val: Vec<KoalaBear>,
    /// Input is-null flag.
    pub input_is_null: bool,
    /// Output value (same as input for init/read-only; written value for write).
    pub output_val: Vec<KoalaBear>,
    /// Output is-null flag.
    pub output_is_null: bool,
}

/// Generate a MemoryShard trace from pre-sorted rows for a single column.
///
/// `rows` must be sorted by `(key, tx_index)`.
/// Each key must have exactly one init row first, followed by access rows
/// with strictly increasing tx_index.
pub fn generate_memory_shard_trace<const W: usize>(
    table_id: u32,
    col_id: u16,
    rows: &[MemoryShardRow],
) -> RowMajorMatrix<KoalaBear> {
    let width = memory_shard_width::<W>();
    let num_real = rows.len();
    let num_rows = (num_real + 1).next_power_of_two().max(2);
    let mut values = vec![KoalaBear::ZERO; num_rows * width];

    // Pass 1: populate identity, key, values, flags.
    populate_base_columns::<W>(table_id, col_id, rows, width, &mut values);

    // Pass 2: chain tracking (is_last_for_key, has_ever_written).
    populate_chain_tracking::<W>(rows, num_real, width, &mut values);

    // Pass 3: ordering witnesses (key limb IsZero, key ordering, tx_diff).
    populate_ordering_witnesses::<W>(rows, num_real, num_rows, width, &mut values);

    RowMajorMatrix::new(values, width)
}

/// Populate identity, key, values, and flags for all real rows.
fn populate_base_columns<const W: usize>(
    table_id: u32,
    col_id: u16,
    rows: &[MemoryShardRow],
    width: usize,
    values: &mut [KoalaBear],
) {
    for (i, row) in rows.iter().enumerate() {
        assert_eq!(row.input_val.len(), W, "input_val length mismatch");
        assert_eq!(row.output_val.len(), W, "output_val length mismatch");

        let offset = i * width;
        let cols: &mut MemoryShardCols<KoalaBear, W> =
            borrow_cols_mut(&mut values[offset..offset + width]);

        cols.is_real = KoalaBear::ONE;
        cols.table_id = KoalaBear::new(table_id);
        cols.col_id = KoalaBear::new(col_id as u32);
        cols.key.populate(row.key);
        cols.tx_index = KoalaBear::new(row.tx_index);
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
}

/// Compute chain tracking flags: has_ever_written and is_last_for_key.
fn populate_chain_tracking<const W: usize>(
    rows: &[MemoryShardRow],
    num_real: usize,
    width: usize,
    values: &mut [KoalaBear],
) {
    // has_ever_written: forward scan within key chains
    let mut ever_written = false;
    for i in 0..num_real {
        let row = &rows[i];

        let is_new_key = i == 0 || row.key != rows[i - 1].key;
        if is_new_key {
            ever_written = false;
        }

        if row.has_write {
            ever_written = true;
        }

        let offset = i * width;
        let cols: &mut MemoryShardCols<KoalaBear, W> =
            borrow_cols_mut(&mut values[offset..offset + width]);
        cols.has_ever_written = bool_fe(ever_written);
    }

    // is_last_for_key: look-ahead
    for i in 0..num_real {
        let is_last = i + 1 >= num_real || rows[i].key != rows[i + 1].key;

        let offset = i * width;
        let cols: &mut MemoryShardCols<KoalaBear, W> =
            borrow_cols_mut(&mut values[offset..offset + width]);
        cols.is_last_for_key = bool_fe(is_last);
    }
}

/// Populate ordering witnesses: key limb IsZero, key ordering, tx_diff.
///
/// Iterates ALL rows including padding — IsZero gadgets are unconditionally
/// constrained in the AIR and must have valid witnesses everywhere.
fn populate_ordering_witnesses<const W: usize>(
    rows: &[MemoryShardRow],
    num_real: usize,
    num_rows: usize,
    width: usize,
    values: &mut [KoalaBear],
) {
    for i in 0..num_rows {
        let next_idx = (i + 1) % num_rows;

        let cur_key = if i < num_real { rows[i].key } else { 0 };
        let next_key = if next_idx < num_real {
            rows[next_idx].key
        } else {
            0
        };

        let limb0_diff = KoalaBear::new((next_key & 0x3FFF_FFFF) as u32)
            - KoalaBear::new((cur_key & 0x3FFF_FFFF) as u32);
        let limb1_diff = KoalaBear::new(((next_key >> 30) & 0x3FFF_FFFF) as u32)
            - KoalaBear::new(((cur_key >> 30) & 0x3FFF_FFFF) as u32);
        let limb2_diff =
            KoalaBear::new((next_key >> 60) as u32) - KoalaBear::new((cur_key >> 60) as u32);

        let cur_offset = i * width;
        let cols: &mut MemoryShardCols<KoalaBear, W> =
            borrow_cols_mut(&mut values[cur_offset..cur_offset + width]);

        cols.r_limb0_iz.populate(limb0_diff);
        cols.r_limb1_iz.populate(limb1_diff);
        cols.r_limb2_iz.populate(limb2_diff);

        let is_real_cur = i < num_real;
        let is_real_next = next_idx < num_real;

        if is_real_cur && is_real_next && cur_key != next_key {
            cols.key_ordering.populate(cur_key, next_key);
        }

        if is_real_cur && is_real_next && cur_key == next_key {
            let cur_init = rows[i].is_init;
            let next_init = rows[next_idx].is_init;
            if !cur_init && !next_init {
                let cur_tx = rows[i].tx_index;
                let next_tx = rows[next_idx].tx_index;
                debug_assert!(
                    next_tx > cur_tx,
                    "tx_index must strictly increase: {cur_tx} -> {next_tx}"
                );
                cols.tx_diff = KoalaBear::new(next_tx - cur_tx - 1);
            }
        }
    }
}

// ── TraceGenerator impl ─────────────────────────────────────────────────────

/// Input bundle for `MemoryShardChip` trace generation.
pub struct MemoryShardInput {
    /// Pre-sorted rows for the column.
    pub rows: Vec<MemoryShardRow>,
}

impl<const W: usize> TraceGenerator for MemoryShardChip<W> {
    type Input = MemoryShardInput;

    fn generate_trace(&self, input: &MemoryShardInput) -> RowMajorMatrix<KoalaBear> {
        generate_memory_shard_trace::<W>(self.table_id(), self.col_id(), &input.rows)
    }
}

// ── TraceContributor impl ──────────────────────────────────────────────────

use crate::ChipSpec;
use tabula_core::error::TabulaError;
use tabula_core::{ColId, TableId};
use tabula_stark::trace::contributor::{TraceContributor, TracePhase, WitnessStore};
use tabula_stark::trace::trace_map::TraceMap;

use super::super::ssmc::{SSMC_WITNESS_LABEL, SsmcWitness};

impl<const W: usize> TraceContributor for MemoryShardChip<W> {
    fn phase(&self) -> TracePhase {
        TracePhase::MEMORY
    }

    fn contribute(&self, store: &WitnessStore, map: &mut TraceMap) -> Result<(), TabulaError> {
        let witness = store.get::<SsmcWitness>(SSMC_WITNESS_LABEL)?;
        let col_data = witness
            .get(TableId(self.table_id()), ColId(self.col_id()))
            .ok_or_else(|| TabulaError::ProofError {
                phase: "memory_shard_trace",
                detail: format!(
                    "no SSMC witness data for ({}, {})",
                    self.table_id(),
                    self.col_id()
                ),
            })?;
        let trace =
            generate_memory_shard_trace::<W>(self.table_id(), self.col_id(), &col_data.memory_rows);
        map.insert(self.chip_id(), trace);
        Ok(())
    }
}
