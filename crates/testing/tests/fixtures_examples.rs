#![allow(missing_docs)]

use tabula_artifact::{ProgramArtifact, StateSnapshot, TransactionBatch, load_json};
use tabula_core::{ColId, RowKey, TableId, Value};
use tabula_executor::consistency::check_consistency;
use tabula_ir::{PrecompileId, PropertyQueryKind};
use tabula_testing::assertions::{
    assert_all_txs_success, assert_program_artifact_semantically_eq,
    assert_state_snapshot_semantically_eq, assert_transaction_batch_semantically_eq,
    assert_write_set_cell,
};
use tabula_testing::exec::{
    core_batch_from_artifact_batch, execute_batch_with_defaults, in_memory_state_from_cells,
};
use tabula_testing::fixtures::artifacts::precompile_requirement_artifact_case;
use tabula_testing::fixtures::cases::{arith_add_sub_trace_case, cmp_assert_trace_case};
use tabula_testing::fixtures::compiled::{
    compiled_property_successor_case, compiled_single_write_case,
};
use tabula_testing::fixtures::examples::{
    transfer_example_artifact_case, transfer_example_compiled_case, transfer_example_trace_case,
};
use tabula_testing::fs::{
    tempdir, write_batch_json, write_program_artifact_json, write_state_json,
};

#[test]
fn transfer_example_adapters_are_consistent() {
    let artifact_case = transfer_example_artifact_case();
    let compiled_case = transfer_example_compiled_case();
    let trace_case = transfer_example_trace_case();

    let artifact_batch =
        core_batch_from_artifact_batch(&artifact_case.batch).expect("convert artifact batch");

    assert_program_artifact_semantically_eq(
        &compiled_case.compiled_program.as_program_artifact(),
        &artifact_case.program_artifact,
    );
    assert_eq!(artifact_batch.transactions, trace_case.transactions);
    assert_eq!(
        tabula_testing::exec::initial_cells_from_state_snapshot(&artifact_case.state),
        trace_case.initial_cells
    );
}

#[test]
fn compiled_single_write_case_executes_via_public_executor_seam() {
    let case = compiled_single_write_case();
    let batch = core_batch_from_artifact_batch(&case.batch).expect("convert batch");
    let state = tabula_testing::exec::in_memory_state_from_snapshot(&case.state);
    let result = execute_batch_with_defaults(&batch, case.compiled_program.program(), &state)
        .expect("execute compiled case through public program seam");

    assert_all_txs_success(&result);
    assert_write_set_cell(
        &result,
        TableId(1),
        ColId(0),
        RowKey(0),
        Some(Value::U64(7)),
    );
}

#[test]
fn compiled_property_successor_case_records_expected_requirement() {
    let case = compiled_property_successor_case();
    let requirements = case.compiled_program.required_property_requirements();

    assert_eq!(requirements.len(), 1);
    assert_eq!(requirements[0].table_id, TableId(1));
    assert_eq!(requirements[0].col_id, ColId(0));
    assert_eq!(requirements[0].query_kind, PropertyQueryKind::Successor);
}

#[test]
fn precompile_requirement_artifact_case_records_expected_capability() {
    let case = precompile_requirement_artifact_case();

    assert_eq!(
        case.program_artifact.required_precompile_ids,
        vec![PrecompileId(0x0001)]
    );
}

#[test]
fn individual_json_helpers_round_trip_artifact_runtime_case() {
    let case = transfer_example_artifact_case();
    let dir = tempdir();
    let program_path =
        write_program_artifact_json(&dir, "transfer.program.json", &case.program_artifact);
    let state_path = write_state_json(&dir, "transfer.state.json", &case.state);
    let batch_path = write_batch_json(&dir, "transfer.batch.json", &case.batch);

    let program: ProgramArtifact = load_json(&program_path).expect("load program json");
    let state: StateSnapshot = load_json(&state_path).expect("load state json");
    let batch: TransactionBatch = load_json(&batch_path).expect("load batch json");

    assert_program_artifact_semantically_eq(&program, &case.program_artifact);
    assert_state_snapshot_semantically_eq(&state, &case.state);
    assert_transaction_batch_semantically_eq(&batch, &case.batch);
}

#[test]
fn transfer_example_trace_case_stays_consistent_under_executor() {
    let case = transfer_example_trace_case();
    let program = tabula_testing::exec::program_from_source(case.source);
    let batch = tabula_testing::exec::batch_from_transactions(case.transactions);
    let state = in_memory_state_from_cells(&case.initial_cells);
    let result = execute_batch_with_defaults(&batch, &program, &state).expect("execute trace case");

    assert_all_txs_success(&result);
    let all_events: Vec<_> = result.successful_events().cloned().collect();
    assert!(check_consistency(&all_events, &result.read_set_old, &result.txs).is_ok());
}

#[test]
fn arith_and_cmp_trace_cases_execute_successfully() {
    for case in [arith_add_sub_trace_case(), cmp_assert_trace_case()] {
        let program = tabula_testing::exec::program_from_source(case.source);
        let batch = tabula_testing::exec::batch_from_transactions(case.transactions);
        let state = in_memory_state_from_cells(&case.initial_cells);
        let result =
            execute_batch_with_defaults(&batch, &program, &state).expect("execute trace case");

        assert_all_txs_success(&result);
    }
}
