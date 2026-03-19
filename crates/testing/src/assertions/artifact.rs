use tabula_artifact::{ProgramArtifact, StateSnapshot, TransactionBatch};

use crate::assertions::state::{ExpectedStateCell, assert_state_cells_exact};

/// Assert that two sealed program artifacts are semantically identical.
pub fn assert_program_artifact_semantically_eq(lhs: &ProgramArtifact, rhs: &ProgramArtifact) {
    assert_eq!(lhs.table_schemas, rhs.table_schemas, "table schemas differ");
    assert_eq!(lhs.tx_types, rhs.tx_types, "transaction types differ");
    assert_eq!(
        lhs.required_precompile_ids, rhs.required_precompile_ids,
        "required precompile ids differ"
    );
    assert_eq!(
        lhs.required_property_requirements, rhs.required_property_requirements,
        "required property requirements differ"
    );
    assert_eq!(
        lhs.column_proof_plan, rhs.column_proof_plan,
        "column proof plans differ"
    );
    assert_eq!(
        lhs.contract_metadata, rhs.contract_metadata,
        "contract metadata differs"
    );
}

/// Assert semantic equality of two state snapshots after normalization.
pub fn assert_state_snapshot_semantically_eq(lhs: &StateSnapshot, rhs: &StateSnapshot) {
    let normalized_rhs = tabula_artifact::normalize_state(rhs).expect("normalize expected state");
    let expected: Vec<_> = normalized_rhs
        .cells
        .iter()
        .map(|cell| ExpectedStateCell {
            table: tabula_core::TableId(cell.table),
            col: tabula_core::ColId(cell.col),
            row: tabula_core::RowKey(cell.row),
            value: cell.value,
        })
        .collect();
    assert_state_cells_exact(lhs, &expected);
}

/// Assert semantic equality of two transaction batches.
pub fn assert_transaction_batch_semantically_eq(lhs: &TransactionBatch, rhs: &TransactionBatch) {
    let lhs_projection: Vec<_> = lhs
        .transactions
        .iter()
        .map(|tx| (tx.tx_type, tx.params.clone(), tx.sender.clone(), tx.nonce))
        .collect();
    let rhs_projection: Vec<_> = rhs
        .transactions
        .iter()
        .map(|tx| (tx.tx_type, tx.params.clone(), tx.sender.clone(), tx.nonce))
        .collect();
    assert_eq!(lhs_projection, rhs_projection, "transaction batches differ");
}
