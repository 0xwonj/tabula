use std::collections::BTreeMap;

use tabula_core::{BatchReport, CellKey, ColId, PortableValue, RowKey, TableId, TxResult};
use tabula_runtime::ExecutionEnvelope;

/// Canonical expected outcome for one transaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExpectedTxOutcome {
    Success,
    Failed,
}

/// Minimal semantic projection for result types that carry transaction outcomes.
pub trait TxOutcomeView {
    fn tx_results(&self) -> &[TxResult];
}

/// Minimal semantic projection for result types that carry a final write-set.
pub trait WriteSetView {
    fn write_set_entries(&self) -> &[(CellKey, Option<PortableValue>)];
}

impl TxOutcomeView for BatchReport {
    fn tx_results(&self) -> &[TxResult] {
        &self.txs
    }
}

impl TxOutcomeView for ExecutionEnvelope {
    fn tx_results(&self) -> &[TxResult] {
        self.txs()
    }
}

impl WriteSetView for BatchReport {
    fn write_set_entries(&self) -> &[(CellKey, Option<PortableValue>)] {
        &self.write_set_final
    }
}

impl WriteSetView for ExecutionEnvelope {
    fn write_set_entries(&self) -> &[(CellKey, Option<PortableValue>)] {
        self.write_set()
    }
}

/// Assert that every transaction succeeded.
pub fn assert_all_txs_success<T: TxOutcomeView>(result: &T) {
    let outcomes: Vec<_> = result
        .tx_results()
        .iter()
        .map(|tx| {
            if tx.is_success() {
                ExpectedTxOutcome::Success
            } else {
                ExpectedTxOutcome::Failed
            }
        })
        .collect();
    assert!(
        outcomes
            .iter()
            .all(|outcome| *outcome == ExpectedTxOutcome::Success),
        "expected all transactions to succeed, got {outcomes:?}"
    );
}

/// Assert exact transaction success/failure outcomes in batch order.
pub fn assert_tx_outcomes<T: TxOutcomeView>(result: &T, expected: &[ExpectedTxOutcome]) {
    let actual: Vec<_> = result
        .tx_results()
        .iter()
        .map(|tx| {
            if tx.is_success() {
                ExpectedTxOutcome::Success
            } else {
                ExpectedTxOutcome::Failed
            }
        })
        .collect();
    assert_eq!(actual, expected, "transaction outcomes differ");
}

/// Assert one final write-set value by logical cell key.
pub fn assert_write_set_cell<T: WriteSetView>(
    result: &T,
    table: TableId,
    col: ColId,
    row: RowKey,
    expected: Option<&PortableValue>,
) {
    let writes: BTreeMap<_, _> = result.write_set_entries().iter().cloned().collect();
    let actual = writes
        .get(&CellKey { table, col, row })
        .and_then(std::option::Option::as_ref);
    assert_eq!(
        actual, expected,
        "write-set mismatch at ({}, {}, {})",
        table.0, col.0, row.0
    );
}
