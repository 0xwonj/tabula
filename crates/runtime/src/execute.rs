//! Canonical batch execution pipeline.
//!
//! Provides [`run_batch`] -- the single entry-point for executing a transaction
//! batch against a program and state. Both the CLI and daemon delegate here
//! instead of assembling the pipeline independently.

use std::collections::BTreeMap;

use tabula_artifact::{
    BatchFile, CompiledProgram, StateFile, merge_output_state_cells, normalize_state,
};
use tabula_core::traits::Hasher;
use tabula_core::{
    Batch, CellKey, ExecutionConsistencyStatus, InMemoryState, InMemoryStaticTables,
    NoopSigVerifier, SequentialNonce, TxResult, Value,
};
use tabula_executor::batch::{BatchEnv, execute_batch};
use tabula_executor::consistency::check_consistency_status;
use tabula_ir::Program;

use crate::error::RuntimeError;

/// Inputs for batch execution (all immutable references).
pub struct BatchInput<'a> {
    /// The IR program to execute.
    pub program: &'a Program,
    /// Pre-execution state.
    pub state: &'a StateFile,
    /// Transaction batch.
    pub batch: &'a BatchFile,
    /// Hasher implementation (Blake3Hasher for CLI, PoseidonHasher for STARK).
    pub hasher: &'a dyn Hasher,
}

/// Inputs for batch execution using a compiler-produced program artifact.
pub struct CompiledBatchInput<'a> {
    /// Semantic artifact produced by the compiler/registration phase.
    pub compiled_program: &'a CompiledProgram,
    /// Pre-execution state.
    pub state: &'a StateFile,
    /// Transaction batch.
    pub batch: &'a BatchFile,
    /// Hasher implementation (Blake3Hasher for CLI, PoseidonHasher for STARK).
    pub hasher: &'a dyn Hasher,
}

/// Result of executing a batch through the canonical pipeline.
#[derive(Debug, Clone)]
pub struct ExecutedBatch {
    /// Normalized pre-state.
    pub state_before: StateFile,
    /// Post-execution state.
    pub state_after: StateFile,
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
    // 1. Normalize state.
    let normalized = normalize_state(input.state).map_err(RuntimeError::InvalidState)?;

    // 2. Build in-memory state snapshot.
    let mut state_store = InMemoryState::new();
    for cell in &normalized.cells {
        let (key, value) = cell.to_cell_pair().map_err(RuntimeError::InvalidState)?;
        state_store.set(key, value);
    }

    // 3. Convert transactions.
    let transactions: Vec<_> = input
        .batch
        .transactions
        .iter()
        .map(|t| t.to_transaction().map_err(RuntimeError::InvalidBatch))
        .collect::<Result<_, _>>()?;
    let batch = Batch { transactions };

    // 4. Execute.
    let st = InMemoryStaticTables::new();
    let env = BatchEnv {
        hasher: input.hasher,
        sig_verifier: &NoopSigVerifier,
        nonce_policy: &SequentialNonce,
        static_tables: &st,
        precompiles: None,
        committed_state: None,
        property_openings: None,
    };

    let result = execute_batch(&batch, input.program, &state_store, &env, &BTreeMap::new())
        .map_err(|e| RuntimeError::Execution {
            source: e,
            instruction_index: None,
            tx_index: None,
        })?;

    // 5. Consistency check.
    let all_events: Vec<_> = result.successful_events().cloned().collect();
    let consistency = check_consistency_status(&all_events, &result.read_set_old, &result.txs);

    // 6. Merge output state.
    let state_after = StateFile {
        cells: merge_output_state_cells(&normalized.cells, &result.write_set_final),
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
    run_batch(&BatchInput {
        program: &input.compiled_program.program,
        state: input.state,
        batch: input.batch,
        hasher: input.hasher,
    })
}

#[cfg(test)]
mod tests {
    use tabula_artifact::{BatchFile, CompiledProgram, StateCell, TxInput};
    use tabula_contract::{
        BINDING_VERSION_V1, CONTRACT_SCHEMA_VERSION_V1, ContractMetadataEnvelope,
    };
    use tabula_core::mock::Blake3Hasher;
    use tabula_core::{TableId, TableSchema, TxTypeId, Value, ValueType};
    use tabula_ir::{Instruction, Program, RowExpr, TxTypeDef, ValueExpr};

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

        let mut program = Program::new();
        program.add_schema(schema.clone());
        program.register(tx_def.clone()).expect("register tx");

        CompiledProgram {
            program,
            table_schemas: vec![schema],
            tx_types: vec![tx_def],
            metadata_envelope: ContractMetadataEnvelope {
                profile_hash: [0; 32],
                contract_schema_version: CONTRACT_SCHEMA_VERSION_V1,
                binding_version: BINDING_VERSION_V1,
                semantic_hash_stub: None,
            },
        }
    }

    fn batch_file() -> BatchFile {
        BatchFile {
            transactions: vec![TxInput {
                tx_type: 1,
                params: vec![],
                sender: String::new(),
                nonce: 0,
            }],
        }
    }

    #[test]
    fn run_compiled_batch_applies_writes() {
        let compiled = compiled_program();
        let state = tabula_artifact::StateFile {
            cells: vec![StateCell {
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
        let invalid_state = tabula_artifact::StateFile {
            cells: vec![StateCell {
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
        let state = tabula_artifact::StateFile {
            cells: vec![StateCell {
                table: 1,
                row: 0,
                col: 0,
                value: Some(Value::U64(1)),
            }],
        };
        let empty_batch = BatchFile {
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
}
