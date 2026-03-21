use std::collections::{BTreeMap, BTreeSet};

use p3_field::PrimeCharacteristicRing;
use p3_koala_bear::KoalaBear;

use tabula_chips::shards::state::trace::{EntrySource, StateShardRow};
use tabula_core::error::TabulaError;
use tabula_core::{ColId, RowKey, TableId};

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
    /// Row key (u64).
    pub key: u64,
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
    /// Previous old-state entry key, or zero when absent.
    pub prev_old_key: u64,
    /// Next old-state entry key, or zero when absent.
    pub next_old_key: u64,
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
    access_rows: &[AccessRow],
    old_entries: &BTreeMap<RowKey, Vec<KoalaBear>>,
    new_entries: &BTreeMap<RowKey, Vec<KoalaBear>>,
    is_touched: bool,
) -> Result<Vec<StateColumnRow>, TabulaError> {
    let mut write_keys: BTreeSet<RowKey> = BTreeSet::new();
    for access in access_rows {
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

        let old_val = old_opt.unwrap_or_else(|| vec![KoalaBear::ZERO; W]);
        let new_val = new_opt.unwrap_or_else(|| vec![KoalaBear::ZERO; W]);

        if old_val.len() != W || new_val.len() != W {
            return Err(TabulaError::ProofError {
                phase: "memory",
                detail: format!(
                    "state row width mismatch for ({:?}, {:?}) key {}",
                    table, col, key.0
                ),
            });
        }

        rows.push(StateColumnRow {
            table_id: table.0,
            col_id: col.0,
            key: key.0,
            is_gap: false,
            source,
            old_val,
            new_val,
            segment_is_touched: is_touched,
            old_hash_acc: [KoalaBear::ZERO; 8],
            new_hash_acc: [KoalaBear::ZERO; 8],
            read_mult: true,
            write_mult: in_write,
            prev_old_key: 0,
            next_old_key: 0,
        });
    }

    populate_old_neighbors(&mut rows);

    Ok(rows)
}
pub(super) fn sort_state_rows(rows: &mut [StateColumnRow]) {
    rows.sort_by_key(|r| (r.table_id, r.col_id, r.key));
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
            .map_or(0, |prev_idx| rows[*prev_idx].key);
        let next = old_indices
            .get(pos + 1)
            .map_or(0, |next_idx| rows[*next_idx].key);
        rows[idx].prev_old_key = prev;
        rows[idx].next_old_key = next;
    }

    let mut last_prev = 0;
    for row in rows.iter_mut() {
        if row.source.in_old() {
            last_prev = row.key;
        } else {
            row.prev_old_key = row.prev_old_key.max(last_prev);
        }
    }

    let mut next_old = 0;
    for row in rows.iter_mut().rev() {
        if row.source.in_old() {
            next_old = row.key;
        } else {
            row.next_old_key = row.next_old_key.max(next_old);
        }
    }
}
