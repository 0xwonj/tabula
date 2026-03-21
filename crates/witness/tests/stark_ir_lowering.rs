#![allow(missing_docs)]

use tabula_core::TxResult;
use tabula_testing::fixtures::cases::{
    arith_add_sub_trace_case, cmp_assert_trace_case, single_transfer_trace_case, touch_trace_case,
};
use tabula_testing::fixtures::examples::success_transfer_with_emit_trace_case;
use tabula_testing::witness::{
    build_and_validate_trace_map, build_trace_map, compile_execute_case,
    debug_validate_core_trace_map, lower_program_batch_for_harness,
};

fn lower_build_validate(case: &tabula_testing::witness::StarkTraceHarness) {
    build_and_validate_trace_map::<3>(case).expect("constraint + bus validation");
}

#[test]
fn trace_builder_arith_add_sub_ir_lowering_e2e() {
    let case = arith_add_sub_trace_case();
    let setup = compile_execute_case(&case);
    assert!(matches!(setup.result.txs[0], TxResult::Success { .. }));
    lower_build_validate(&setup);
}

#[test]
fn trace_builder_cmp_assert_ir_lowering_e2e() {
    let case = cmp_assert_trace_case();
    let setup = compile_execute_case(&case);
    assert!(matches!(setup.result.txs[0], TxResult::Success { .. }));
    lower_build_validate(&setup);
}

#[test]
fn trace_builder_full_pipeline_e2e() {
    let case = touch_trace_case();
    let setup = compile_execute_case(&case);
    assert!(matches!(setup.result.txs[0], TxResult::Success { .. }));

    let trace_map = build_trace_map::<3>(&setup).expect("trace assembly");
    debug_validate_core_trace_map(&trace_map)
        .expect("unified pipeline must satisfy all constraints");
}

#[test]
fn trace_builder_transfer_param_materialization_e2e() {
    let case = single_transfer_trace_case();
    let setup = compile_execute_case(&case);
    assert!(matches!(setup.result.txs[0], TxResult::Success { .. }));

    let lowering = lower_program_batch_for_harness::<3>(&setup);
    assert!(!lowering.instruction_records.is_empty());
    lower_build_validate(&setup);
}

#[test]
fn trace_builder_transfer_3tx_with_emit_e2e() {
    let case = success_transfer_with_emit_trace_case();
    let setup = compile_execute_case(&case);
    for (i, outcome) in setup.result.txs.iter().enumerate() {
        assert!(
            matches!(outcome, TxResult::Success { .. }),
            "tx {i} should succeed, got: {outcome:?}"
        );
    }
    lower_build_validate(&setup);
}
