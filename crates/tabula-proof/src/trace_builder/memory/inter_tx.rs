use std::collections::{BTreeMap, BTreeSet};

use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;

use tabula_commitment::{FieldHasher, NativeDigest};
use tabula_core::RowKey;
use tabula_core::error::TabulaError;

use crate::air::chips::inter_tx_order::trace::InterTxOrderRow;
use crate::witness::ColumnWitness;

pub(super) fn build_inter_tx_rows<H, const W: usize>(
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

pub(super) fn sort_inter_tx_rows(rows: &mut [InterTxOrderRow]) {
    rows.sort_by(|a, b| {
        (a.table_id, a.col_id, a.key)
            .cmp(&(b.table_id, b.col_id, b.key))
            .then_with(|| a.tx_index.cmp(&b.tx_index))
            // For same (t,c,key,tx), init must come first.
            .then_with(|| b.is_init.cmp(&a.is_init))
    });
}
