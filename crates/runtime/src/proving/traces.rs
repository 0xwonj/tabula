use rayon::prelude::*;
use tabula_commitment::PoseidonHasher;
use tabula_core::error::TabulaError;
use tabula_machine::{ProofSetups, ProofTraces, TabulaMachine, TierSetup};
use tabula_stark::trace::{TraceMap, WitnessStore};
use tabula_witness::trace::builtin::lowering::LoweringOutput;
use tabula_witness::trace::builtin::{BuiltinTraceBuilder, BuiltinTraceContext};
use tabula_witness::trace::{PartitionedStores, build_all_traces, partition_by_tier};

use crate::columns::BatchProofInput;
use crate::error::RuntimeError;

/// Build traces from canonical batch proof input through the full runtime-owned
/// pipeline: builtin lowering + per-column witness stores -> tier partition -> traces.
#[tracing::instrument(skip_all)]
pub fn build_traces(
    machine: &TabulaMachine,
    proof_input: BatchProofInput,
    lowering: &LoweringOutput,
) -> Result<ProofTraces, RuntimeError> {
    let metas = proof_input.column_metas();
    let prepared = BuiltinTraceBuilder::<PoseidonHasher, 3>::new(BuiltinTraceContext {
        column_metas: &metas,
        old_state_root: &proof_input.old_state_root,
        new_state_root: &proof_input.new_state_root,
    })
    .prepare_witness_store(lowering, PoseidonHasher::new())
    .map_err(RuntimeError::TraceBuild)?;

    let column_stores = build_column_stores(proof_input);
    let stores = partition_by_tier(prepared.store, column_stores);
    build_proof_traces(machine.setup().proof_setups(), stores).map_err(RuntimeError::TraceBuild)
}

fn build_column_stores(
    proof_input: BatchProofInput,
) -> Vec<((tabula_core::TableId, tabula_core::ColId), WitnessStore)> {
    proof_input
        .columns
        .into_iter()
        .map(|column| ((column.table, column.col), column.witness_store))
        .collect()
}

fn build_proof_traces(
    setups: &ProofSetups,
    stores: PartitionedStores,
) -> Result<ProofTraces, TabulaError> {
    let exec_traces = build_tier_traces(&setups.execution, stores.execution)?;

    let setup_index: std::collections::BTreeMap<(tabula_core::TableId, tabula_core::ColId), usize> =
        setups
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
    use tabula_testing::fixtures::examples::transfer_example_compiled_case;

    use super::*;
    use crate::TabulaRuntime;
    use crate::proving::prepare_witness_artifacts;

    #[test]
    fn canonical_column_proof_inputs_assemble_one_store_per_planned_column() {
        let case = transfer_example_compiled_case();
        let runtime = TabulaRuntime::builder(case.compiled_program)
            .build()
            .expect("runtime");
        let executed = runtime
            .execute(&case.state, &case.batch)
            .expect("execution succeeds");
        let artifacts = prepare_witness_artifacts(
            runtime.runtime_program(),
            &case.state,
            &case.batch,
            &executed,
        )
        .expect("witness artifacts");

        let column_count = artifacts.proof_input.columns.len();
        let expected_keys: Vec<_> = artifacts
            .proof_input
            .columns
            .iter()
            .map(|column| (column.table, column.col))
            .collect();
        let stores = build_column_stores(artifacts.proof_input);

        assert_eq!(stores.len(), column_count);
        for ((table_id, col_id), _) in &stores {
            assert!(
                expected_keys
                    .iter()
                    .any(|(table, col)| table == table_id && col == col_id),
                "missing proof input column for ({}, {})",
                table_id.0,
                col_id.0
            );
        }

        let artifacts = prepare_witness_artifacts(
            runtime.runtime_program(),
            &case.state,
            &case.batch,
            &executed,
        )
        .expect("witness artifacts");
        let proof_input = artifacts.proof_input;
        let traces = build_traces(runtime.machine(), proof_input, &artifacts.lowering)
            .expect("proof traces");

        assert_eq!(
            traces.columns.len(),
            runtime.machine().setup().proof_setups().columns.len()
        );
    }
}
