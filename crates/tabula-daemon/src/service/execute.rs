//! Batch execution pipeline.

use std::collections::BTreeMap;

use tabula_artifact::{BatchFile, StateCell, StateFile, merge_output_state_cells, normalize_state};
use tabula_core::mock::{InMemoryState, InMemoryStaticTables, MockSigVerifier, SequentialNonce};
use tabula_core::traits::Hasher;
use tabula_core::{Batch, CellKey, ExecutionConsistencyStatus, Value};
use tabula_driver::RegisteredProgram;
use tabula_executor::batch::{BatchEnv, execute_batch};
use tabula_executor::consistency::check_consistency_status;

use super::error::{ServiceError, ServiceResult};
use super::types::ExecutionResult;
use crate::protocol::error::ErrorCode;

/// Result of executing a registered batch.
#[derive(Debug, Clone)]
pub struct ExecutedBatch {
    pub artifact: RegisteredProgram,
    pub state_file: StateFile,
    pub batch_file: BatchFile,
    pub tx_outcomes: Vec<tabula_core::TxOutcome>,
    pub read_set: Vec<(CellKey, Option<Value>)>,
    pub write_set: Vec<(CellKey, Option<Value>)>,
    pub emitted: Vec<tabula_core::EmittedEvent>,
    pub events: Vec<tabula_core::ExecutionEvent>,
    pub consistency: ExecutionConsistencyStatus,
    pub state_after: StateFile,
}

impl ExecutedBatch {
    pub fn into_execution_result(self, include_trace: bool) -> ExecutionResult {
        let trace = if include_trace {
            Some(self.events.clone())
        } else {
            None
        };

        ExecutionResult {
            tx_outcomes: self.tx_outcomes,
            read_set: self
                .read_set
                .iter()
                .map(|(k, v)| StateCell::from_cell_pair(k, v))
                .collect(),
            write_set: self
                .write_set
                .iter()
                .map(|(k, v)| StateCell::from_cell_pair(k, v))
                .collect(),
            emitted: self.emitted,
            consistency: self.consistency,
            trace,
            state_after: self.state_after,
        }
    }
}

/// Execute a batch against a registered program and state.
pub fn execute_registered_batch(
    artifact: RegisteredProgram,
    state_file: StateFile,
    batch_file: BatchFile,
    hasher: &dyn Hasher,
) -> ServiceResult<ExecutedBatch> {
    let normalized_state = normalize_state(&state_file)
        .map_err(|e| ServiceError::bad_request(ErrorCode::InvalidStateCell, e.to_string()))?;

    let mut state_store = InMemoryState::new();
    for cell in &normalized_state.cells {
        let (key, value) = cell
            .to_cell_pair()
            .map_err(|e| ServiceError::bad_request(ErrorCode::InvalidStateCell, e.to_string()))?;
        state_store.set(key, value);
    }

    let transactions: Vec<_> = batch_file
        .transactions
        .iter()
        .map(|t| {
            t.to_transaction()
                .map_err(|e| ServiceError::bad_request(ErrorCode::InvalidBatchTx, e.to_string()))
        })
        .collect::<Result<_, _>>()?;
    let batch_value = Batch { transactions };

    let st = InMemoryStaticTables::new();
    let env = BatchEnv {
        hasher,
        sig_verifier: &MockSigVerifier,
        nonce_policy: &SequentialNonce,
        static_tables: &st,
    };

    let result = execute_batch(
        &batch_value,
        &artifact.program,
        &state_store,
        &env,
        &BTreeMap::new(),
    )
    .map_err(|e| ServiceError::unprocessable(ErrorCode::ExecutionError, e.to_string()))?;

    let consistency = check_consistency_status(&result.events, &result.read_set_old);
    let state_after = StateFile {
        cells: merge_output_state_cells(&normalized_state.cells, &result.write_set_final),
    };

    Ok(ExecutedBatch {
        artifact,
        state_file: normalized_state,
        batch_file,
        tx_outcomes: result.tx_outcomes,
        read_set: result.read_set_old,
        write_set: result.write_set_final,
        emitted: result.emitted,
        events: result.events,
        consistency,
        state_after,
    })
}
