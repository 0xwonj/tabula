#![allow(missing_docs)]

use tabula_core::{ColId, RowKey, TableId, Value};
use tabula_runtime::RuntimeError;
use tabula_testing::assertions::{
    assert_all_txs_success, assert_runtime_consistency_passed, assert_state_after_matches_expected,
    assert_statement_matches_artifact, assert_write_set_cell,
};
use tabula_testing::fixtures::compiled::{
    compiled_precompile_requirement_case, compiled_single_write_case,
};
use tabula_testing::fixtures::examples::{
    transfer_example_artifact_case, transfer_example_compiled_case,
};
use tabula_testing::fixtures::state::three_account_balances;
use tabula_testing::runtime::{
    execute_artifact_case, execute_compiled_case, execute_compiled_case_free,
    prove_and_verify_artifact_case, prove_compiled_case, verify_artifact_case,
};

#[test]
fn compiled_case_execute_uses_canonical_runtime_harness() {
    let case = compiled_single_write_case();
    let executed = execute_compiled_case(&case);

    assert_all_txs_success(&executed);
    assert_runtime_consistency_passed(&executed);
    assert_write_set_cell(
        &executed,
        TableId(1),
        ColId(0),
        RowKey(0),
        Some(Value::U64(7)),
    );
}

#[test]
fn artifact_case_execute_uses_canonical_runtime_harness() {
    let case = transfer_example_artifact_case();
    let executed = execute_artifact_case(&case);

    assert_all_txs_success(&executed);
    assert_runtime_consistency_passed(&executed);
    assert_state_after_matches_expected(&executed, &three_account_balances(750, 600, 350));
}

#[test]
fn prove_and_verify_happy_path_works_through_runtime_harness() {
    let artifact_case = transfer_example_artifact_case();
    let compiled_case = transfer_example_compiled_case();
    let proved = prove_compiled_case(&compiled_case);
    let verified = prove_and_verify_artifact_case(&artifact_case);

    assert_statement_matches_artifact(&proved.statement, &artifact_case.program_artifact);
    assert_statement_matches_artifact(&verified.statement, &artifact_case.program_artifact);
    assert!(verified.verified, "prove_and_verify should verify");
    assert!(
        !verified.proof.columns.is_empty(),
        "verified proof should include column proofs"
    );
    verify_artifact_case(&artifact_case, &proved).expect("artifact verifier accepts runtime proof");
}

#[test]
fn required_precompile_case_fails_via_free_execution_harness() {
    let compiled_case = compiled_precompile_requirement_case();

    let err = execute_compiled_case_free(&compiled_case)
        .expect_err("required precompile case should fail under free execution");

    match err {
        RuntimeError::ValidationFailed { detail } => {
            assert!(detail.contains("precompile"), "unexpected detail: {detail}");
        }
        other => panic!("unexpected error: {other}"),
    }
}
