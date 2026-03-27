//! Execution error type for operation-level failures.

use tabula_core::error::TabulaError;

/// An error that occurred while executing a specific IR operation.
#[derive(Debug, Clone, thiserror::Error)]
#[error("op {op_index}: {error}")]
pub struct ExecuteError {
    /// The underlying executor error.
    #[source]
    pub error: TabulaError,
    /// Index of the IR operation that failed.
    pub op_index: usize,
}
