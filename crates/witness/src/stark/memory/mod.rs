//! Memory-layer chip input preparation from explicit per-column parts.

use std::collections::BTreeMap;

use tabula_commitment::ColumnRootBinding;
use tabula_core::error::TabulaError;
use tabula_core::{ColId, TableId};
use tabula_types::{EncodingRuntime, TypeRuntime, encode_value_with_null_flag};

use crate::{AccessEvent, InitCell};

use super::rows::{AccessRow, InitRow};

pub(crate) mod chain;
pub(crate) mod inter_tx;
pub(crate) mod state;

use chain::populate_state_chain_accumulators;
use inter_tx::build_inter_tx_rows_for_parts;
use state::{build_state_rows_for_parts, sort_state_rows};

use tabula_chips::shards::memory::trace::MemoryShardRow;
use tabula_chips::shards::meta::trace::MetaShardRow;
use tabula_chips::shards::ssmc::SsmcColumnWitness;
use tabula_chips::shards::state::trace::StateShardRow;

/// Explicit parts needed to assemble one SSMC column witness.
pub(crate) struct SsmcColumnWitnessParts<'a> {
    pub column: (TableId, ColId),
    pub type_runtime: &'a dyn TypeRuntime,
    pub encoding_runtime: &'a dyn EncodingRuntime,
    pub init_cells: &'a [InitCell],
    pub access_events: &'a [AccessEvent],
    pub old_entries: &'a BTreeMap<tabula_core::RowKey, Vec<p3_koala_bear::KoalaBear>>,
    pub new_entries: &'a BTreeMap<tabula_core::RowKey, Vec<p3_koala_bear::KoalaBear>>,
    pub root_binding: &'a ColumnRootBinding,
    pub has_commitment_proof: bool,
}

/// Prepare the shared MemoryShard rows for one committed column from explicit parts.
pub(crate) fn prepare_memory_shard_rows_from_parts<const W: usize>(
    table: TableId,
    col: ColId,
    type_runtime: &dyn TypeRuntime,
    encoding_runtime: &dyn EncodingRuntime,
    init_cells: &[InitCell],
    access_events: &[AccessEvent],
) -> Result<Vec<MemoryShardRow>, TabulaError> {
    let init_rows = encode_init_cells(type_runtime, encoding_runtime, init_cells)?;
    let access_rows = encode_access_events(type_runtime, encoding_runtime, access_events)?;
    Ok(
        build_inter_tx_rows_for_parts::<W>(table, col, &init_rows, &access_rows)?
            .into_iter()
            .map(MemoryShardRow::from)
            .collect(),
    )
}

pub(crate) fn prepare_meta_shard_row_from_parts(
    root_binding: &ColumnRootBinding,
    access_events: &[AccessEvent],
    has_commitment_proof: bool,
) -> MetaShardRow {
    let empty_read_count = if root_binding.is_empty_old {
        access_events
            .iter()
            .filter(|r| !r.is_write && r.is_null)
            .count() as u32
    } else {
        0
    };

    MetaShardRow {
        com_old: root_binding.old_digest.digest,
        com_new: root_binding.new_digest.digest,
        is_empty_old: root_binding.is_empty_old,
        is_empty_new: root_binding.is_empty_new,
        is_touched: root_binding.is_touched,
        has_commitment_proof,
        empty_read_count,
    }
}

pub(crate) fn prepare_ssmc_column_witness_from_parts<const W: usize>(
    parts: &SsmcColumnWitnessParts<'_>,
) -> Result<SsmcColumnWitness, TabulaError> {
    let (table, col) = parts.column;
    let memory_rows = prepare_memory_shard_rows_from_parts::<W>(
        table,
        col,
        parts.type_runtime,
        parts.encoding_runtime,
        parts.init_cells,
        parts.access_events,
    )?;
    let access_rows = encode_access_events(
        parts.type_runtime,
        parts.encoding_runtime,
        parts.access_events,
    )?;

    let mut sc_rows = build_state_rows_for_parts::<W>(
        table,
        col,
        &access_rows,
        parts.old_entries,
        parts.new_entries,
        parts.root_binding.is_touched,
    )?;
    sort_state_rows(&mut sc_rows);
    populate_state_chain_accumulators::<W>(&mut sc_rows);

    let state_rows: Vec<StateShardRow> = sc_rows.into_iter().map(StateShardRow::from).collect();
    let meta_row = prepare_meta_shard_row_from_parts(
        parts.root_binding,
        parts.access_events,
        parts.has_commitment_proof,
    );

    Ok(SsmcColumnWitness {
        memory_rows,
        state_rows,
        meta_row: Some(meta_row),
    })
}

fn encode_init_cells(
    type_runtime: &dyn TypeRuntime,
    encoding_runtime: &dyn EncodingRuntime,
    init_cells: &[InitCell],
) -> Result<Vec<InitRow>, TabulaError> {
    init_cells
        .iter()
        .map(|cell| {
            let (value_fes, val_is_null) = encode_value_with_null_flag(
                type_runtime,
                encoding_runtime,
                &cell.value,
                cell.is_null,
            )?;
            Ok(InitRow {
                key: cell.key,
                value_fes,
                val_is_null,
            })
        })
        .collect()
}

fn encode_access_events(
    type_runtime: &dyn TypeRuntime,
    encoding_runtime: &dyn EncodingRuntime,
    access_events: &[AccessEvent],
) -> Result<Vec<AccessRow>, TabulaError> {
    access_events
        .iter()
        .map(|event| {
            let (value_fes, val_is_null) = encode_value_with_null_flag(
                type_runtime,
                encoding_runtime,
                &event.value,
                event.is_null,
            )?;
            Ok(AccessRow {
                key: event.key,
                time: event.time,
                is_write: event.is_write,
                value_fes,
                val_is_null,
                tx_index: event.tx_index,
                effect_ordinal_in_tx: event.effect_ordinal_in_tx,
            })
        })
        .collect()
}
