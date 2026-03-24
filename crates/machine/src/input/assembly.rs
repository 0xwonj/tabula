use rayon::prelude::*;
use tabula_core::error::TabulaError;
use tabula_stark::trace::{TraceMap, WitnessStore, build_all_traces};

use super::{ColumnSlotKey, PreparedColumnInput};
use crate::proof::errors::ProveError;
use crate::setup::{ProofTopology, TierTopology};

pub(crate) struct ColumnTraceBundle {
    pub(crate) key: ColumnSlotKey,
    pub(crate) trace_map: TraceMap,
}

pub(crate) struct TierTraceBundle {
    pub(crate) execution: TraceMap,
    pub(crate) columns: Vec<ColumnTraceBundle>,
    pub(crate) root: TraceMap,
}

pub(crate) fn build_proof_traces(
    setups: &ProofTopology,
    execution_store: WitnessStore,
    columns: Vec<PreparedColumnInput>,
    root_store: WitnessStore,
) -> Result<TierTraceBundle, ProveError> {
    let execution = build_tier_traces(&setups.execution, execution_store)?;

    if columns.len() != setups.columns.len() {
        return Err(ProveError::InvalidProofInput {
            detail: format!(
                "column trace input count {} does not match machine setup count {}",
                columns.len(),
                setups.columns.len(),
            ),
        });
    }

    let columns = columns
        .into_par_iter()
        .zip(setups.columns.par_iter())
        .map(|(column, ((table, col), setup))| {
            let expected = ColumnSlotKey {
                table: *table,
                col: *col,
            };
            if column.key != expected {
                return Err(ProveError::InvalidProofInput {
                    detail: format!(
                        "prepared column {} does not match machine setup order {}",
                        column.key, expected,
                    ),
                });
            }

            let trace_map = build_tier_traces(setup, column.store)?;
            Ok(ColumnTraceBundle {
                key: column.key,
                trace_map,
            })
        })
        .collect::<Result<Vec<_>, ProveError>>()?;

    let root = build_tier_traces(&setups.root, root_store)?;

    Ok(TierTraceBundle {
        execution,
        columns,
        root,
    })
}

fn build_tier_traces(setup: &TierTopology, store: WitnessStore) -> Result<TraceMap, ProveError> {
    build_all_traces(setup.dyn_chips(), setup.bus_consumers(), store)
        .map_err(|error| trace_build_error(&error))
}

fn trace_build_error(error: &TabulaError) -> ProveError {
    ProveError::InvalidProofInput {
        detail: format!("trace build failed: {error}"),
    }
}
