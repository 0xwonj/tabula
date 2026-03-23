#![allow(missing_docs)]

use tabula_core::{ColId, TableId};
use tabula_stark::trace::build_all_traces;
use tabula_testing::fixtures::cases::touch_trace_case;
use tabula_testing::witness::{
    build_trace_map, compile_execute_case, lower_program_batch_for_harness, prepare_witness_store,
};

#[test]
fn stark_trace_builder_executes_all_core_chips_e2e() {
    let case = touch_trace_case();
    let setup = compile_execute_case(&case);
    let access_effects: Vec<_> = setup
        .execution_journal
        .successful_access_effects()
        .filter(|effect| effect.key.table == TableId(0) && effect.key.col == ColId(0))
        .collect();

    assert_eq!(access_effects.len(), 2);
    assert_eq!(access_effects[0].effect_ordinal_in_tx, 0);
    assert_eq!(access_effects[1].effect_ordinal_in_tx, 1);

    let trace_map =
        build_trace_map::<3>(&setup).expect("all-chip trace assembly from execution result");
    assert!(!trace_map.chip_ids().is_empty());
}

#[test]
fn stark_shared_store_builds_core_traces_directly() {
    let case = touch_trace_case();
    let setup = compile_execute_case(&case);
    let lowering = lower_program_batch_for_harness::<3>(&setup);
    let store = prepare_witness_store::<3>(&setup, &lowering).expect("witness store");

    let chips = tabula_chips::core_dyn_chips();
    let consumers = tabula_chips::core_bus_consumers();
    let trace_map = build_all_traces(&chips, &consumers, store).expect("trace map");

    assert!(!trace_map.chip_ids().is_empty());
}
