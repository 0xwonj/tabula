//! Integration tests: multi-tx batch execution with mixed outcomes and determinism.

use tabula_core::{ColId, RowKey, TableId};
use tabula_executor::consistency::check_consistency;
use tabula_testing::assertions::{
    ExpectedTxOutcome, assert_all_txs_success, assert_tx_outcomes, assert_write_set_cell,
};
use tabula_testing::exec::{
    batch_from_transactions, execute_batch_with_defaults, in_memory_state_from_cells,
    program_from_source,
};
use tabula_testing::fixtures::cases::mixed_outcome_transfer_trace_case;
use tabula_testing::fixtures::examples::transfer_example_trace_case;
use tabula_types::u64_portable;

#[test]
fn test_multi_tx_mixed_outcomes() {
    let case = mixed_outcome_transfer_trace_case();
    let program = program_from_source(case.source);
    let state = in_memory_state_from_cells(&case.initial_cells);
    let batch = batch_from_transactions(case.transactions);
    let result = execute_batch_with_defaults(&batch, &program, &state).unwrap();

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
        RowKey(1),
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
fn test_deterministic_execution() {
    let case = transfer_example_trace_case();
    let program = program_from_source(case.source);
    let state = in_memory_state_from_cells(&case.initial_cells);
    let batch = batch_from_transactions(case.transactions);

    let r1 = execute_batch_with_defaults(&batch, &program, &state).unwrap();
    let r2 = execute_batch_with_defaults(&batch, &program, &state).unwrap();

    assert_eq!(r1.read_set_old, r2.read_set_old);
    assert_eq!(r1.write_set_final, r2.write_set_final);
    assert_eq!(r1.txs, r2.txs);
}

#[test]
fn test_consistency_passes_for_valid_batch() {
    let case = transfer_example_trace_case();
    let program = program_from_source(case.source);
    let state = in_memory_state_from_cells(&case.initial_cells);
    let batch = batch_from_transactions(case.transactions);

    let result = execute_batch_with_defaults(&batch, &program, &state).unwrap();

    assert_all_txs_success(&result);
    let all_events: Vec<_> = result.successful_events().cloned().collect();
    assert!(check_consistency(&all_events, &result.read_set_old, &result.txs).is_ok());
}
