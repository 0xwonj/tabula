//! Canonical batch execution pipeline.
//!
//! Provides [`run_batch`] -- the single entry-point for executing a transaction
//! batch against a program and state. Both the CLI and daemon delegate here
//! instead of assembling the pipeline independently.

use std::collections::BTreeMap;

use tabula_artifact::{BatchFile, StateFile, merge_output_state_cells, normalize_state};
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
    /// The registered IR program.
    pub program: &'a Program,
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
