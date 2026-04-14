use std::collections::{BTreeMap, BTreeSet};

use p3_field::PrimeCharacteristicRing;
use p3_koala_bear::KoalaBear;

use tabula_chips::shards::state::trace::{EntrySource, StateShardRow};
use tabula_core::error::TabulaError;
use tabula_core::{ColId, CommittedKey, TableId};
use tabula_types::{NativeKeyPayload, TableKeyCodec, zero_key_payload};

use crate::stark::AccessRow;

/// A single row for building state column data.
///
/// Pre-sorted by `(table_id, col_id, key)`.
#[derive(Debug, Clone)]
pub(crate) struct StateColumnRow {
    /// Table identifier.
    pub table_id: u32,
    /// Column identifier.
    pub col_id: u16,
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
    /// Per-segment: 1 if this column is touched in the batch.
    pub segment_is_touched: bool,
    /// Precomputed old hash chain accumulator.
    pub old_hash_acc: [KoalaBear; 8],
    /// Precomputed new hash chain accumulator.
    pub new_hash_acc: [KoalaBear; 8],
    /// Multiplicity for ReadAccess bus (C1 receive).
    pub read_mult: bool,
    /// Multiplicity for WriteAccess bus (C4 receive).
    pub write_mult: bool,
    /// Previous old-state entry key, or zero payload when absent.
    pub prev_old_key: NativeKeyPayload,
    /// Next old-state entry key, or zero payload when absent.
    pub next_old_key: NativeKeyPayload,
}

/// Ordered column entry used by the SSMC witness builder.
#[derive(Debug, Clone)]
pub(crate) struct OrderedStateEntry {
    /// Canonical committed key.
    pub key: CommittedKey,
    /// Native proof-visible committed-key payload.
    pub payload: NativeKeyPayload,
    /// Encoded value field elements.
    pub value: Vec<KoalaBear>,
}

impl From<StateColumnRow> for StateShardRow {
    fn from(r: StateColumnRow) -> Self {
        Self {
            key: r.key,
            is_gap: r.is_gap,
            source: r.source,
            old_val: r.old_val,
            new_val: r.new_val,
            segment_is_touched: r.segment_is_touched,
            old_hash_acc: r.old_hash_acc,
            new_hash_acc: r.new_hash_acc,
            read_mult: r.read_mult,
            write_mult: r.write_mult,
            prev_old_key: r.prev_old_key,
            next_old_key: r.next_old_key,
        }
    }
}

pub(super) fn build_state_rows_for_parts<const W: usize>(
    table: TableId,
    col: ColId,
    key_codec: &TableKeyCodec,
    access_rows: &[AccessRow],
    old_entries: &[OrderedStateEntry],
    new_entries: &[OrderedStateEntry],
    is_touched: bool,
) -> Result<Vec<StateColumnRow>, TabulaError> {
    let mut write_keys: BTreeSet<CommittedKey> = BTreeSet::new();
    for access in access_rows {
        if access.is_write && !write_keys.insert(access.key.key.clone()) {
            return Err(TabulaError::ProofError {
                phase: "memory",
                detail: format!("duplicate write key in SSMC state rows for ({table:?}, {col:?})"),
            });
        }
    }

    let old_by_key = map_entries_by_key(table, col, old_entries)?;
    let new_by_key = map_entries_by_key(table, col, new_entries)?;
    let ordered_keys =
        ordered_union_keys(table, col, key_codec, old_entries, new_entries, access_rows)?;

    let mut rows = Vec::new();
    for (key, payload) in ordered_keys {
        let old_opt = old_by_key.get(&key).cloned();
        let new_opt = new_by_key.get(&key).cloned();
        let in_write = write_keys.contains(&key);

        let source = match (old_opt.as_ref(), new_opt.as_ref()) {
            (Some(_), Some(_)) if in_write => EntrySource::Both,
            (Some(_), Some(_)) => EntrySource::OldOnly,
            (None, Some(_)) => EntrySource::WriteOnly,
            (Some(_), None) => EntrySource::Delete,
            (None, None) => continue,
        };

        let old_val = old_opt.unwrap_or_else(|| vec![KoalaBear::ZERO; W]);
        let new_val = new_opt.unwrap_or_else(|| vec![KoalaBear::ZERO; W]);

        if old_val.len() != W || new_val.len() != W {
            return Err(TabulaError::ProofError {
                phase: "memory",
                detail: format!("state row width mismatch for ({table:?}, {col:?}) key {key:?}"),
            });
        }

        rows.push(StateColumnRow {
            table_id: table.0,
            col_id: col.0,
            key: payload,
            is_gap: false,
            source,
            old_val,
            new_val,
            segment_is_touched: is_touched,
            old_hash_acc: [KoalaBear::ZERO; 8],
            new_hash_acc: [KoalaBear::ZERO; 8],
            read_mult: true,
            write_mult: in_write,
            prev_old_key: zero_key_payload(),
            next_old_key: zero_key_payload(),
        });
    }

    populate_old_neighbors(&mut rows);

    Ok(rows)
}

fn populate_old_neighbors(rows: &mut [StateColumnRow]) {
    let old_indices: Vec<usize> = rows
        .iter()
        .enumerate()
        .filter_map(|(idx, row)| row.source.in_old().then_some(idx))
        .collect();

    for (pos, &idx) in old_indices.iter().enumerate() {
        let prev = pos
            .checked_sub(1)
            .and_then(|prev_pos| old_indices.get(prev_pos))
            .map_or(zero_key_payload(), |prev_idx| rows[*prev_idx].key);
        let next = old_indices
            .get(pos + 1)
            .map_or(zero_key_payload(), |next_idx| rows[*next_idx].key);
        rows[idx].prev_old_key = prev;
        rows[idx].next_old_key = next;
    }

    let mut last_prev = zero_key_payload();
    for row in rows.iter_mut() {
        if row.source.in_old() {
            last_prev = row.key;
        } else {
            row.prev_old_key = last_prev;
        }
    }

    let mut next_old = zero_key_payload();
    for row in rows.iter_mut().rev() {
        if row.source.in_old() {
            next_old = row.key;
        } else {
            row.next_old_key = next_old;
        }
    }
}

fn map_entries_by_key(
    table: TableId,
    col: ColId,
    entries: &[OrderedStateEntry],
) -> Result<BTreeMap<CommittedKey, Vec<KoalaBear>>, TabulaError> {
    let mut by_key = BTreeMap::new();
    for entry in entries {
        if by_key
            .insert(entry.key.clone(), entry.value.clone())
            .is_some()
        {
            return Err(TabulaError::ProofError {
                phase: "memory",
                detail: format!("duplicate ordered SSMC state entry for ({table:?}, {col:?})"),
            });
        }
    }
    Ok(by_key)
}

fn ordered_union_keys(
    table: TableId,
    col: ColId,
    key_codec: &TableKeyCodec,
    old_entries: &[OrderedStateEntry],
    new_entries: &[OrderedStateEntry],
    access_rows: &[AccessRow],
) -> Result<Vec<(CommittedKey, NativeKeyPayload)>, TabulaError> {
    let mut ordered = Vec::with_capacity(old_entries.len() + new_entries.len() + access_rows.len());
    ordered.extend(
        old_entries
            .iter()
            .map(|entry| (entry.key.clone(), entry.payload)),
    );
    ordered.extend(
        new_entries
            .iter()
            .map(|entry| (entry.key.clone(), entry.payload)),
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
                            detail: format!(
                                "conflicting payloads for equal committed keys in SSMC column ({table:?}, {col:?})"
                            ),
                        });
                    }
                }
                std::cmp::Ordering::Greater => {
                    return Err(TabulaError::ProofError {
                        phase: "memory",
                        detail: format!(
                            "unordered committed keys while preparing SSMC column ({table:?}, {col:?})"
                        ),
                    });
                }
            }
        } else {
            deduped.push((key, payload));
        }
    }
    Ok(deduped)
}
