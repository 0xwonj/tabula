//! Batch execution pipeline — delegates to the driver's canonical pipeline.

use tabula_artifact::{BatchFile, StateCell, StateFile};
use tabula_core::traits::Hasher;
use tabula_driver::{BatchInput, RegisteredProgram};

use super::error::{ServiceError, ServiceResult};
use super::types::ExecutionResult;
use crate::protocol::error::ErrorCode;

/// Result of executing a registered batch.
#[derive(Debug, Clone)]
pub struct ExecutedBatch {
    pub artifact: RegisteredProgram,
    pub batch_file: BatchFile,
    pub inner: tabula_driver::ExecutedBatch,
}

impl ExecutedBatch {
    pub fn into_execution_result(self, include_trace: bool) -> ExecutionResult {
        let trace = if include_trace {
            Some(self.inner.events.clone())
        } else {
            None
        };

        ExecutionResult {
            tx_outcomes: self.inner.tx_outcomes,
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
            emitted: self.inner.emitted,
            consistency: self.inner.consistency,
            trace,
            state_after: self.inner.state_after,
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
    let inner = tabula_driver::run_batch(&BatchInput {
        program: &artifact.program,
        state: &state_file,
        batch: &batch_file,
        hasher,
    })
    .map_err(map_driver_execution_error)?;

    Ok(ExecutedBatch {
        artifact,
        batch_file,
        inner,
    })
}

fn map_driver_execution_error(err: tabula_driver::DriverError) -> ServiceError {
    match &err {
        tabula_driver::DriverError::InvalidState(source) => {
            ServiceError::bad_request(ErrorCode::InvalidStateCell, source.to_string())
        }
        tabula_driver::DriverError::InvalidBatch(source) => {
            ServiceError::bad_request(ErrorCode::InvalidBatchTx, source.to_string())
        }
        tabula_driver::DriverError::Execution { source, .. } => {
            ServiceError::unprocessable(ErrorCode::ExecutionError, source.to_string())
        }
        _ => ServiceError::internal(ErrorCode::InternalError, err.to_string()),
    }
}
