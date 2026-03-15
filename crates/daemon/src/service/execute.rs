//! Batch execution pipeline — delegates to the runtime's canonical pipeline.

use tabula_artifact::{BatchFile, CompiledProgram, ExecutionSummary, StateCell, StateFile};
use tabula_core::traits::Hasher;
use tabula_runtime::{CompiledBatchInput, RuntimeError, run_compiled_batch};

use super::error::{ServiceError, ServiceResult};
use crate::protocol::error::ErrorCode;

/// Result of executing a compiled batch.
#[derive(Debug, Clone)]
pub struct ExecutedBatch {
    pub compiled_program: CompiledProgram,
    pub batch_file: BatchFile,
    pub inner: tabula_runtime::ExecutedBatch,
}

impl ExecutedBatch {
    pub fn into_execution_summary(self, include_trace: bool) -> ExecutionSummary {
        let all_events: Vec<_> = self
            .inner
            .txs
            .iter()
            .flat_map(|tx| tx.access_trace())
            .cloned()
            .collect();
        let trace = if include_trace {
            Some(all_events)
        } else {
            None
        };

        let emitted: Vec<_> = self
            .inner
            .txs
            .iter()
            .filter_map(|tx| match tx {
                tabula_core::TxResult::Success { emitted, .. } => Some(emitted.iter()),
                _ => None,
            })
            .flatten()
            .cloned()
            .collect();

        ExecutionSummary {
            tx_results: self.inner.txs,
            read_set: self
                .inner
                .read_set
                .iter()
                .map(|(k, v)| StateCell::from_cell_pair(k, v))
                .collect(),
            write_set: self
                .inner
                .write_set
                .iter()
                .map(|(k, v)| StateCell::from_cell_pair(k, v))
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
    compiled_program: CompiledProgram,
    state_file: &StateFile,
    batch_file: BatchFile,
    hasher: &dyn Hasher,
) -> ServiceResult<ExecutedBatch> {
    let inner = run_compiled_batch(&CompiledBatchInput {
        compiled_program: &compiled_program,
        state: state_file,
        batch: &batch_file,
        hasher,
    })
    .map_err(|e| map_runtime_execution_error(&e))?;

    Ok(ExecutedBatch {
        compiled_program,
        batch_file,
        inner,
    })
}

#[cfg(feature = "stark")]
pub fn execute_prepared_batch(
    compiled_program: CompiledProgram,
    runtime: &tabula_runtime::PreparedRuntime,
    state_file: &StateFile,
    batch_file: BatchFile,
) -> ServiceResult<ExecutedBatch> {
    let inner = runtime
        .execute(state_file, &batch_file)
        .map_err(|e| map_runtime_execution_error(&e))?;

    Ok(ExecutedBatch {
        compiled_program,
        batch_file,
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
