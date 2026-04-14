use std::collections::BTreeMap;

use p3_field::PrimeCharacteristicRing;
use p3_koala_bear::KoalaBear;

use crate::stark::{AccessRow, InitRow};
use tabula_chips::shards::memory::trace::MemoryShardRow;
use tabula_core::error::TabulaError;
use tabula_core::{ColId, CommittedKey, TableId};
use tabula_types::{NativeKeyPayload, TableKeyCodec};

/// A single row for building inter-tx ordering data.
///
/// Pre-sorted by `(table_id, col_id, key, tx_index)`.
/// Init rows have `is_init=true` and appear first for each key.
#[derive(Debug, Clone)]
pub(crate) struct InterTxOrderRow {
    /// Native committed-key payload.
    pub key: NativeKeyPayload,
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
    key_codec: &TableKeyCodec,
    init_rows: &[InitRow],
    access_rows: &[AccessRow],
) -> Result<Vec<InterTxOrderRow>, TabulaError> {
    let mut init_by_key: BTreeMap<CommittedKey, (NativeKeyPayload, Vec<KoalaBear>, bool)> =
        BTreeMap::new();
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
        if init_by_key
            .insert(
                init.key.key.clone(),
                (init.key_payload, init.value_fes.clone(), init.val_is_null),
            )
            .is_some()
        {
            return Err(TabulaError::ProofError {
                phase: "memory",
                detail: format!(
                    "duplicate init key while building inter-tx rows for ({table:?}, {col:?})"
                ),
            });
        }
    }

    let mut by_key_tx: BTreeMap<CommittedKey, BTreeMap<u32, Vec<&AccessRow>>> = BTreeMap::new();
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
        by_key_tx
            .entry(access.key.key.clone())
            .or_default()
            .entry(access.tx_index)
            .or_default()
            .push(access);
    }

    let ordered_keys = ordered_keys(key_codec, init_rows, access_rows)?;

    let zero = vec![KoalaBear::ZERO; W];
    let mut rows = Vec::new();

    for (key, payload) in ordered_keys {
        let (init_val, init_is_null) = init_by_key.get(&key).map_or_else(
            || (zero.clone(), true),
            |(_, value, is_null)| (value.clone(), *is_null),
        );

        rows.push(InterTxOrderRow {
            key: payload,
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
                    key: payload,
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

fn ordered_keys(
    key_codec: &TableKeyCodec,
    init_rows: &[InitRow],
    access_rows: &[AccessRow],
) -> Result<Vec<(CommittedKey, NativeKeyPayload)>, TabulaError> {
    let mut ordered = Vec::with_capacity(init_rows.len() + access_rows.len());
    ordered.extend(
        init_rows
            .iter()
            .map(|row| (row.key.key.clone(), row.key_payload)),
    );
    ordered.extend(
        access_rows
            .iter()
            .map(|row| (row.key.key.clone(), row.key_payload)),
    );
    ordered.sort_by(|(lhs, _), (rhs, _)| {
        key_codec
            .compare(lhs, rhs)
            .expect("validated state-key ordering must remain available")
    });

    let mut deduped = Vec::with_capacity(ordered.len());
    for (key, payload) in ordered {
        if let Some((prev_key, prev_payload)) = deduped.last() {
            match key_codec.compare(prev_key, &key)? {
                std::cmp::Ordering::Less => deduped.push((key, payload)),
                std::cmp::Ordering::Equal => {
                    if *prev_payload != payload {
                        return Err(TabulaError::ProofError {
                            phase: "memory",
                            detail:
                                "conflicting payloads for equal committed keys in inter-tx ordering"
                                    .into(),
                        });
                    }
                }
                std::cmp::Ordering::Greater => {
                    return Err(TabulaError::ProofError {
                        phase: "memory",
                        detail: "unordered committed keys while building inter-tx rows".into(),
                    });
                }
            }
        } else {
            deduped.push((key, payload));
        }
    }
    Ok(deduped)
}
