//! Canonical semantic assertions for shared black-box tests.

mod artifact;
mod batch;
mod proof;
mod runtime;
mod state;

pub use artifact::{
    assert_program_artifact_semantically_eq, assert_state_snapshot_semantically_eq,
    assert_transaction_batch_semantically_eq,
};
pub use batch::{
    ExpectedTxOutcome, TxOutcomeView, WriteSetView, assert_all_txs_success, assert_tx_outcomes,
    assert_write_set_cell,
};
pub use proof::{assert_proof_verifies, assert_statement_matches_artifact};
pub use runtime::{assert_runtime_consistency_passed, assert_state_after_matches_expected};
pub use state::{ExpectedStateCell, assert_state_cell, assert_state_cells_exact};
