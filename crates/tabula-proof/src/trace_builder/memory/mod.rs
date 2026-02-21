use std::collections::BTreeMap;

use p3_baby_bear::BabyBear;

use tabula_commitment::{FieldHasher, NativeDigest};
use tabula_core::error::TabulaError;
use tabula_core::{ColId, TableId};

use crate::air::chips::column_meta::trace::generate_column_meta_trace;
use crate::air::chips::inter_tx_order::trace::generate_inter_tx_order_trace;
use crate::air::chips::state_column::trace::generate_state_column_trace;
use crate::witness::BatchWitness;

use super::types::ProofTraceBundle;

mod chain;
mod inter_tx;
mod state;

use chain::populate_state_chain_accumulators;
use inter_tx::{build_inter_tx_rows, sort_inter_tx_rows};
use state::{build_state_rows, sort_state_rows};

/// Build all memory/metadata traces from one `BatchWitness`.
pub fn build_trace_bundle<H, const W: usize>(
    witness: &BatchWitness<H>,
) -> Result<ProofTraceBundle<W>, TabulaError>
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

    let inter_tx_trace = generate_inter_tx_order_trace::<W>(&inter_tx_rows);
    let state_trace = generate_state_column_trace::<W>(&state_rows);
    let column_meta_trace =
        generate_column_meta_trace(&witness.column_metas, &empty_read_mults_for_trace);

    Ok(ProofTraceBundle {
        inter_tx_rows,
        state_rows,
        empty_read_mults,
        inter_tx_trace,
        state_trace,
        column_meta_trace,
    })
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
