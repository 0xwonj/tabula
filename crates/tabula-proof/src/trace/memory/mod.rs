use std::collections::BTreeMap;

use p3_baby_bear::BabyBear;

use tabula_commitment::{FieldHasher, NativeDigest};
use tabula_core::error::TabulaError;
use tabula_core::{ColId, TableId};

use crate::chips::ChipSpec;
use crate::chips::column_meta::air::ColumnMetaChip;
use crate::chips::column_meta::trace::ColumnMetaInput;
use crate::chips::inter_tx_order::air::InterTxOrderChip;
use crate::chips::state_column::air::StateColumnChip;
use crate::trace::TraceGenerator;
use crate::witness::BatchWitness;

use super::trace_map::TraceMap;

mod chain;
mod inter_tx;
mod state;

use chain::populate_state_chain_accumulators;
use inter_tx::{build_inter_tx_rows, sort_inter_tx_rows};
use state::{build_state_rows, sort_state_rows};

/// Build memory/metadata chip traces and insert them into a [`TraceMap`].
///
/// Generates InterTxOrder, StateColumn, and ColumnMeta traces from the
/// witness data and inserts them directly into the provided map.
pub(super) fn build_memory_traces<H, const W: usize>(
    witness: &BatchWitness<H>,
    map: &mut TraceMap,
) -> Result<(), TabulaError>
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

    let ito_chip = InterTxOrderChip::<W>;
    let state_chip = StateColumnChip::<W>;
    let col_meta_chip = ColumnMetaChip;

    let col_meta_input = ColumnMetaInput {
        metas: witness.column_metas.clone(),
        empty_read_counts: empty_read_mults_for_trace,
    };

    map.insert_entry(ito_chip.chip_name(), ito_chip.build_entry(&inter_tx_rows));
    map.insert_entry(state_chip.chip_name(), state_chip.build_entry(&state_rows));
    map.insert_entry(
        col_meta_chip.chip_name(),
        col_meta_chip.build_entry(&col_meta_input),
    );

    Ok(())
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
