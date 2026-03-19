use std::collections::{BTreeMap, BTreeSet};

use p3_field::PrimeCharacteristicRing;
use p3_koala_bear::KoalaBear;

use tabula_chips::shards::memory::trace::MemoryShardRow;
use tabula_core::RowKey;
use tabula_core::error::TabulaError;
use tabula_core::{ColId, TableId};

use crate::witness::{AccessRow, InitRow};

/// A single row for building inter-tx ordering data.
///
/// Pre-sorted by `(table_id, col_id, key, tx_index)`.
/// Init rows have `is_init=true` and appear first for each key.
#[derive(Debug, Clone)]
pub(crate) struct InterTxOrderRow {
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

impl From<InterTxOrderRow> for MemoryShardRow {
    fn from(r: InterTxOrderRow) -> Self {
        Self {
            key: r.key,
            tx_index: r.tx_index,
            is_init: r.is_init,
            has_read: r.has_read,
            has_write: r.has_write,
            input_val: r.input_val,
            input_is_null: r.input_is_null,
            output_val: r.output_val,
            output_is_null: r.output_is_null,
        }
    }
}

pub(super) fn build_inter_tx_rows_for_parts<const W: usize>(
    table: TableId,
    col: ColId,
    init_rows: &[InitRow],
    access_rows: &[AccessRow],
) -> Result<Vec<InterTxOrderRow>, TabulaError> {
    let mut keys = BTreeSet::new();

    let mut init_by_key: BTreeMap<RowKey, (Vec<KoalaBear>, bool)> = BTreeMap::new();
    for init in init_rows {
        if init.value_fes.len() != W {
            return Err(TabulaError::ProofError {
                phase: "memory",
                detail: format!(
                    "init row width mismatch for ({:?}, {:?}): expected {}, got {}",
                    table,
                    col,
                    W,
                    init.value_fes.len()
                ),
            });
        }
        keys.insert(init.key.row);
        init_by_key.insert(init.key.row, (init.value_fes.clone(), init.val_is_null));
    }

    let mut by_key_tx: BTreeMap<RowKey, BTreeMap<u32, Vec<&AccessRow>>> = BTreeMap::new();
    for access in access_rows {
        if access.value_fes.len() != W {
            return Err(TabulaError::ProofError {
                phase: "memory",
                detail: format!(
                    "access row width mismatch for ({:?}, {:?}): expected {}, got {}",
                    table,
                    col,
                    W,
                    access.value_fes.len()
                ),
            });
        }
        keys.insert(access.key.row);
        by_key_tx
            .entry(access.key.row)
            .or_default()
            .entry(access.tx_index)
            .or_default()
            .push(access);
    }

    let zero = vec![KoalaBear::ZERO; W];
    let mut rows = Vec::new();

    for key in keys {
        let (init_val, init_is_null) = init_by_key
            .get(&key)
            .cloned()
            .unwrap_or_else(|| (zero.clone(), true));

        rows.push(InterTxOrderRow {
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
