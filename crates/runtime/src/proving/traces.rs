use rayon::prelude::*;
use tabula_core::error::TabulaError;
use tabula_machine::{ColumnProofTrace, ProofSetups, ProofTraces, TabulaMachine, TierSetup};
use tabula_stark::trace::{TraceMap, WitnessStore, build_all_traces};

use crate::error::RuntimeError;

use super::partition::{PartitionedStores, partition_by_tier};
use super::prepare::PreparedProofBatch;

/// Build traces from a fully prepared runtime-owned proof batch.
#[tracing::instrument(skip_all)]
pub(crate) fn build_traces(
    machine: &TabulaMachine,
    prepared: &mut PreparedProofBatch,
) -> Result<ProofTraces, RuntimeError> {
    let shared_store = std::mem::take(&mut prepared.shared_store);
    let columns = std::mem::take(&mut prepared.columns);
    let stores = partition_by_tier(shared_store);
    build_proof_traces(machine.setup().proof_setups(), stores, columns)
        .map_err(RuntimeError::TraceBuild)
}

fn build_proof_traces(
    setups: &ProofSetups,
    stores: PartitionedStores,
    columns: Vec<super::prepare::ColumnTraceInput>,
) -> Result<ProofTraces, TabulaError> {
    let exec_traces = build_tier_traces(&setups.execution, stores.execution)?;

    if columns.len() != setups.columns.len() {
        return Err(TabulaError::ProofError {
            phase: "trace_build",
            detail: format!(
                "column trace input count {} does not match machine setup count {}",
                columns.len(),
                setups.columns.len()
            ),
        });
    }

    let col_traces: Vec<_> = columns
        .into_par_iter()
        .zip(setups.columns.par_iter())
        .map(|(column, ((table, col), setup))| {
            if column.identity.table_id != table.0 || column.identity.col_id != col.0 {
                return Err(TabulaError::ProofError {
                    phase: "trace_build",
                    detail: format!(
                        "prepared column ({}, {}) does not match machine setup order ({}, {})",
                        column.identity.table_id, column.identity.col_id, table.0, col.0
                    ),
                });
            }
            let trace_map = build_tier_traces(setup, column.store)?;
            Ok(ColumnProofTrace {
                identity: column.identity,
                trace_map,
            })
        })
        .collect::<Result<Vec<_>, TabulaError>>()?;

    let root_traces = build_tier_traces(&setups.root, stores.root)?;

    Ok(ProofTraces {
        execution: exec_traces,
        columns: col_traces,
        root: root_traces,
    })
}

fn build_tier_traces(setup: &TierSetup, store: WitnessStore) -> Result<TraceMap, TabulaError> {
    build_all_traces(setup.dyn_chips(), setup.bus_consumers(), store)
}

#[cfg(test)]
mod tests {
    use tabula_testing::fixtures::examples::transfer_example_compiled_case;

    use super::*;
    use crate::TabulaRuntime;
    use crate::proving::prepare_proof_batch;

    #[test]
    fn prepared_proof_batch_assembles_one_store_per_planned_column() {
        let case = transfer_example_compiled_case();
        let runtime = TabulaRuntime::builder(case.compiled_program)
            .build()
            .expect("runtime");
        let executed = runtime
            .execute(&case.state, &case.batch)
            .expect("execution succeeds");
        let prepared = prepare_proof_batch(
            runtime.resolved_program(),
            runtime.proof_recipes(),
            runtime.precompile_recipes(),
            &case.state,
            &case.batch,
            &executed,
        )
        .expect("prepared proof batch");

        let expected_keys: Vec<_> = prepared
            .columns
            .iter()
            .map(|column| (column.identity.table_id, column.identity.col_id))
            .collect();

        assert_eq!(prepared.columns.len(), expected_keys.len());
        for column in &prepared.columns {
            assert!(
                expected_keys.iter().any(|(table, col)| {
                    *table == column.identity.table_id && *col == column.identity.col_id
                }),
                "missing prepared artifact for ({}, {})",
                column.identity.table_id,
                column.identity.col_id
            );
        }
    }

    #[test]
    fn prepared_proof_batch_builds_column_traces_for_all_setups() {
        let case = transfer_example_compiled_case();
        let runtime = TabulaRuntime::builder(case.compiled_program)
            .build()
            .expect("runtime");
        let executed = runtime
            .execute(&case.state, &case.batch)
            .expect("execution succeeds");
        let prepared = prepare_proof_batch(
            runtime.resolved_program(),
            runtime.proof_recipes(),
            runtime.precompile_recipes(),
            &case.state,
            &case.batch,
            &executed,
        )
        .expect("prepared proof batch");

        let mut prepared = prepared;
        let traces = build_traces(runtime.machine(), &mut prepared).expect("proof traces");

        assert_eq!(
            traces.columns.len(),
            runtime.machine().setup().proof_setups().columns.len()
        );
        assert!(
            traces
                .columns
                .iter()
                .zip(runtime.machine().setup().proof_setups().columns.iter())
                .all(|(trace, ((table_id, col_id), _))| {
                    trace.identity.table_id == table_id.0 && trace.identity.col_id == col_id.0
                }),
            "column trace order must match machine setup order"
        );
    }
}
