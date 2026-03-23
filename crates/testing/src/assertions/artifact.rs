use tabula_artifact::{Artifact, State, TransactionBatch};

use crate::assertions::state::{ExpectedStateCell, assert_state_cells_exact};

/// Assert that two sealed artifacts are semantically identical.
pub fn assert_artifact_semantically_eq(lhs: &Artifact, rhs: &Artifact) {
    assert_eq!(lhs.table_schemas, rhs.table_schemas, "table schemas differ");
    assert_eq!(
        lhs.profile_catalog, rhs.profile_catalog,
        "profile catalogs differ"
    );
    assert_eq!(lhs.tx_types, rhs.tx_types, "transaction types differ");
    assert_eq!(
        lhs.precompile_manifest, rhs.precompile_manifest,
        "precompile manifests differ"
    );
    assert_eq!(
        lhs.required_property_requirements, rhs.required_property_requirements,
        "required property requirements differ"
    );
    assert_eq!(
        lhs.contract_metadata, rhs.contract_metadata,
        "contract metadata differs"
    );
}

/// Assert semantic equality of two states after normalization.
pub fn assert_state_semantically_eq(lhs: &State, rhs: &State) {
    let normalized_rhs = tabula_artifact::normalize_state(rhs).expect("normalize expected state");
    let expected: Vec<_> = normalized_rhs
        .cells
        .iter()
        .map(|cell| ExpectedStateCell {
            table: tabula_core::TableId(cell.table),
            col: tabula_core::ColId(cell.col),
            row: tabula_core::RowKey(cell.row),
            value: cell.value.clone(),
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
