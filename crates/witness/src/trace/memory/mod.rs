use std::collections::BTreeMap;

use p3_baby_bear::BabyBear;

use tabula_commitment::{FieldHasher, NativeDigest};
use tabula_core::error::TabulaError;
use tabula_core::{ColId, TableId};

use crate::witness::BatchWitness;
use tabula_chips::column_meta::trace::ColumnMetaInput;
use tabula_chips::inter_tx_order::trace::InterTxOrderRow;
use tabula_chips::state_column::trace::StateColumnRow;

mod chain;
mod inter_tx;
mod state;

use chain::populate_state_chain_accumulators;
use inter_tx::{build_inter_tx_rows, sort_inter_tx_rows};
use state::{build_state_rows, sort_state_rows};

/// Pre-built row data for memory-layer chips (Phase 1).
///
/// Produced by [`prepare_memory_inputs`], consumed by each chip's
/// [`TraceContributor::contribute`] via the [`WitnessStore`].
pub(crate) struct MemoryInputs {
    /// Sorted inter-tx ordering rows.
    pub inter_tx_rows: Vec<InterTxOrderRow>,
    /// Sorted state column rows with chain accumulators populated.
    pub state_rows: Vec<StateColumnRow>,
    /// Column metadata + empty-read counts.
    pub column_meta_input: ColumnMetaInput,
}

/// Prepare memory-layer chip inputs from a [`BatchWitness`].
///
/// Builds, sorts, and populates running accumulators for InterTxOrder,
/// StateColumn, and ColumnMeta chips. The results are placed in the
/// [`WitnessStore`] by the caller for phase-based dispatch.
pub(super) fn prepare_memory_inputs<H, const W: usize>(
    witness: &BatchWitness<H>,
) -> Result<MemoryInputs, TabulaError>
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

    let column_meta_input = ColumnMetaInput {
        metas: witness.column_metas.clone(),
        empty_read_counts: empty_read_mults_for_trace,
    };

    Ok(MemoryInputs {
        inter_tx_rows,
        state_rows,
        column_meta_input,
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
