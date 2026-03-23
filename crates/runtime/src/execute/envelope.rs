use tabula_artifact::State;
use tabula_core::{BatchReport, CellKey, ExecutionConsistencyStatus, PortableValue, TxResult};
use tabula_executor::ExecutionJournal;

/// Result of executing a batch through the canonical pipeline.
#[derive(Debug, Clone)]
pub struct ExecutionEnvelope {
    /// Normalized pre-state.
    pub state_before: State,
    /// Post-execution state.
    pub state_after: State,
    /// Canonical internal execution result.
    execution_journal: ExecutionJournal,
    /// Reporting projection of the canonical execution result.
    batch_report: BatchReport,
    /// Consistency check result.
    pub consistency: ExecutionConsistencyStatus,
}

impl ExecutionEnvelope {
    pub(crate) fn new(
        state_before: State,
        state_after: State,
        execution_journal: ExecutionJournal,
        batch_report: BatchReport,
        consistency: ExecutionConsistencyStatus,
    ) -> Self {
        Self {
            state_before,
            state_after,
            execution_journal,
            batch_report,
            consistency,
        }
    }

    /// Canonical internal execution journal for this batch.
    pub fn execution_journal(&self) -> &ExecutionJournal {
        &self.execution_journal
    }

    /// Reporting projection for this executed batch.
    pub fn batch_report(&self) -> &BatchReport {
        &self.batch_report
    }

    /// Per-transaction outcomes in execution order.
    pub fn txs(&self) -> &[TxResult] {
        &self.batch_report.txs
    }

    /// Base-state reads observed by the executor.
    pub fn read_set(&self) -> &[(CellKey, Option<PortableValue>)] {
        &self.batch_report.read_set_old
    }

    /// Final coalesced writes after execution.
    pub fn write_set(&self) -> &[(CellKey, Option<PortableValue>)] {
        &self.batch_report.write_set_final
    }
}
