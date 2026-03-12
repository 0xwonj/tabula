//! Canonical batch execution pipeline.
//!
//! Provides [`run_batch`] — the single entry-point for executing a transaction
//! batch against a program and state. Both the CLI and daemon delegate here
//! instead of assembling the pipeline independently.

use std::collections::BTreeMap;

use tabula_artifact::{BatchFile, StateFile, merge_output_state_cells, normalize_state};
use tabula_core::mock::{InMemoryState, InMemoryStaticTables, MockSigVerifier, SequentialNonce};
use tabula_core::traits::Hasher;
use tabula_core::{
    Batch, CellKey, ExecutionConsistencyStatus, TxResult, Value,
};
use tabula_executor::batch::{BatchEnv, execute_batch};
use tabula_executor::consistency::check_consistency_status;
use tabula_ir::Program;

use crate::error::DriverError;

/// Inputs for batch execution (all immutable references).
pub struct BatchInput<'a> {
    /// The registered IR program.
    pub program: &'a Program,
    /// Pre-execution state.
    pub state: &'a StateFile,
    /// Transaction batch.
    pub batch: &'a BatchFile,
    /// Hasher implementation (MockHasher for CLI, PoseidonHasher for STARK).
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
pub fn run_batch(input: &BatchInput<'_>) -> Result<ExecutedBatch, DriverError> {
    // 1. Normalize state.
    let normalized = normalize_state(input.state).map_err(DriverError::InvalidState)?;

    // 2. Build in-memory state snapshot.
    let mut state_store = InMemoryState::new();
    for cell in &normalized.cells {
        let (key, value) = cell.to_cell_pair().map_err(DriverError::InvalidState)?;
        state_store.set(key, value);
    }

    // 3. Convert transactions.
    let transactions: Vec<_> = input
        .batch
        .transactions
        .iter()
        .map(|t| t.to_transaction().map_err(DriverError::InvalidBatch))
        .collect::<Result<_, _>>()?;
    let batch = Batch { transactions };

    // 4. Execute.
    let st = InMemoryStaticTables::new();
    let env = BatchEnv {
        hasher: input.hasher,
        sig_verifier: &MockSigVerifier,
        nonce_policy: &SequentialNonce,
        static_tables: &st,
        precompiles: None,
    };

    let result = execute_batch(&batch, input.program, &state_store, &env, &BTreeMap::new())
        .map_err(|e| DriverError::Execution {
            source: e,
            instruction_index: None,
            tx_index: None,
        })?;

    // 5. Consistency check.
    let all_events: Vec<_> = result.successful_events().cloned().collect();
    let consistency = check_consistency_status(&all_events, &result.read_set_old);

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

#[cfg(test)]
mod tests {
    use super::*;
    use tabula_artifact::StateCell;
    use tabula_core::Value;
    use tabula_core::mock::MockHasher;

    use crate::example::transfer_example_bundle;
    use crate::register::{MetadataPolicy, register_program_sources};

    #[test]
    fn run_batch_transfer_example() {
        let bundle = transfer_example_bundle().expect("example bundle");
        let registered =
            register_program_sources(&bundle.program, MetadataPolicy::Optional).expect("register");

        let executed = run_batch(&BatchInput {
            program: &registered.program,
            state: &bundle.state,
            batch: &bundle.batch,
            hasher: &MockHasher,
        })
        .expect("run_batch");

        assert_eq!(executed.txs.len(), 3);
        assert!(executed.txs.iter().all(|tx| tx.is_success()));

        // After transfers: row0=1000-300+50=750, row1=500+300-200=600, row2=200+200-50=350
        let val = |row: u64| -> Option<Value> {
            executed
                .state_after
                .cells
                .iter()
                .find(|c| c.table == 0 && c.col == 0 && c.row == row)
                .and_then(|c| c.value)
        };
        assert_eq!(val(0), Some(Value::U64(750)));
        assert_eq!(val(1), Some(Value::U64(600)));
        assert_eq!(val(2), Some(Value::U64(350)));

        assert!(matches!(
            executed.consistency,
            ExecutionConsistencyStatus::Passed
        ));
    }

    #[test]
    fn run_batch_invalid_state() {
        let bundle = transfer_example_bundle().expect("example bundle");
        let registered =
            register_program_sources(&bundle.program, MetadataPolicy::Optional).expect("register");

        let bad_state = StateFile {
            cells: vec![StateCell {
                table: 0,
                row: 0,
                col: 0,
                value: None, // missing value
            }],
        };

        let err = run_batch(&BatchInput {
            program: &registered.program,
            state: &bad_state,
            batch: &bundle.batch,
            hasher: &MockHasher,
        })
        .expect_err("invalid state should fail");
        assert!(matches!(err, DriverError::InvalidState(_)));
    }

    #[test]
    fn run_batch_empty_batch() {
        let bundle = transfer_example_bundle().expect("example bundle");
        let registered =
            register_program_sources(&bundle.program, MetadataPolicy::Optional).expect("register");

        let empty_batch = BatchFile {
            transactions: vec![],
        };

        let executed = run_batch(&BatchInput {
            program: &registered.program,
            state: &bundle.state,
            batch: &empty_batch,
            hasher: &MockHasher,
        })
        .expect("run_batch");

        assert!(executed.txs.is_empty());
        assert!(executed.write_set.is_empty());
        // State passthrough: output equals normalized input
        assert_eq!(
            executed.state_after.cells.len(),
            executed.state_before.cells.len()
        );
    }
}
