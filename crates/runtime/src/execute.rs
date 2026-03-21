//! Canonical batch execution pipeline.
//!
//! Provides [`run_batch`] -- the single entry-point for executing a transaction
//! batch against a program and state. Both the CLI and daemon delegate here
//! instead of assembling the pipeline independently.

use std::collections::BTreeMap;

use tabula_artifact::{State, TransactionBatch, merge_output_state_entries, normalize_state};
use tabula_compiler::SealedProgram;
use tabula_core::traits::Hasher;
use tabula_core::{
    Batch, BatchResult, CellKey, ExecutionConsistencyStatus, InMemoryState, InMemoryStaticTables,
    NoopSigVerifier, SequentialNonce, TxResult, Value,
};
use tabula_executor::batch::{BatchEnv, execute_batch};
use tabula_executor::consistency::check_consistency_status;
use tabula_executor::precompile::PrecompileRegistry;
use tabula_executor::property::{CommittedStateProvider, PropertyQueryRegistry};
use tabula_ir::Program;

use crate::error::RuntimeError;

/// Inputs for batch execution (all immutable references).
pub struct BatchInput<'a> {
    /// The IR program to execute.
    pub program: &'a Program,
    /// Pre-execution state.
    pub state: &'a State,
    /// Transaction batch.
    pub batch: &'a TransactionBatch,
    /// Hasher implementation (Blake3Hasher for CLI, PoseidonHasher for STARK).
    pub hasher: &'a dyn Hasher,
}

/// Inputs for batch execution using a compiler-produced artifact.
pub struct CompiledBatchInput<'a> {
    /// Semantic artifact produced by the compiler/registration phase.
    pub compiled_program: &'a SealedProgram,
    /// Pre-execution state.
    pub state: &'a State,
    /// Transaction batch.
    pub batch: &'a TransactionBatch,
    /// Hasher implementation (Blake3Hasher for CLI, PoseidonHasher for STARK).
    pub hasher: &'a dyn Hasher,
}

/// Optional runtime-owned resources used during execution.
#[derive(Clone, Copy)]
pub(crate) struct ExecutionResources<'a> {
    pub precompiles: Option<&'a PrecompileRegistry>,
    pub committed_state: Option<&'a dyn CommittedStateProvider>,
    pub property_queries: &'a PropertyQueryRegistry,
}

/// Result of executing a batch through the canonical pipeline.
#[derive(Debug, Clone)]
pub struct ExecutedBatch {
    /// Normalized pre-state.
    pub state_before: State,
    /// Post-execution state.
    pub state_after: State,
    /// Canonical execution result.
    batch_result: BatchResult,
    /// Consistency check result.
    pub consistency: ExecutionConsistencyStatus,
}

impl ExecutedBatch {
    /// Canonical execution result for this batch.
    pub fn batch_result(&self) -> &BatchResult {
        &self.batch_result
    }

    /// Per-transaction outcomes in execution order.
    pub fn txs(&self) -> &[TxResult] {
        &self.batch_result.txs
    }

    /// Base-state reads observed by the executor.
    pub fn read_set(&self) -> &[(CellKey, Option<Value>)] {
        &self.batch_result.read_set_old
    }

    /// Final coalesced writes after execution.
    pub fn write_set(&self) -> &[(CellKey, Option<Value>)] {
        &self.batch_result.write_set_final
    }
}

/// Execute a batch through the canonical pipeline.
///
/// Steps:
/// 1. Normalize state
/// 2. Build in-memory state
/// 3. Convert transactions
/// 4. Execute batch
/// 5. Check consistency
/// 6. Merge output state
pub fn run_batch(input: &BatchInput<'_>) -> Result<ExecutedBatch, RuntimeError> {
    let property_queries = PropertyQueryRegistry::new();
    execute_pipeline(
        input.program,
        input.state,
        input.batch,
        input.hasher,
        ExecutionResources {
            precompiles: None,
            committed_state: None,
            property_queries: &property_queries,
        },
    )
}

pub(crate) fn execute_pipeline(
    program: &Program,
    state: &State,
    batch: &TransactionBatch,
    hasher: &dyn Hasher,
    resources: ExecutionResources<'_>,
) -> Result<ExecutedBatch, RuntimeError> {
    // 1. Normalize state.
    let normalized = normalize_state(state).map_err(RuntimeError::InvalidState)?;

    // 2. Build in-memory state.
    let mut state_store = InMemoryState::new();
    for cell in &normalized.cells {
        let (key, value) = cell.to_cell_pair().map_err(RuntimeError::InvalidState)?;
        state_store.set(key, value);
    }

    // 3. Convert transactions.
    let transactions: Vec<_> = batch
        .transactions
        .iter()
        .map(|t| t.to_transaction().map_err(RuntimeError::InvalidBatch))
        .collect::<Result<_, _>>()?;
    let batch_core = Batch { transactions };

    // 4. Execute.
    let st = InMemoryStaticTables::new();
    let env = BatchEnv {
        hasher,
        sig_verifier: &NoopSigVerifier,
        nonce_policy: &SequentialNonce,
        static_tables: &st,
        precompiles: resources.precompiles,
        committed_state: resources.committed_state,
        property_queries: resources.property_queries,
    };

    let result = execute_batch(&batch_core, program, &state_store, &env, &BTreeMap::new())
        .map_err(|e| RuntimeError::Execution {
            source: e,
            instruction_index: None,
            tx_index: None,
        })?;

    // 5. Consistency check.
    let all_events: Vec<_> = result.successful_events().cloned().collect();
    let consistency = check_consistency_status(&all_events, &result.read_set_old, &result.txs);

    // 6. Merge output state.
    let state_after = State {
        cells: merge_output_state_entries(&normalized.cells, &result.write_set_final),
    };

    Ok(ExecutedBatch {
        state_before: normalized,
        state_after,
        batch_result: result,
        consistency,
    })
}

/// Execute a batch using a compiler-produced artifact.
pub fn run_compiled_batch(input: &CompiledBatchInput<'_>) -> Result<ExecutedBatch, RuntimeError> {
    validate_free_execution_requirements(input.compiled_program)?;
    let property_queries = PropertyQueryRegistry::new();
    execute_pipeline(
        input.compiled_program.program(),
        input.state,
        input.batch,
        input.hasher,
        ExecutionResources {
            precompiles: None,
            committed_state: None,
            property_queries: &property_queries,
        },
    )
}

fn validate_free_execution_requirements(
    compiled_program: &SealedProgram,
) -> Result<(), RuntimeError> {
    if !compiled_program.precompile_manifest().is_empty() {
        return Err(RuntimeError::ValidationFailed {
            detail:
                "program requires precompiles; use TabulaRuntime::builder(...), register the required PrecompileRegistration values, and build before execution"
                    .to_string(),
        });
    }
    if !compiled_program.required_property_requirements().is_empty() {
        return Err(RuntimeError::ValidationFailed {
            detail:
                "program requires scheme-backed property queries; use TabulaRuntime::builder(...), install any required custom scheme factories, and build before execution"
                    .to_string(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use tabula_artifact::StateEntry;
    use tabula_core::Value;
    use tabula_core::mock::Blake3Hasher;
    use tabula_testing::fixtures::compiled::{
        compiled_empty_batch_case, compiled_precompile_requirement_case,
        compiled_property_successor_case, compiled_single_write_case,
    };
    use tabula_testing::fixtures::state::empty_state;

    use super::{CompiledBatchInput, RuntimeError, run_compiled_batch};

    #[test]
    fn run_compiled_batch_applies_writes() {
        let case = compiled_single_write_case();

        let executed = run_compiled_batch(&CompiledBatchInput {
            compiled_program: &case.compiled_program,
            state: &case.state,
            batch: &case.batch,
            hasher: &Blake3Hasher,
        })
        .expect("execute compiled batch");

        let updated = executed
            .state_after
            .cells
            .iter()
            .find(|cell| cell.table == 1 && cell.row == 0 && cell.col == 0)
            .and_then(|cell| cell.value);
        assert_eq!(updated, Some(Value::U64(7)));
    }

    #[test]
    fn run_compiled_batch_rejects_invalid_state() {
        let case = compiled_single_write_case();
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
        })
        .expect_err("invalid state must fail");

        assert!(matches!(err, RuntimeError::InvalidState(_)));
    }

    #[test]
    fn run_compiled_batch_handles_empty_batch() {
        let case = compiled_empty_batch_case();

        let executed = run_compiled_batch(&CompiledBatchInput {
            compiled_program: &case.compiled_program,
            state: &case.state,
            batch: &case.batch,
            hasher: &Blake3Hasher,
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
        let err = run_compiled_batch(&CompiledBatchInput {
            compiled_program: &case.compiled_program,
            state: &empty_state(),
            batch: &case.batch,
            hasher: &Blake3Hasher,
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
        let err = run_compiled_batch(&CompiledBatchInput {
            compiled_program: &case.compiled_program,
            state: &empty_state(),
            batch: &case.batch,
            hasher: &Blake3Hasher,
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
