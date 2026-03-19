//! Memory-layer chip input preparation from explicit per-column parts.
//!
//! Provides built-in shard witness helpers that operate on runtime-owned
//! execution rows and scheme-owned state transition artifacts.

use std::collections::BTreeMap;

use p3_koala_bear::KoalaBear;

use tabula_core::error::TabulaError;
use tabula_core::{ColId, TableId};

pub(crate) mod chain;
pub(crate) mod inter_tx;
pub(crate) mod state;

use chain::populate_state_chain_accumulators;
use inter_tx::build_inter_tx_rows_for_parts;
use state::{build_state_rows_for_parts, sort_state_rows};

// ── Shard witness preparation ──────────────────────────────────────────────

use tabula_chips::shards::memory::trace::MemoryShardRow;
use tabula_chips::shards::meta::trace::MetaShardRow;
use tabula_chips::shards::ssmc::SsmcColumnWitness;
use tabula_chips::shards::state::trace::StateShardRow;

/// Explicit parts needed to assemble one SSMC column witness.
pub struct SsmcColumnWitnessParts<'a> {
    /// Column identity `(table, col)`.
    pub column: (TableId, ColId),
    /// Shared init rows derived from the executor read-set.
    pub init_rows: &'a [crate::witness::InitRow],
    /// Shared access rows derived from execution events.
    pub access_rows: &'a [crate::witness::AccessRow],
    /// Old committed entries keyed by row.
    pub old_entries: &'a BTreeMap<tabula_core::RowKey, Vec<KoalaBear>>,
    /// New committed entries keyed by row.
    pub new_entries: &'a BTreeMap<tabula_core::RowKey, Vec<KoalaBear>>,
    /// Verifier-visible column metadata.
    pub meta: &'a tabula_commitment::ColumnMeta,
    /// Whether the scheme emits a commitment proof row for this column.
    pub has_commitment_proof: bool,
}

/// Prepare the shared MemoryShard rows for one committed column from explicit parts.
pub fn prepare_memory_shard_rows_from_parts<const W: usize>(
    table: TableId,
    col: ColId,
    init_rows: &[crate::witness::InitRow],
    access_rows: &[crate::witness::AccessRow],
) -> Result<Vec<MemoryShardRow>, TabulaError> {
    Ok(
        build_inter_tx_rows_for_parts::<W>(table, col, init_rows, access_rows)?
            .into_iter()
            .map(MemoryShardRow::from)
            .collect(),
    )
}

/// Build one MetaShard witness row from explicit column metadata and access rows.
pub fn prepare_meta_shard_row_from_parts(
    meta: &tabula_commitment::ColumnMeta,
    access_rows: &[crate::witness::AccessRow],
    has_commitment_proof: bool,
) -> MetaShardRow {
    let empty_read_count = if meta.is_empty_old {
        access_rows
            .iter()
            .filter(|r| !r.is_write && r.val_is_null)
            .count() as u32
    } else {
        0
    };

    MetaShardRow {
        com_old: meta.com_old,
        com_new: meta.com_new,
        is_empty_old: meta.is_empty_old,
        is_empty_new: meta.is_empty_new,
        is_touched: meta.is_touched,
        has_commitment_proof,
        empty_read_count,
    }
}

/// Prepare SSMC shard witness rows from explicit column parts.
pub fn prepare_ssmc_column_witness_from_parts<const W: usize>(
    parts: &SsmcColumnWitnessParts<'_>,
) -> Result<SsmcColumnWitness, TabulaError> {
    let (table, col) = parts.column;
    let memory_rows =
        prepare_memory_shard_rows_from_parts::<W>(table, col, parts.init_rows, parts.access_rows)?;

    let mut sc_rows = build_state_rows_for_parts::<W>(
        table,
        col,
        parts.access_rows,
        parts.old_entries,
        parts.new_entries,
        parts.meta.is_touched,
    )?;
    sort_state_rows(&mut sc_rows);
    populate_state_chain_accumulators::<W>(&mut sc_rows);

    let state_rows: Vec<StateShardRow> = sc_rows.into_iter().map(StateShardRow::from).collect();
    let meta_row = prepare_meta_shard_row_from_parts(
        parts.meta,
        parts.access_rows,
        parts.has_commitment_proof,
    );

    Ok(SsmcColumnWitness {
        memory_rows,
        state_rows,
        meta_row: Some(meta_row),
    })
}
