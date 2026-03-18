//! Canonical batch execution pipeline.
//!
//! Provides [`run_batch`] -- the single entry-point for executing a transaction
//! batch against a program and state. Both the CLI and daemon delegate here
//! instead of assembling the pipeline independently.

use std::collections::BTreeMap;

use tabula_artifact::{
    StateSnapshot, TransactionBatch, merge_output_state_entries, normalize_state,
};
use tabula_compiler::CompiledProgram;
use tabula_core::traits::Hasher;
use tabula_core::{
    Batch, CellKey, ExecutionConsistencyStatus, InMemoryState, InMemoryStaticTables,
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
    pub state: &'a StateSnapshot,
    /// Transaction batch.
    pub batch: &'a TransactionBatch,
    /// Hasher implementation (Blake3Hasher for CLI, PoseidonHasher for STARK).
    pub hasher: &'a dyn Hasher,
}

/// Inputs for batch execution using a compiler-produced program artifact.
pub struct CompiledBatchInput<'a> {
    /// Semantic artifact produced by the compiler/registration phase.
    pub compiled_program: &'a CompiledProgram,
    /// Pre-execution state.
    pub state: &'a StateSnapshot,
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
    pub state_before: StateSnapshot,
    /// Post-execution state.
    pub state_after: StateSnapshot,
    /// Per-transaction results (each carries its own access trace and emitted events).
    pub txs: Vec<TxResult>,
    /// Read set from base state.
    pub read_set: Vec<(CellKey, Option<Value>)>,
    /// Final write set.
    pub write_set: Vec<(CellKey, Option<Value>)>,
    /// Consistency check result.
    pub consistency: ExecutionConsistencyStatus,
}

/// Execute a batch through the canonical pipeline.
///
/// Steps:
/// 1. Normalize state
/// 2. Build in-memory state snapshot
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
    state: &StateSnapshot,
    batch: &TransactionBatch,
    hasher: &dyn Hasher,
    resources: ExecutionResources<'_>,
) -> Result<ExecutedBatch, RuntimeError> {
    // 1. Normalize state.
    let normalized = normalize_state(state).map_err(RuntimeError::InvalidState)?;

    // 2. Build in-memory state snapshot.
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
    let state_after = StateSnapshot {
        cells: merge_output_state_entries(&normalized.cells, &result.write_set_final),
    };

    Ok(ExecutedBatch {
        state_before: normalized,
        state_after,
        txs: result.txs,
        read_set: result.read_set_old,
        write_set: result.write_set_final,
        consistency,
    })
}

/// Execute a batch using a compiler-produced program artifact.
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
    compiled_program: &CompiledProgram,
) -> Result<(), RuntimeError> {
    if !compiled_program.required_precompile_ids().is_empty() {
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
    use tabula_artifact::{StateEntry, StateSnapshot, TransactionBatch, TransactionInput};
    use tabula_compiler::{CompiledProgram, register_program};
    use tabula_core::mock::Blake3Hasher;
    use tabula_core::{TableId, TableSchema, TxTypeId, Value, ValueType};
    use tabula_ir::{Instruction, PrecompileId, PropertyQuery, RowExpr, TxTypeDef, ValueExpr};

    use super::{CompiledBatchInput, RuntimeError, run_compiled_batch};
    fn compiled_program() -> CompiledProgram {
        let schema = TableSchema {
            id: TableId(1),
            name: "accounts".to_string(),
            columns: vec![tabula_core::ColumnDef {
                id: tabula_core::ColId(0),
                name: "balance".to_string(),
                value_type: ValueType::U64,
            }],
        };
        let tx_def = TxTypeDef {
            id: TxTypeId(1),
            name: "set_balance".to_string(),
            param_schema: vec![],
            body: vec![Instruction::Write {
                table: TableId(1),
                row: RowExpr::Literal(tabula_core::RowKey(0)),
                col: tabula_core::ColId(0),
                src_val: ValueExpr::Literal(Value::U64(7)),
                src_is_null: ValueExpr::Literal(Value::Bool(false)),
            }],
        };

        register_program(&[schema], &[tx_def]).expect("register program")
    }

    fn batch_file() -> TransactionBatch {
        TransactionBatch {
            transactions: vec![TransactionInput {
                tx_type: 1,
                params: vec![],
                sender: String::new(),
                nonce: 0,
            }],
        }
    }

    fn compiled_program_with_precompile() -> CompiledProgram {
        register_program(
            &[],
            &[TxTypeDef {
                id: TxTypeId(1),
                name: "call".to_string(),
                param_schema: vec![],
                body: vec![Instruction::Precompile {
                    id: PrecompileId(7),
                    dst_slots: vec![0],
                    inputs: vec![ValueExpr::Literal(Value::U64(1))],
                }],
            }],
        )
        .expect("register precompile program")
    }

    fn compiled_program_with_property_query() -> CompiledProgram {
        let schema = TableSchema {
            id: TableId(1),
            name: "accounts".to_string(),
            columns: vec![tabula_core::ColumnDef {
                id: tabula_core::ColId(0),
                name: "balance".to_string(),
                value_type: ValueType::U64,
            }],
        };
        register_program(
            &[schema],
            &[TxTypeDef {
                id: TxTypeId(1),
                name: "scan".to_string(),
                param_schema: vec![],
                body: vec![Instruction::PropertyRead {
                    dst_val: 0,
                    dst_key: 1,
                    dst_is_null: 2,
                    table: TableId(1),
                    col: tabula_core::ColId(0),
                    query: PropertyQuery::Successor {
                        key: tabula_core::RowKey(0),
                    },
                }],
            }],
        )
        .expect("register property program")
    }

    #[test]
    fn run_compiled_batch_applies_writes() {
        let compiled = compiled_program();
        let state = StateSnapshot {
            cells: vec![StateEntry {
                table: 1,
                row: 0,
                col: 0,
                value: Some(Value::U64(1)),
            }],
        };

        let executed = run_compiled_batch(&CompiledBatchInput {
            compiled_program: &compiled,
            state: &state,
            batch: &batch_file(),
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
        let compiled = compiled_program();
        let invalid_state = StateSnapshot {
            cells: vec![StateEntry {
                table: 1,
                row: 0,
                col: 0,
                value: None,
            }],
        };

        let err = run_compiled_batch(&CompiledBatchInput {
            compiled_program: &compiled,
            state: &invalid_state,
            batch: &batch_file(),
            hasher: &Blake3Hasher,
        })
        .expect_err("invalid state must fail");

        assert!(matches!(err, RuntimeError::InvalidState(_)));
    }

    #[test]
    fn run_compiled_batch_handles_empty_batch() {
        let compiled = compiled_program();
        let state = StateSnapshot {
            cells: vec![StateEntry {
                table: 1,
                row: 0,
                col: 0,
                value: Some(Value::U64(1)),
            }],
        };
        let empty_batch = TransactionBatch {
            transactions: vec![],
        };

        let executed = run_compiled_batch(&CompiledBatchInput {
            compiled_program: &compiled,
            state: &state,
            batch: &empty_batch,
            hasher: &Blake3Hasher,
        })
        .expect("empty batch should succeed");

        assert!(executed.txs.is_empty());
        assert!(executed.write_set.is_empty());
        assert_eq!(
            executed.state_after.cells.len(),
            executed.state_before.cells.len()
        );
    }

    #[test]
    fn run_compiled_batch_rejects_required_precompiles() {
        let compiled = compiled_program_with_precompile();
        let err = run_compiled_batch(&CompiledBatchInput {
            compiled_program: &compiled,
            state: &StateSnapshot { cells: vec![] },
            batch: &batch_file(),
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
        let compiled = compiled_program_with_property_query();
        let err = run_compiled_batch(&CompiledBatchInput {
            compiled_program: &compiled,
            state: &StateSnapshot { cells: vec![] },
            batch: &batch_file(),
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
