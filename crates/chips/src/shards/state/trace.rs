//! Trace generation for the StateShard chip.
//!
//! Converts per-column witness data (sorted by key) into a
//! `RowMajorMatrix<KoalaBear>` trace with two parallel hash chains.

use std::collections::BTreeMap;

use p3_field::PrimeCharacteristicRing;
use p3_koala_bear::KoalaBear;
use p3_matrix::dense::RowMajorMatrix;

use tabula_gadgets::bool_fe;
use tabula_stark::air::columns::borrow_cols_mut;
use tabula_stark::trace::generator::TraceGenerator;
use tabula_types::{NativeKeyPayload, zero_key_payload};

use super::air::StateShardChip;
use super::columns::{StateShardCols, state_shard_width};
use crate::execution::{native_key_payload_prefix3, native_key_payload_to_u64};

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
    pub fn encode(self) -> (bool, bool) {
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

/// A single row for building the StateShard trace.
///
/// Pre-sorted by `key` within the column.
#[derive(Debug, Clone)]
pub struct StateShardRow {
    /// Native committed-key payload.
    pub key: NativeKeyPayload,
    /// True if this is a gap row (non-membership proof).
    pub is_gap: bool,
    /// Source type (meaningful only for entry rows).
    pub source: EntrySource,
    /// Old value (zeros for write_only/gap).
    pub old_val: Vec<KoalaBear>,
    /// New value (zeros for delete/gap).
    pub new_val: Vec<KoalaBear>,
    /// Whether this column is touched in the batch.
    pub segment_is_touched: bool,
    /// Precomputed old hash chain accumulator.
    pub old_hash_acc: [KoalaBear; 8],
    /// Precomputed new hash chain accumulator.
    pub new_hash_acc: [KoalaBear; 8],
    /// Multiplicity for BaseStateEntry bus receive.
    pub read_mult: bool,
    /// Multiplicity for CoalescedWrite bus receive.
    pub write_mult: bool,
    /// Previous old-state entry key, or zero payload when absent.
    pub prev_old_key: NativeKeyPayload,
    /// Next old-state entry key, or zero payload when absent.
    pub next_old_key: NativeKeyPayload,
}

/// Generate a StateShard trace from pre-sorted rows for a single column.
///
/// `rows` must be sorted by `key`. Keys must be strictly increasing.
pub fn generate_state_shard_trace<const W: usize>(
    table_id: u32,
    col_id: u16,
    rows: &[StateShardRow],
) -> RowMajorMatrix<KoalaBear> {
    generate_state_shard_trace_with_anchor_mults::<W>(table_id, col_id, rows, &BTreeMap::new())
}

/// Generate a StateShard trace with explicit property-anchor multiplicities.
///
/// `anchor_mults` is keyed by old-entry row key and controls how many times a
/// row sends on the internal `SSMC_OLD_ENTRY` bus for scheme-owned property
/// verification. Rows absent from the map send zero multiplicity.
pub fn generate_state_shard_trace_with_anchor_mults<const W: usize>(
    table_id: u32,
    col_id: u16,
    rows: &[StateShardRow],
    anchor_mults: &BTreeMap<NativeKeyPayload, u32>,
) -> RowMajorMatrix<KoalaBear> {
    debug_assert!(
        rows.windows(2).all(|w| w[0].key < w[1].key),
        "rows must be sorted by key"
    );

    let width = state_shard_width::<W>();
    let num_real = rows.len();
    let num_rows = (num_real + 1).next_power_of_two().max(2);
    let mut values = vec![KoalaBear::ZERO; num_rows * width];
    let (prev_old_keys, next_old_keys) = derive_old_neighbor_keys(rows);

    // Pass 1: populate base columns, hash chains, chain tracking.
    populate_base_and_chains::<W>(
        StateShardTraceLayout {
            table_id,
            col_id,
            width,
        },
        rows,
        &prev_old_keys,
        &next_old_keys,
        anchor_mults,
        &mut values,
    );

    // Pass 2: chain tracking flags (look-ahead).
    populate_chain_tracking_flags::<W>(rows, num_real, width, &mut values);

    // Pass 3: key ordering witnesses.
    populate_ordering_witnesses::<W>(rows, num_real, num_rows, width, &mut values);

    RowMajorMatrix::new(values, width)
}

fn derive_old_neighbor_keys(
    rows: &[StateShardRow],
) -> (Vec<NativeKeyPayload>, Vec<NativeKeyPayload>) {
    let mut prev_old_keys = vec![zero_key_payload(); rows.len()];
    let mut next_old_keys = vec![zero_key_payload(); rows.len()];
    let mut last_old_key = zero_key_payload();
    for (idx, row) in rows.iter().enumerate() {
        prev_old_keys[idx] = last_old_key;
        if !row.is_gap && row.source.in_old() {
            last_old_key = row.key;
        }
    }

    let mut upcoming_old_key = zero_key_payload();
    for (idx, row) in rows.iter().enumerate().rev() {
        next_old_keys[idx] = upcoming_old_key;
        if !row.is_gap && row.source.in_old() {
            upcoming_old_key = row.key;
        }
    }

    (prev_old_keys, next_old_keys)
}

fn payload_to_u64(payload: &NativeKeyPayload) -> u64 {
    native_key_payload_to_u64(payload)
}

#[derive(Clone, Copy)]
struct StateShardTraceLayout {
    table_id: u32,
    col_id: u16,
    width: usize,
}

/// Populate base columns, hash chain inputs, and forward-scan flags.
fn populate_base_and_chains<const W: usize>(
    layout: StateShardTraceLayout,
    rows: &[StateShardRow],
    prev_old_keys: &[NativeKeyPayload],
    next_old_keys: &[NativeKeyPayload],
    anchor_mults: &BTreeMap<NativeKeyPayload, u32>,
    values: &mut [KoalaBear],
) {
    let mut seen_old = false;
    let mut seen_new = false;
    let mut seen_write = false;
    let mut prev_old_hash_acc: Option<[KoalaBear; 8]> = None;
    let mut prev_new_hash_acc: Option<[KoalaBear; 8]> = None;

    for (i, row) in rows.iter().enumerate() {
        assert_eq!(row.old_val.len(), W, "old_val length mismatch");
        assert_eq!(row.new_val.len(), W, "new_val length mismatch");

        let offset = i * layout.width;
        let cols: &mut StateShardCols<KoalaBear, W> =
            borrow_cols_mut(&mut values[offset..offset + layout.width]);

        cols.is_real = KoalaBear::ONE;
        cols.table_id = KoalaBear::new(layout.table_id);
        cols.col_id = KoalaBear::new(layout.col_id as u32);
        cols.key
            .populate_payload(&native_key_payload_prefix3(&row.key));

        cols.is_gap = bool_fe(row.is_gap);
        if !row.is_gap {
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
        cols.property_anchor_mult = KoalaBear::new(if !row.is_gap && row.source.in_old() {
            anchor_mults.get(&row.key).copied().unwrap_or(0)
        } else {
            0
        });
        cols.prev_old_key
            .populate_payload(&native_key_payload_prefix3(&prev_old_keys[i]));
        cols.next_old_key
            .populate_payload(&native_key_payload_prefix3(&next_old_keys[i]));

        let in_old = !row.is_gap && row.source.in_old();
        let in_new = !row.is_gap && row.source.in_new();
        let in_write = !row.is_gap && row.source.in_write();
        seen_write |= in_write;
        cols.write_seen_prefix = bool_fe(seen_write);

        // Old chain tracking
        cols.has_prev_old_entry = bool_fe(seen_old);
        if in_old {
            if !seen_old {
                cols.old_hash_chain.populate_first(
                    layout.table_id,
                    layout.col_id as u32,
                    payload_to_u64(&row.key),
                    &row.old_val,
                );
            } else {
                cols.old_hash_chain.populate_continuation(
                    prev_old_hash_acc
                        .as_ref()
                        .expect("continuation must have prev"),
                    payload_to_u64(&row.key),
                    &row.old_val,
                );
            }
            prev_old_hash_acc = Some(row.old_hash_acc);
            seen_old = true;
        }

        // New chain tracking
        cols.has_prev_new_entry = bool_fe(seen_new);
        if in_new {
            if !seen_new {
                cols.new_hash_chain.populate_first(
                    layout.table_id,
                    layout.col_id as u32,
                    payload_to_u64(&row.key),
                    &row.new_val,
                );
            } else {
                cols.new_hash_chain.populate_continuation(
                    prev_new_hash_acc
                        .as_ref()
                        .expect("continuation must have prev"),
                    payload_to_u64(&row.key),
                    &row.new_val,
                );
            }
            prev_new_hash_acc = Some(row.new_hash_acc);
            seen_new = true;
        }
    }
}

/// Compute chain tracking flags requiring look-ahead.
fn populate_chain_tracking_flags<const W: usize>(
    rows: &[StateShardRow],
    num_real: usize,
    width: usize,
    values: &mut [KoalaBear],
) {
    // Backward pass: find last old/new entries.
    let mut found_last_old = false;
    let mut found_last_new = false;

    for i in (0..num_real).rev() {
        let row = &rows[i];
        let in_old = !row.is_gap && row.source.in_old();
        let in_new = !row.is_gap && row.source.in_new();

        let offset = i * width;
        let cols: &mut StateShardCols<KoalaBear, W> =
            borrow_cols_mut(&mut values[offset..offset + width]);

        if in_old && !found_last_old {
            cols.is_last_old_entry = KoalaBear::ONE;
            found_last_old = true;
        }

        if in_new && !found_last_new {
            cols.is_last_new_entry = KoalaBear::ONE;
            found_last_new = true;
        }
    }

    // Forward pass: past_last_old_entry.
    let mut past_last_old = false;
    for i in 0..num_real {
        let offset = i * width;
        let cols: &mut StateShardCols<KoalaBear, W> =
            borrow_cols_mut(&mut values[offset..offset + width]);

        if cols.is_last_old_entry == KoalaBear::ONE {
            cols.past_last_old_entry = KoalaBear::ZERO;
            past_last_old = true;
        } else {
            cols.past_last_old_entry = bool_fe(past_last_old);
        }
    }
}

/// Populate key ordering witnesses.
fn populate_ordering_witnesses<const W: usize>(
    rows: &[StateShardRow],
    num_real: usize,
    num_rows: usize,
    width: usize,
    values: &mut [KoalaBear],
) {
    for i in 0..num_rows {
        let next_idx = (i + 1) % num_rows;

        let is_real_cur = i < num_real;
        let is_real_next = next_idx < num_real;

        if is_real_cur && is_real_next {
            let cur_key = rows[i].key;
            let next_key = rows[next_idx].key;
            let offset = i * width;
            let cols: &mut StateShardCols<KoalaBear, W> =
                borrow_cols_mut(&mut values[offset..offset + width]);
            cols.key_ordering.populate_payload(
                &native_key_payload_prefix3(&cur_key),
                &native_key_payload_prefix3(&next_key),
            );
        }
    }
}

/// Input bundle for `StateShardChip` trace generation.
pub struct StateShardInput {
    /// Pre-sorted rows for the column.
    pub rows: Vec<StateShardRow>,
}

impl<const W: usize> TraceGenerator for StateShardChip<W> {
    type Input = StateShardInput;

    fn generate_trace(&self, input: &StateShardInput) -> RowMajorMatrix<KoalaBear> {
        generate_state_shard_trace::<W>(self.table_id(), self.col_id(), &input.rows)
    }
}

// ── TraceContributor impl ──────────────────────────────────────────────────

use crate::ChipSpec;
use tabula_core::error::TabulaError;
use tabula_core::{ColId, TableId};
use tabula_stark::trace::contributor::{TraceContributor, TracePhase, WitnessStore};
use tabula_stark::trace::trace_map::TraceMap;

use super::super::property::trace::{
    PROPERTY_READ_WITNESS_LABEL, PropertyReadRecord, ssmc_property_anchor_multiplicities,
};
use super::super::ssmc::{SSMC_WITNESS_LABEL, SsmcWitness};

impl<const W: usize> TraceContributor for StateShardChip<W> {
    fn phase(&self) -> TracePhase {
        TracePhase::MEMORY
    }

    fn contribute(&self, store: &WitnessStore, map: &mut TraceMap) -> Result<(), TabulaError> {
        let witness = store.get::<SsmcWitness>(SSMC_WITNESS_LABEL)?;
        let col_data = witness
            .get(TableId(self.table_id()), ColId(self.col_id()))
            .ok_or_else(|| TabulaError::ProofError {
                phase: "state_shard_trace",
                detail: format!(
                    "no SSMC witness data for ({}, {})",
                    self.table_id(),
                    self.col_id()
                ),
            })?;
        let property_claims = store
            .get::<Vec<PropertyReadRecord>>(PROPERTY_READ_WITNESS_LABEL)
            .cloned()
            .unwrap_or_default();
        let anchor_mults = ssmc_property_anchor_multiplicities::<W>(&property_claims, col_data)?;
        let trace = generate_state_shard_trace_with_anchor_mults::<W>(
            self.table_id(),
            self.col_id(),
            &col_data.state_rows,
            &anchor_mults,
        );
        map.insert(self.chip_id(), trace);
        Ok(())
    }
}
