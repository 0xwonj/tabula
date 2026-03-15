//! Memory-layer chip input preparation from [`BatchWitness`].
//!
//! Provides per-column shard witness preparation via [`prepare_shard_witness`].
//! Produces per-column data for MemoryShard + StateShard + MetaShard chips.

use std::collections::BTreeMap;

use p3_koala_bear::KoalaBear;

use tabula_commitment::{FieldHasher, NativeDigest};
use tabula_core::error::TabulaError;
use tabula_core::{ColId, TableId};

use crate::witness::BatchWitness;

pub(crate) mod chain;
pub(crate) mod inter_tx;
pub(crate) mod state;

use chain::populate_state_chain_accumulators;
use inter_tx::build_inter_tx_rows;
use state::{build_state_rows, sort_state_rows};

fn build_empty_read_mults<H>(witness: &BatchWitness<H>) -> BTreeMap<(TableId, ColId), u32>
where
    H: FieldHasher<F = KoalaBear, Digest = NativeDigest>,
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

// ── Shard witness preparation ──────────────────────────────────────────────

use tabula_chips::shards::memory::trace::MemoryShardRow;
use tabula_chips::shards::meta::trace::MetaShardRow;
use tabula_chips::shards::ssmc::{SsmcColumnWitness, SsmcWitness};
use tabula_chips::shards::state::trace::StateShardRow;

/// Prepare per-column shard witness data from a [`BatchWitness`].
///
/// Builds [`SsmcWitness`] containing per-column `MemoryShardRow`,
/// `StateShardRow`, and `MetaShardRow` data suitable for shard chip
/// trace generation. Each column's data is self-contained.
///
/// Used by the sharded prover to produce per-column proof instances.
pub fn prepare_shard_witness<H, const W: usize>(
    witness: &BatchWitness<H>,
) -> Result<SsmcWitness, TabulaError>
where
    H: FieldHasher<F = KoalaBear, Digest = NativeDigest>,
{
    let empty_read_mults = build_empty_read_mults::<H>(witness);
    let mut ssmc_witness = SsmcWitness::default();

    for column in &witness.columns {
        // Skip untouched columns — they don't need column proofs.
        // Their leaf digests appear implicitly as SMT siblings in
        // touched columns' Merkle paths.
        if !column.meta.is_touched {
            continue;
        }

        // Build InterTxOrder rows, then convert to MemoryShardRows.
        let itx_rows = build_inter_tx_rows::<H, W>(column)?;
        let memory_rows: Vec<MemoryShardRow> =
            itx_rows.into_iter().map(MemoryShardRow::from).collect();

        // Build StateColumn rows, sort, populate hash chain accumulators,
        // then convert to StateShardRows.
        let mut sc_rows = build_state_rows::<H, W>(column)?;
        sort_state_rows(&mut sc_rows);
        populate_state_chain_accumulators::<W>(&mut sc_rows);

        let state_rows: Vec<StateShardRow> = sc_rows.into_iter().map(StateShardRow::from).collect();

        // Create MetaShardRow from ColumnMeta.
        let empty_count = empty_read_mults
            .get(&(column.table, column.col))
            .copied()
            .unwrap_or(0);
        let meta_row = MetaShardRow {
            com_old: column.meta.com_old,
            com_new: column.meta.com_new,
            is_empty_old: column.meta.is_empty_old,
            is_empty_new: column.meta.is_empty_new,
            is_touched: column.meta.is_touched,
            empty_read_count: empty_count,
        };

        ssmc_witness.insert(
            column.table,
            column.col,
            SsmcColumnWitness {
                memory_rows,
                state_rows,
                meta_row: Some(meta_row),
            },
        );
    }

    Ok(ssmc_witness)
}
