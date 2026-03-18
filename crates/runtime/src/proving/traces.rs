use std::collections::BTreeMap;

use rayon::prelude::*;
use tabula_commitment::PoseidonHasher;
use tabula_core::error::TabulaError;
use tabula_core::{Batch, BatchResult, ColId, InMemoryStaticTables, TableId};
use tabula_machine::{ProofSetups, ProofTraces, TabulaMachine, TierSetup};
use tabula_stark::trace::{TraceMap, WitnessStore};
use tabula_witness::trace::{PartitionedStores, build_all_traces, partition_by_tier};
use tabula_witness::trace::builtin::{BuiltinTraceBuilder, PropertyReadRecord};
use tabula_witness::BatchWitness;

use crate::error::RuntimeError;
use crate::program::RuntimeProgram;

/// Build traces from witness through the full runtime-owned pipeline:
/// witness store -> per-column proof inputs -> tier partition -> proof traces.
#[tracing::instrument(skip_all)]
pub fn build_traces(
    machine: &TabulaMachine,
    runtime_program: &RuntimeProgram,
    witness: &BatchWitness<PoseidonHasher>,
    batch: &Batch,
    batch_result: &BatchResult,
) -> Result<ProofTraces, RuntimeError> {
    let prepared = BuiltinTraceBuilder::<PoseidonHasher, 3>::new(witness)
        .prepare_witness_store(
            runtime_program.program(),
            batch,
            batch_result,
            runtime_program.schemas_by_id(),
            &InMemoryStaticTables::new(),
            PoseidonHasher::new(),
        )
        .map_err(RuntimeError::TraceBuild)?;

    let column_stores =
        build_column_stores(runtime_program, witness, &prepared.property_reads)?;
    let stores = partition_by_tier(prepared.store, column_stores);
    build_proof_traces(machine.setup().proof_setups(), stores).map_err(RuntimeError::TraceBuild)
}

fn build_column_stores(
    runtime_program: &RuntimeProgram,
    witness: &BatchWitness<PoseidonHasher>,
    property_records: &BTreeMap<(TableId, ColId), Vec<PropertyReadRecord>>,
) -> Result<Vec<((TableId, ColId), WitnessStore)>, RuntimeError> {
    witness
        .columns
        .par_iter()
        .map(|column| {
            let key = (column.table, column.col);
            let Some(builder) = runtime_program.proof_input_builders().get(&key) else {
                return Err(RuntimeError::ValidationFailed {
                    detail: format!(
                        "missing proof input builder for table {} col {}",
                        column.table.0, column.col.0
                    ),
                });
            };
            let reads = property_records
                .get(&key)
                .map_or(&[] as &[PropertyReadRecord], Vec::as_slice);
            let store = builder
                .build_witness_store(column, reads)
                .map_err(RuntimeError::TraceBuild)?;
            Ok((key, store))
        })
        .collect()
}

fn build_proof_traces(
    setups: &ProofSetups,
    stores: PartitionedStores,
) -> Result<ProofTraces, TabulaError> {
    let exec_traces = build_tier_traces(&setups.execution, stores.execution)?;

    let setup_index: BTreeMap<(TableId, ColId), usize> = setups
        .columns
        .iter()
        .enumerate()
        .map(|(i, ((table_id, col_id), _))| ((*table_id, *col_id), i))
        .collect();

    let col_traces: Vec<_> = stores
        .columns
        .into_par_iter()
        .map(|((table, col), col_store)| {
            let idx = setup_index
                .get(&(table, col))
                .ok_or_else(|| TabulaError::ProofError {
                    phase: "trace_build",
                    detail: format!("no setup for column ({}, {})", table.0, col.0),
                })?;
            let traces = build_tier_traces(&setups.columns[*idx].1, col_store)?;
            Ok(((table, col), traces))
        })
        .collect::<Result<Vec<_>, TabulaError>>()?;

    debug_assert!(
        col_traces
            .iter()
            .zip(setups.columns.iter())
            .all(|(((t1, c1), _), ((t2, c2), _))| t1 == t2 && c1 == c2),
        "column trace ordering must match setup ordering"
    );

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
    use tabula_compiler::{register_program_artifact, transfer_example_bundle};

    use super::*;
    use crate::proving::{convert_batch, prepare_witness_artifacts};
    use crate::TabulaRuntime;

    #[test]
    fn proof_input_builders_assemble_one_store_per_witness_column() {
        let bundle = transfer_example_bundle().expect("example bundle");
        let compiled = register_program_artifact(&bundle.program).expect("compiled program");
        let runtime = TabulaRuntime::builder(compiled).build().expect("runtime");
        let executed = runtime
            .execute(&bundle.state, &bundle.batch)
            .expect("execution succeeds");
        let artifacts = prepare_witness_artifacts(runtime.runtime_program(), &bundle.state, &executed)
            .expect("witness artifacts");
        let batch = convert_batch(&bundle.batch).expect("batch conversion");
        let prepared = BuiltinTraceBuilder::<PoseidonHasher, 3>::new(&artifacts.witness)
            .prepare_witness_store(
                runtime.runtime_program().program(),
                &batch,
                &artifacts.batch_result,
                runtime.runtime_program().schemas_by_id(),
                &InMemoryStaticTables::new(),
                PoseidonHasher::new(),
            )
            .expect("builtin witness preparation");

        let stores = build_column_stores(
            runtime.runtime_program(),
            &artifacts.witness,
            &prepared.property_reads,
        )
        .expect("column stores");

        assert_eq!(stores.len(), artifacts.witness.columns.len());
        for ((table_id, col_id), _) in &stores {
            assert!(
                artifacts
                    .witness
                    .columns
                    .iter()
                    .any(|column| column.table == *table_id && column.col == *col_id),
                "missing witness column for ({}, {})",
                table_id.0,
                col_id.0
            );
        }

        let traces = build_proof_traces(
            runtime.machine().setup().proof_setups(),
            partition_by_tier(prepared.store, stores),
        )
        .expect("proof traces");

        assert_eq!(
            traces.columns.len(),
            runtime.machine().setup().proof_setups().columns.len()
        );
    }
}
