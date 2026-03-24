use tabula_artifact::{State, TransactionBatch, merge_output_state_entries, normalize_state};
use tabula_core::traits::Hasher;
use tabula_core::{Batch, InMemoryState, InMemoryStaticTables};
use tabula_executor::property::PropertyQueryRegistry;
use tabula_executor::{
    ResolvedExecutionProgram,
    batch::{BatchEnv, execute_batch},
    derive_batch_report, derive_consistency_status, derive_portable_state_summary,
};
use tabula_types::TypeRuntimeRegistry;

use crate::error::RuntimeError;
use crate::policy::{validate_execution_state_surface, validate_free_execution_requirements};

use super::envelope::ExecutionEnvelope;
use super::inputs::{BatchInput, CompiledBatchInput, ExecutionResources};

/// Execute a batch through the canonical pipeline.
///
/// Steps:
/// 1. Normalize state
/// 2. Build in-memory state
/// 3. Convert transactions
/// 4. Execute batch
/// 5. Check consistency
/// 6. Merge output state
pub fn run_batch(input: &BatchInput<'_>) -> Result<ExecutionEnvelope, RuntimeError> {
    let property_queries = PropertyQueryRegistry::new();
    let resolved_program =
        ResolvedExecutionProgram::from_program(input.program).map_err(|source| {
            RuntimeError::Execution {
                source,
                instruction_index: None,
                tx_index: None,
            }
        })?;
    execute_pipeline(
        &resolved_program,
        input.state,
        input.batch,
        input.hasher,
        input.type_runtimes,
        ExecutionResources {
            precompiles: None,
            committed_state: None,
            property_queries: &property_queries,
        },
    )
}

pub(crate) fn execute_pipeline(
    program: &ResolvedExecutionProgram,
    state: &State,
    batch: &TransactionBatch,
    hasher: &dyn Hasher,
    type_runtimes: &TypeRuntimeRegistry,
    resources: ExecutionResources<'_>,
) -> Result<ExecutionEnvelope, RuntimeError> {
    let normalized = normalize_state(state).map_err(RuntimeError::InvalidState)?;
    validate_execution_state_surface(program, &normalized)?;

    let mut state_store = InMemoryState::new();
    for cell in &normalized.cells {
        let (key, value) = cell
            .to_cell_pair(type_runtimes)
            .map_err(RuntimeError::InvalidState)?;
        state_store.set(key, value);
    }

    let transactions: Vec<_> = batch
        .transactions
        .iter()
        .map(|t| {
            t.to_transaction(type_runtimes)
                .map_err(RuntimeError::InvalidBatch)
        })
        .collect::<Result<_, _>>()?;
    let batch_core = Batch { transactions };

    let st = InMemoryStaticTables::new();
    let env = BatchEnv {
        hasher,
        type_runtimes,
        static_tables: &st,
        precompiles: resources.precompiles,
        committed_state: resources.committed_state,
        property_queries: resources.property_queries,
    };

    let execution_journal =
        execute_batch(&batch_core, program, &state_store, &env).map_err(|e| {
            RuntimeError::Execution {
                source: e,
                instruction_index: None,
                tx_index: None,
            }
        })?;
    let batch_report = derive_batch_report(&execution_journal, type_runtimes).map_err(|e| {
        RuntimeError::Execution {
            source: e,
            instruction_index: None,
            tx_index: None,
        }
    })?;
    let portable_state_summary =
        derive_portable_state_summary(&execution_journal.state_summary, type_runtimes).map_err(
            |e| RuntimeError::Execution {
                source: e,
                instruction_index: None,
                tx_index: None,
            },
        )?;
    let consistency = derive_consistency_status(&execution_journal, type_runtimes);
    let state_after = State {
        cells: merge_output_state_entries(
            &normalized.cells,
            &portable_state_summary.write_set_final,
        ),
    };

    Ok(ExecutionEnvelope::new(
        normalized,
        state_after,
        execution_journal,
        batch_report,
        consistency,
    ))
}

/// Execute a batch using a compiler-produced artifact.
pub fn run_compiled_batch(
    input: &CompiledBatchInput<'_>,
) -> Result<ExecutionEnvelope, RuntimeError> {
    validate_free_execution_requirements(input.compiled_program)?;
    let property_queries = PropertyQueryRegistry::new();
    let resolved_program = ResolvedExecutionProgram::from_program(input.compiled_program.program())
        .map_err(|source| RuntimeError::Execution {
            source,
            instruction_index: None,
            tx_index: None,
        })?;
    execute_pipeline(
        &resolved_program,
        input.state,
        input.batch,
        input.hasher,
        input.type_runtimes,
        ExecutionResources {
            precompiles: None,
            committed_state: None,
            property_queries: &property_queries,
        },
    )
}

#[cfg(test)]
mod tests {
    use tabula_artifact::{State, StateEntry, merge_output_state_entries};
    use tabula_core::mock::Blake3Hasher;
    use tabula_executor::derive_portable_state_summary;
    use tabula_testing::fixtures::compiled::{
        compiled_empty_batch_case, compiled_precompile_requirement_case,
        compiled_property_successor_case, compiled_single_write_case,
    };
    use tabula_testing::fixtures::state::empty_state;
    use tabula_types::{TypeRuntimeRegistry, u64_portable};

    use super::{BatchInput, CompiledBatchInput, run_batch, run_compiled_batch};
    use crate::RuntimeError;

    fn state_with_extra_surface_cell(mut state: State) -> State {
        state.cells.push(StateEntry {
            table: 99,
            row: 0,
            col: 0,
            value: Some(u64_portable(9)),
        });
        state
    }

    #[test]
    fn run_compiled_batch_applies_writes() {
        let case = compiled_single_write_case();
        let type_runtimes = TypeRuntimeRegistry::seeded().expect("seeded type runtimes");

        let executed = run_compiled_batch(&CompiledBatchInput {
            compiled_program: &case.compiled_program,
            state: &case.state,
            batch: &case.batch,
            hasher: &Blake3Hasher,
            type_runtimes: &type_runtimes,
        })
        .expect("execute compiled batch");

        let updated = executed
            .state_after
            .cells
            .iter()
            .find(|cell| cell.table == 1 && cell.row == 0 && cell.col == 0)
            .and_then(|cell| cell.value.clone());
        assert_eq!(updated, Some(u64_portable(7)));
    }

    #[test]
    fn run_compiled_batch_state_after_comes_from_journal_state_summary() {
        let case = compiled_single_write_case();
        let type_runtimes = TypeRuntimeRegistry::seeded().expect("seeded type runtimes");

        let executed = run_compiled_batch(&CompiledBatchInput {
            compiled_program: &case.compiled_program,
            state: &case.state,
            batch: &case.batch,
            hasher: &Blake3Hasher,
            type_runtimes: &type_runtimes,
        })
        .expect("execute compiled batch");

        let portable_state = derive_portable_state_summary(
            &executed.execution_journal().state_summary,
            &type_runtimes,
        )
        .expect("portable state summary");
        let merged = merge_output_state_entries(
            &executed.state_before.cells,
            &portable_state.write_set_final,
        );
        let actual_after: Vec<_> = executed
            .state_after
            .cells
            .iter()
            .map(|cell| (cell.table, cell.row, cell.col, cell.value.clone()))
            .collect();
        let expected_after: Vec<_> = merged
            .iter()
            .map(|cell| (cell.table, cell.row, cell.col, cell.value.clone()))
            .collect();
        assert_eq!(actual_after, expected_after);
        assert_eq!(
            executed.batch_report().write_set_final,
            portable_state.write_set_final
        );
    }

    #[test]
    fn run_compiled_batch_rejects_invalid_state() {
        let case = compiled_single_write_case();
        let type_runtimes = TypeRuntimeRegistry::seeded().expect("seeded type runtimes");
        let invalid_state = tabula_artifact::State {
            cells: vec![StateEntry {
                table: 1,
                row: 0,
                col: 0,
                value: None,
            }],
        };

        let err = run_compiled_batch(&CompiledBatchInput {
            compiled_program: &case.compiled_program,
            state: &invalid_state,
            batch: &case.batch,
            hasher: &Blake3Hasher,
            type_runtimes: &type_runtimes,
        })
        .expect_err("invalid state must fail");

        assert!(matches!(err, RuntimeError::InvalidState(_)));
    }

    #[test]
    fn run_batch_rejects_state_outside_declared_surface() {
        let case = compiled_single_write_case();
        let type_runtimes = TypeRuntimeRegistry::seeded().expect("seeded type runtimes");
        let invalid_state = state_with_extra_surface_cell(case.state.clone());

        let err = run_batch(&BatchInput {
            program: case.compiled_program.program(),
            state: &invalid_state,
            batch: &case.batch,
            hasher: &Blake3Hasher,
            type_runtimes: &type_runtimes,
        })
        .expect_err("state outside execution surface must fail");

        match err {
            RuntimeError::ValidationFailed { detail } => {
                assert!(detail.contains("outside the declared program state surface"));
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn run_compiled_batch_rejects_state_outside_declared_surface() {
        let case = compiled_single_write_case();
        let type_runtimes = TypeRuntimeRegistry::seeded().expect("seeded type runtimes");
        let invalid_state = state_with_extra_surface_cell(case.state.clone());

        let err = run_compiled_batch(&CompiledBatchInput {
            compiled_program: &case.compiled_program,
            state: &invalid_state,
            batch: &case.batch,
            hasher: &Blake3Hasher,
            type_runtimes: &type_runtimes,
        })
        .expect_err("state outside execution surface must fail");

        match err {
            RuntimeError::ValidationFailed { detail } => {
                assert!(detail.contains("outside the declared program state surface"));
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn run_compiled_batch_handles_empty_batch() {
        let case = compiled_empty_batch_case();
        let type_runtimes = TypeRuntimeRegistry::seeded().expect("seeded type runtimes");

        let executed = run_compiled_batch(&CompiledBatchInput {
            compiled_program: &case.compiled_program,
            state: &case.state,
            batch: &case.batch,
            hasher: &Blake3Hasher,
            type_runtimes: &type_runtimes,
        })
        .expect("empty batch should succeed");

        assert!(executed.txs().is_empty());
        assert!(executed.write_set().is_empty());
        assert_eq!(
            executed.state_after.cells.len(),
            executed.state_before.cells.len()
        );
    }

    #[test]
    fn run_compiled_batch_rejects_required_precompiles() {
        let case = compiled_precompile_requirement_case();
        let type_runtimes = TypeRuntimeRegistry::seeded().expect("seeded type runtimes");
        let err = run_compiled_batch(&CompiledBatchInput {
            compiled_program: &case.compiled_program,
            state: &empty_state(),
            batch: &case.batch,
            hasher: &Blake3Hasher,
            type_runtimes: &type_runtimes,
        })
        .expect_err("free execute should reject required precompiles");

        match err {
            RuntimeError::ValidationFailed { detail } => {
                assert!(detail.contains("requires precompiles"));
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn run_compiled_batch_rejects_required_property_requirements() {
        let case = compiled_property_successor_case();
        let type_runtimes = TypeRuntimeRegistry::seeded().expect("seeded type runtimes");
        let err = run_compiled_batch(&CompiledBatchInput {
            compiled_program: &case.compiled_program,
            state: &empty_state(),
            batch: &case.batch,
            hasher: &Blake3Hasher,
            type_runtimes: &type_runtimes,
        })
        .expect_err("free execute should reject required property requirements");

        match err {
            RuntimeError::ValidationFailed { detail } => {
                assert!(detail.contains("requires scheme-backed property queries"));
            }
            other => panic!("unexpected error: {other}"),
        }
    }
}
