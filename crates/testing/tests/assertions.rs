#![allow(missing_docs)]

use std::panic::catch_unwind;

use tabula_artifact::{State, StateEntry};
use tabula_core::{ColId, RowKey, TableId};
use tabula_testing::assertions::{
    ExpectedStateCell, ExpectedTxOutcome, assert_all_txs_success, assert_artifact_semantically_eq,
    assert_state_cell, assert_state_cells_exact, assert_state_semantically_eq,
    assert_transaction_batch_semantically_eq, assert_tx_outcomes, assert_write_set_cell,
};
use tabula_testing::exec::{
    batch_from_transactions, execute_batch_with_defaults, program_from_source,
};
use tabula_testing::fixtures::cases::mixed_outcome_transfer_trace_case;
use tabula_testing::fixtures::examples::{
    transfer_example_artifact_case, transfer_example_compiled_case,
};
use tabula_testing::fixtures::state::three_account_balances;
use tabula_testing::runtime::execute_compiled_case;
use tabula_types::u64_portable;

#[test]
fn state_and_artifact_comparators_accept_semantically_equal_values() {
    let case = transfer_example_artifact_case();
    let reordered = State {
        cells: vec![
            StateEntry {
                table: 0,
                row: 2,
                col: 0,
                value: Some(u64_portable(200)),
            },
            StateEntry {
                table: 0,
                row: 0,
                col: 0,
                value: Some(u64_portable(1000)),
            },
            StateEntry {
                table: 0,
                row: 1,
                col: 0,
                value: Some(u64_portable(500)),
            },
            StateEntry {
                table: 0,
                row: 0,
                col: 0,
                value: Some(u64_portable(1000)),
            },
        ],
    };

    assert_artifact_semantically_eq(&case.artifact, &case.artifact);
    assert_state_semantically_eq(&case.state, &reordered);
    assert_transaction_batch_semantically_eq(&case.batch, &case.batch);
}

#[test]
fn semantic_comparators_panic_on_mismatch() {
    let transfer = transfer_example_artifact_case();
    let other = tabula_testing::fixtures::artifacts::precompile_requirement_artifact_case();

    assert!(
        catch_unwind(|| {
            assert_artifact_semantically_eq(&transfer.artifact, &other.artifact);
        })
        .is_err(),
        "artifact mismatch should panic"
    );
    assert!(
        catch_unwind(|| {
            assert_transaction_batch_semantically_eq(&transfer.batch, &other.batch);
        })
        .is_err(),
        "batch mismatch should panic"
    );
}

#[test]
fn tx_and_write_set_assertions_work_for_batch_results() {
    let case = mixed_outcome_transfer_trace_case();
    let program = program_from_source(case.source);
    let batch = batch_from_transactions(case.transactions);
    let state = tabula_testing::exec::in_memory_state_from_cells(&case.initial_cells);
    let result = execute_batch_with_defaults(&batch, &program, &state).expect("execute case");

    assert_tx_outcomes(
        &result,
        &[
            ExpectedTxOutcome::Success,
            ExpectedTxOutcome::Failed,
            ExpectedTxOutcome::Success,
        ],
    );
    assert_write_set_cell(
        &result,
        TableId(0),
        ColId(0),
        RowKey(0),
        Some(&u64_portable(700)),
    );
    assert_write_set_cell(
        &result,
        TableId(0),
        ColId(0),
        RowKey(2),
        Some(&u64_portable(300)),
    );
}

#[test]
fn state_and_success_assertions_work_for_runtime_results() {
    let compiled_case = transfer_example_compiled_case();
    let executed = execute_compiled_case(&compiled_case);

    assert_all_txs_success(&executed);
    assert_state_cell(
        &executed.state_after,
        TableId(0),
        ColId(0),
        RowKey(0),
        Some(&u64_portable(750)),
    );
    assert_state_cells_exact(
        &executed.state_after,
        &[
            ExpectedStateCell {
                table: TableId(0),
                col: ColId(0),
                row: RowKey(0),
                value: Some(u64_portable(750)),
            },
            ExpectedStateCell {
                table: TableId(0),
                col: ColId(0),
                row: RowKey(1),
                value: Some(u64_portable(600)),
            },
            ExpectedStateCell {
                table: TableId(0),
                col: ColId(0),
                row: RowKey(2),
                value: Some(u64_portable(350)),
            },
        ],
    );
    assert_state_semantically_eq(
        &executed.state_after,
        &three_account_balances(750, 600, 350),
    );
}
