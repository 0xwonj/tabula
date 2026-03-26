use tabula_core::error::TabulaError;

#[derive(Debug, Clone, thiserror::Error)]
#[error("op {op_index}: {error}")]
pub struct ExecuteError {
    #[source]
    pub error: TabulaError,
    pub op_index: usize,
}
