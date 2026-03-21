//! Batch execution pipeline — delegates to the runtime's canonical pipeline.

use tabula_artifact::{State, StateEntry, TransactionBatch};
use tabula_compiler::SealedProgram;
use tabula_core::traits::Hasher;
use tabula_runtime::{CompiledBatchInput, RuntimeError, run_compiled_batch};

use super::ExecutionSummary;
use super::error::{ServiceError, ServiceResult};
use crate::protocol::error::ErrorCode;

/// Result of executing a compiled batch.
#[derive(Debug, Clone)]
pub struct ExecutedBatch {
    pub compiled_program: SealedProgram,
    pub transaction_batch: TransactionBatch,
    pub inner: tabula_runtime::ExecutedBatch,
}

impl ExecutedBatch {
    pub fn into_execution_summary(self, include_trace: bool) -> ExecutionSummary {
        let all_events: Vec<_> = self
            .inner
            .txs()
            .iter()
            .flat_map(tabula_core::TxResult::access_trace)
            .cloned()
            .collect();
        let trace = if include_trace {
            Some(all_events)
        } else {
            None
        };

        let emitted: Vec<_> = self
            .inner
            .txs()
            .iter()
            .filter_map(|tx| match tx {
                tabula_core::TxResult::Success { emitted, .. } => Some(emitted.iter()),
                _ => None,
            })
            .flatten()
            .cloned()
            .collect();

        ExecutionSummary {
            tx_results: self.inner.txs().to_vec(),
            read_set: self
                .inner
                .read_set()
                .iter()
                .map(|(k, v)| StateEntry::from_cell_pair(k, v))
                .collect(),
            write_set: self
                .inner
                .write_set()
                .iter()
                .map(|(k, v)| StateEntry::from_cell_pair(k, v))
                .collect(),
            emitted,
            consistency: self.inner.consistency,
            trace,
            state_after: self.inner.state_after,
        }
    }
}

/// Execute a batch against a compiled program and state.
pub fn execute_compiled_batch(
    compiled_program: SealedProgram,
    state: &State,
    transaction_batch: TransactionBatch,
    hasher: &dyn Hasher,
) -> ServiceResult<ExecutedBatch> {
    let inner = run_compiled_batch(&CompiledBatchInput {
        compiled_program: &compiled_program,
        state: state,
        batch: &transaction_batch,
        hasher,
    })
    .map_err(|e| map_runtime_execution_error(&e))?;

    Ok(ExecutedBatch {
        compiled_program,
        transaction_batch,
        inner,
    })
}

#[cfg(feature = "stark")]
pub fn execute_prepared_batch(
    compiled_program: SealedProgram,
    runtime: &tabula_runtime::TabulaRuntime,
    state: &State,
    transaction_batch: TransactionBatch,
) -> ServiceResult<ExecutedBatch> {
    let inner = runtime
        .execute(state, &transaction_batch)
        .map_err(|e| map_runtime_execution_error(&e))?;

    Ok(ExecutedBatch {
        compiled_program,
        transaction_batch,
        inner,
    })
}

#[cfg(not(feature = "stark"))]
fn map_runtime_execution_error(err: &RuntimeError) -> ServiceError {
    match err {
        RuntimeError::InvalidState(source) => {
            ServiceError::bad_request(ErrorCode::InvalidStateCell, source.to_string())
        }
        RuntimeError::InvalidBatch(source) => {
            ServiceError::bad_request(ErrorCode::InvalidBatchTx, source.to_string())
        }
        RuntimeError::Execution { source, .. } => {
            ServiceError::unprocessable(ErrorCode::ExecutionError, source.to_string())
        }
        _ => ServiceError::internal(ErrorCode::InternalError, err.to_string()),
    }
}

#[cfg(feature = "stark")]
fn map_runtime_execution_error(err: &RuntimeError) -> ServiceError {
    match err {
        RuntimeError::InvalidState(source) => {
            ServiceError::bad_request(ErrorCode::InvalidStateCell, source.to_string())
        }
        RuntimeError::InvalidBatch(source) => {
            ServiceError::bad_request(ErrorCode::InvalidBatchTx, source.to_string())
        }
        RuntimeError::Execution { source, .. } => {
            ServiceError::unprocessable(ErrorCode::ExecutionError, source.to_string())
        }
        _ => ServiceError::internal(ErrorCode::InternalError, err.to_string()),
    }
}
