#![allow(missing_docs)]

use tabula_core::TxResult;
use tabula_testing::fixtures::cases::single_transfer_trace_case;
use tabula_testing::fixtures::examples::success_transfer_with_emit_trace_case;
use tabula_testing::witness::{build_and_validate_trace_map, compile_execute_case};

#[test]
fn compile_execute_witness_pipeline_round_trips_from_shared_harness() {
    let case = single_transfer_trace_case();
    let setup = compile_execute_case(&case);

    assert!(
        matches!(setup.result.txs[0], TxResult::Success { .. }),
        "transaction should succeed before witness generation"
    );

    build_and_validate_trace_map::<3>(&setup).expect("shared E2E witness pipeline");
}

#[test]
fn transfer_example_trace_case_round_trips_through_shared_witness_harness() {
    let case = success_transfer_with_emit_trace_case();
    let setup = compile_execute_case(&case);

    assert!(
        setup.result.txs.iter().all(TxResult::is_success),
        "example trace case should execute successfully before witness generation"
    );

    build_and_validate_trace_map::<3>(&setup).expect("example trace case witness pipeline");
}
