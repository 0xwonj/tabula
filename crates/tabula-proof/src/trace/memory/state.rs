use std::collections::{BTreeMap, BTreeSet};

use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;

use tabula_commitment::{ColumnState, FieldHasher, NativeDigest};
use tabula_core::RowKey;
use tabula_core::error::TabulaError;

use crate::chips::state_column::trace::{EntrySource, StateColumnRow};
use crate::witness::ColumnWitness;

pub(super) fn build_state_rows<H, const W: usize>(
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
            return Err(TabulaError::ProofError {
                phase: "memory",
                detail: format!(
                    "state row width mismatch for ({:?}, {:?}) key {}",
                    column.table, column.col, key.0
                ),
            });
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

pub(super) fn sort_state_rows(rows: &mut [StateColumnRow]) {
    rows.sort_by_key(|r| (r.table_id, r.col_id, r.key));
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
        ColumnState::Smt(_) => Err(TabulaError::ProofError {
            phase: "memory",
            detail: "only SSMC-backed columns are currently supported".to_string(),
        }),
    }
}
