//! Trace builder orchestrator (M12 entry).
//!
//! Converts `BatchWitness` into canonical chip traces via one entrypoint,
//! enforcing a shared E-Trace/contract boundary.

use std::collections::{BTreeMap, BTreeSet};

use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;
use p3_matrix::dense::RowMajorMatrix;

use tabula_commitment::{ColumnState, FieldHasher, NativeDigest};
use tabula_core::error::TabulaError;
use tabula_core::{ColId, RowKey, TableId};

use crate::air::chips::column_meta::trace::generate_column_meta_trace;
use crate::air::chips::inter_tx_order::trace::{InterTxOrderRow, generate_inter_tx_order_trace};
use crate::air::chips::poseidon::constants::poseidon2_permutation;
use crate::air::chips::state_column::trace::{
    EntrySource, StateColumnRow, generate_state_column_trace,
};
use crate::witness::{BatchWitness, ColumnWitness};

/// Output of the M12 trace builder.
#[derive(Debug, Clone)]
pub struct ProofTraceBundle<const W: usize> {
    /// InterTxOrder rows (pre-trace representation).
    pub inter_tx_rows: Vec<InterTxOrderRow>,
    /// StateColumn rows (pre-trace representation).
    pub state_rows: Vec<StateColumnRow>,
    /// ColumnMeta empty-read multiplicity map.
    pub empty_read_mults: BTreeMap<(TableId, ColId), u32>,
    /// InterTxOrder chip trace.
    pub inter_tx_trace: RowMajorMatrix<BabyBear>,
    /// StateColumn chip trace.
    pub state_trace: RowMajorMatrix<BabyBear>,
    /// ColumnMeta chip trace.
    pub column_meta_trace: RowMajorMatrix<BabyBear>,
}

/// Build all memory/metadata traces from one `BatchWitness`.
pub fn build_trace_bundle<H, const W: usize>(
    witness: &BatchWitness<H>,
) -> Result<ProofTraceBundle<W>, TabulaError>
where
    H: FieldHasher<F = BabyBear, Digest = NativeDigest>,
{
    let mut inter_tx_rows = Vec::new();
    let mut state_rows = Vec::new();

    for column in &witness.columns {
        inter_tx_rows.extend(build_inter_tx_rows::<H, W>(column)?);
        state_rows.extend(build_state_rows::<H, W>(column)?);
    }

    sort_inter_tx_rows(&mut inter_tx_rows);
    sort_state_rows(&mut state_rows);

    // Populate running hash accumulators required by StateColumn constraints.
    populate_state_chain_accumulators::<W>(&mut state_rows);

    let empty_read_mults = build_empty_read_mults::<H>(witness);
    let empty_read_mults_for_trace: BTreeMap<(u32, u16), u32> = empty_read_mults
        .iter()
        .map(|(&(table, col), &count)| ((table.0, col.0), count))
        .collect();

    let inter_tx_trace = generate_inter_tx_order_trace::<W>(&inter_tx_rows);
    let state_trace = generate_state_column_trace::<W>(&state_rows);
    let column_meta_trace =
        generate_column_meta_trace(&witness.column_metas, &empty_read_mults_for_trace);

    Ok(ProofTraceBundle {
        inter_tx_rows,
        state_rows,
        empty_read_mults,
        inter_tx_trace,
        state_trace,
        column_meta_trace,
    })
}

fn build_inter_tx_rows<H, const W: usize>(
    column: &ColumnWitness<H>,
) -> Result<Vec<InterTxOrderRow>, TabulaError>
where
    H: FieldHasher<F = BabyBear, Digest = NativeDigest>,
{
    let mut keys = BTreeSet::new();

    let mut init_by_key: BTreeMap<RowKey, (Vec<BabyBear>, bool)> = BTreeMap::new();
    for init in &column.init_rows {
        if init.value_fes.len() != W {
            return Err(TabulaError::ConsistencyError(format!(
                "init row width mismatch for ({:?}, {:?}): expected {}, got {}",
                column.table,
                column.col,
                W,
                init.value_fes.len()
            )));
        }
        keys.insert(init.key.row);
        init_by_key.insert(init.key.row, (init.value_fes.clone(), init.val_is_null));
    }

    let mut by_key_tx: BTreeMap<RowKey, BTreeMap<u32, Vec<&crate::witness::AccessRow>>> =
        BTreeMap::new();
    for access in &column.access_rows {
        if access.value_fes.len() != W {
            return Err(TabulaError::ConsistencyError(format!(
                "access row width mismatch for ({:?}, {:?}): expected {}, got {}",
                column.table,
                column.col,
                W,
                access.value_fes.len()
            )));
        }
        keys.insert(access.key.row);
        by_key_tx
            .entry(access.key.row)
            .or_default()
            .entry(access.tx_index)
            .or_default()
            .push(access);
    }

    let zero = vec![BabyBear::ZERO; W];
    let mut rows = Vec::new();

    for key in keys {
        let (init_val, init_is_null) = init_by_key
            .get(&key)
            .cloned()
            .unwrap_or_else(|| (zero.clone(), true));

        rows.push(InterTxOrderRow {
            table_id: column.table.0,
            col_id: column.col.0,
            key: key.0,
            tx_index: 0,
            is_init: true,
            has_read: false,
            has_write: false,
            input_val: init_val.clone(),
            input_is_null: init_is_null,
            output_val: init_val.clone(),
            output_is_null: init_is_null,
        });

        let mut current_val = init_val;
        let mut current_is_null = init_is_null;

        if let Some(tx_map) = by_key_tx.get(&key) {
            for (tx_index, events) in tx_map {
                let mut ordered = events.clone();
                ordered.sort_by_key(|e| e.effect_ordinal_in_tx);

                let has_read = ordered.iter().any(|e| !e.is_write);
                let has_write = ordered.iter().any(|e| e.is_write);

                let (input_val, input_is_null) =
                    if let Some(first_read) = ordered.iter().find(|e| !e.is_write) {
                        (first_read.value_fes.clone(), first_read.val_is_null)
                    } else {
                        (current_val.clone(), current_is_null)
                    };

                let (output_val, output_is_null) =
                    if let Some(last_write) = ordered.iter().rev().find(|e| e.is_write) {
                        (last_write.value_fes.clone(), last_write.val_is_null)
                    } else {
                        (input_val.clone(), input_is_null)
                    };

                rows.push(InterTxOrderRow {
                    table_id: column.table.0,
                    col_id: column.col.0,
                    key: key.0,
                    tx_index: *tx_index,
                    is_init: false,
                    has_read,
                    has_write,
                    input_val,
                    input_is_null,
                    output_val: output_val.clone(),
                    output_is_null,
                });

                current_val = output_val;
                current_is_null = output_is_null;
            }
        }
    }

    Ok(rows)
}

fn build_state_rows<H, const W: usize>(
    column: &ColumnWitness<H>,
) -> Result<Vec<StateColumnRow>, TabulaError>
where
    H: FieldHasher<F = BabyBear, Digest = NativeDigest>,
{
    let old_entries = ssmc_entries::<H>(&column.old_state)?;
    let new_entries = ssmc_entries::<H>(&column.new_state)?;

    let mut write_keys: BTreeSet<RowKey> = BTreeSet::new();
    for access in &column.access_rows {
        if access.is_write {
            write_keys.insert(access.key.row);
        }
    }

    let mut keys = BTreeSet::new();
    keys.extend(old_entries.keys().copied());
    keys.extend(new_entries.keys().copied());
    keys.extend(write_keys.iter().copied());

    let mut rows = Vec::new();
    for key in keys {
        let old_opt = old_entries.get(&key).cloned();
        let new_opt = new_entries.get(&key).cloned();
        let in_write = write_keys.contains(&key);

        let source = match (old_opt.as_ref(), new_opt.as_ref()) {
            (Some(_), Some(_)) if in_write => EntrySource::Both,
            (Some(_), Some(_)) => EntrySource::OldOnly,
            (None, Some(_)) => EntrySource::WriteOnly,
            (Some(_), None) => EntrySource::Delete,
            (None, None) => continue,
        };

        let old_val = old_opt.unwrap_or_else(|| vec![BabyBear::ZERO; W]);
        let new_val = new_opt.unwrap_or_else(|| vec![BabyBear::ZERO; W]);

        if old_val.len() != W || new_val.len() != W {
            return Err(TabulaError::ConsistencyError(format!(
                "state row width mismatch for ({:?}, {:?}) key {}",
                column.table, column.col, key.0
            )));
        }

        rows.push(StateColumnRow {
            table_id: column.table.0,
            col_id: column.col.0,
            key: key.0,
            is_gap: false,
            source,
            old_val,
            new_val,
            segment_is_touched: column.meta.is_touched,
            old_hash_acc: [BabyBear::ZERO; 8],
            new_hash_acc: [BabyBear::ZERO; 8],
            read_mult: true,
            write_mult: in_write,
        });
    }

    Ok(rows)
}

fn ssmc_entries<H>(state: &ColumnState<H>) -> Result<BTreeMap<RowKey, Vec<BabyBear>>, TabulaError>
where
    H: FieldHasher<F = BabyBear, Digest = NativeDigest>,
{
    match state {
        ColumnState::Ssmc(list) => Ok(list
            .entries()
            .iter()
            .map(|entry| (entry.key, entry.value.clone()))
            .collect()),
        ColumnState::Smt(_) => Err(TabulaError::ConsistencyError(
            "trace_builder currently supports SSMC-backed columns only".to_string(),
        )),
    }
}

fn sort_inter_tx_rows(rows: &mut [InterTxOrderRow]) {
    rows.sort_by(|a, b| {
        (a.table_id, a.col_id, a.key)
            .cmp(&(b.table_id, b.col_id, b.key))
            .then_with(|| a.tx_index.cmp(&b.tx_index))
            // For same (t,c,key,tx), init must come first.
            .then_with(|| b.is_init.cmp(&a.is_init))
    });
}

fn sort_state_rows(rows: &mut [StateColumnRow]) {
    rows.sort_by_key(|r| (r.table_id, r.col_id, r.key));
}

fn build_empty_read_mults<H>(witness: &BatchWitness<H>) -> BTreeMap<(TableId, ColId), u32>
where
    H: FieldHasher<F = BabyBear, Digest = NativeDigest>,
{
    let mut mults = BTreeMap::new();
    for column in &witness.columns {
        if !column.meta.is_empty_old {
            continue;
        }
        let cnt = column
            .access_rows
            .iter()
            .filter(|r| !r.is_write && r.val_is_null)
            .count() as u32;
        if cnt > 0 {
            mults.insert((column.table, column.col), cnt);
        }
    }
    mults
}

fn populate_state_chain_accumulators<const W: usize>(rows: &mut [StateColumnRow]) {
    let mut i = 0;
    while i < rows.len() {
        let (t, c) = (rows[i].table_id, rows[i].col_id);
        let mut j = i;
        while j < rows.len() && rows[j].table_id == t && rows[j].col_id == c {
            j += 1;
        }

        let mut prev_old: Option<[BabyBear; 8]> = None;
        let mut prev_new: Option<[BabyBear; 8]> = None;

        for row in rows[i..j].iter_mut() {
            if !row.is_gap && row.source.in_old() {
                let acc = match prev_old {
                    Some(prev) => hash_chain_step_cont::<W>(prev, row.key, &row.old_val),
                    None => hash_chain_step_first::<W>(t, c, row.key, &row.old_val),
                };
                row.old_hash_acc = acc;
                prev_old = Some(acc);
            } else if let Some(prev) = prev_old {
                row.old_hash_acc = prev;
            }

            if !row.is_gap && row.source.in_new() {
                let acc = match prev_new {
                    Some(prev) => hash_chain_step_cont::<W>(prev, row.key, &row.new_val),
                    None => hash_chain_step_first::<W>(t, c, row.key, &row.new_val),
                };
                row.new_hash_acc = acc;
                prev_new = Some(acc);
            } else if let Some(prev) = prev_new {
                row.new_hash_acc = prev;
            }
        }

        i = j;
    }
}

fn hash_chain_step_first<const W: usize>(
    table_id: u32,
    col_id: u16,
    key: u64,
    value: &[BabyBear],
) -> [BabyBear; 8] {
    let key_limbs = decompose_u64(key);
    let mut input = [BabyBear::ZERO; 16];
    input[1] = BabyBear::new(table_id);
    input[2] = BabyBear::new(col_id as u32);
    input[3] = key_limbs[0];
    input[4] = key_limbs[1];
    input[5] = key_limbs[2];
    for (idx, v) in value.iter().enumerate().take(W) {
        input[6 + idx] = *v;
    }
    let (_, out) = poseidon2_permutation(input);
    core::array::from_fn(|i| out[i])
}

fn hash_chain_step_cont<const W: usize>(
    prev: [BabyBear; 8],
    key: u64,
    value: &[BabyBear],
) -> [BabyBear; 8] {
    let key_limbs = decompose_u64(key);
    let mut input = [BabyBear::ZERO; 16];
    input[..8].copy_from_slice(&prev);
    input[8] = key_limbs[0];
    input[9] = key_limbs[1];
    input[10] = key_limbs[2];
    for (idx, v) in value.iter().enumerate().take(W) {
        input[11 + idx] = *v;
    }
    let (_, out) = poseidon2_permutation(input);
    core::array::from_fn(|i| out[i])
}

fn decompose_u64(v: u64) -> [BabyBear; 3] {
    const MASK_30: u64 = (1u64 << 30) - 1;
    [
        BabyBear::new((v & MASK_30) as u32),
        BabyBear::new(((v >> 30) & MASK_30) as u32),
        BabyBear::new((v >> 60) as u32),
    ]
}
