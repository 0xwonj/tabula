use tabula_artifact::State;
#[cfg(feature = "prove")]
use tabula_artifact::TransactionBatch;
use tabula_core::{CellKey, ExecutionConsistencyStatus, PortableValue, TxResult};
use tabula_runtime::ExecutedBatch;

/// SDK wrapper over one completed execution plus the original inputs used to produce it.
#[derive(Debug, Clone)]
pub struct Execution {
    #[cfg(feature = "prove")]
    pub(crate) program_hash: String,
    #[cfg(feature = "prove")]
    pub(crate) state: State,
    #[cfg(feature = "prove")]
    pub(crate) batch: TransactionBatch,
    pub(crate) inner: ExecutedBatch,
}

impl Execution {
    #[cfg(feature = "prove")]
    pub(crate) fn new(
        program_hash: String,
        state: State,
        batch: TransactionBatch,
        inner: ExecutedBatch,
    ) -> Self {
        Self {
            program_hash,
            state,
            batch,
            inner,
        }
    }

    #[cfg(not(feature = "prove"))]
    pub(crate) fn new(inner: ExecutedBatch) -> Self {
        Self { inner }
    }

    /// The normalized pre-execution state.
    pub fn state_before(&self) -> &State {
        &self.inner.state_before
    }

    /// The post-execution state.
    pub fn state_after(&self) -> &State {
        &self.inner.state_after
    }

    /// Per-transaction execution results.
    pub fn txs(&self) -> &[TxResult] {
        self.inner.txs()
    }

    /// The observed read-set over committed state.
    pub fn read_set(&self) -> &[(CellKey, Option<PortableValue>)] {
        self.inner.read_set()
    }

    /// The final write-set after execution.
    pub fn write_set(&self) -> &[(CellKey, Option<PortableValue>)] {
        self.inner.write_set()
    }

    /// The execution consistency status.
    pub fn consistency(&self) -> ExecutionConsistencyStatus {
        self.inner.consistency.clone()
    }
}
