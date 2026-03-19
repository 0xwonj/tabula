use super::*;
use tabula_testing::fixtures::cases::{
    arith_add_sub_trace_case, cmp_assert_trace_case, single_transfer_trace_case, touch_trace_case,
};
use tabula_testing::fixtures::examples::success_transfer_with_emit_trace_case;
use tabula_testing::witness::{
    BuiltinTraceHarness, build_and_validate_trace_map, build_trace_map, compile_execute_case,
    debug_validate_core_trace_map, lower_program_batch_for_harness,
};

/// Run IR-based lowering + full trace build + validation.
pub(super) fn lower_build_validate(setup: &BuiltinTraceHarness) {
    build_and_validate_trace_map::<3>(setup).expect("constraint + bus validation");
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
fn trace_builder_transfer_param_debug_lowering() {
    use p3_field::PrimeField32;

    let case = single_transfer_trace_case();
    let setup = compile_execute_case(&case);
    let lowering = lower_program_batch_for_harness::<3>(&setup);
    for (i, rec) in lowering.instruction_records.iter().enumerate() {
        eprintln!(
            "  rec[{i}]: opcode={:?} tx={} written_slots={:?} src1_idx={:?} src2_idx={:?} writes={:?}",
            rec.opcode,
            rec.tx_index,
            rec.written_slots,
            rec.src1_slot_idx,
            rec.src2_slot_idx,
            rec.writes
                .iter()
                .map(|(s, v, n)| (
                    s,
                    v.iter()
                        .map(|f: &KoalaBear| f.as_canonical_u32())
                        .collect::<Vec<_>>(),
                    n
                ))
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn trace_builder_transfer_param_materialization_e2e() {
    let case = single_transfer_trace_case();
    let setup = compile_execute_case(&case);
    assert!(matches!(setup.result.txs[0], TxResult::Success { .. }));
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
