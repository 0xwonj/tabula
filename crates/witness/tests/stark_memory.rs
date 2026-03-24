#![allow(missing_docs)]

use tabula_core::{ColId, TableId};
use tabula_stark::trace::{build_all_traces, witness_labels};
use tabula_testing::fixtures::cases::touch_trace_case;
use tabula_testing::witness::{
    build_trace_map, compile_execute_case, lower_program_batch_for_harness, prepare_witness_store,
};
use tabula_witness::stark::{prepare_execution_store, prepare_smt_root_store};

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
fn stark_execution_and_root_stores_build_core_traces_directly() {
    let case = touch_trace_case();
    let setup = compile_execute_case(&case);
    let lowering = lower_program_batch_for_harness::<3>(&setup);
    let store = prepare_witness_store::<3>(&setup, &lowering).expect("witness store");

    let chips = tabula_chips::core_dyn_chips();
    let consumers = tabula_chips::core_bus_consumers();
    let trace_map = build_all_traces(&chips, &consumers, store).expect("trace map");

    assert!(!trace_map.chip_ids().is_empty());
}

#[test]
fn kernel_execution_and_root_store_functions_produce_expected_labels() {
    let case = touch_trace_case();
    let setup = compile_execute_case(&case);
    let lowering = lower_program_batch_for_harness::<3>(&setup);
    let execution_store = prepare_execution_store(&lowering).expect("execution store");
    let root_store = prepare_smt_root_store(
        setup.smt_root_store_context(),
        tabula_commitment::PoseidonHasher::new(),
    )
    .expect("root store");

    assert!(
        execution_store.contains::<Vec<tabula_chips::execution::trace::InstructionRecord>>(
            witness_labels::EXECUTION_RECORDS
        )
    );
    assert!(
        execution_store.contains::<Vec<tabula_chips::static_table::trace::StaticTableRow>>(
            witness_labels::STATIC_TABLE_ROWS
        )
    );
    assert!(
        root_store.contains::<Vec<tabula_chips::smt_path::trace::SmtPathWitness>>(
            witness_labels::SMT_COL_PATHS
        )
    );
    assert!(
        root_store.contains::<Vec<tabula_chips::smt_path::trace::SmtTablePathWitness>>(
            witness_labels::SMT_TABLE_PATHS
        )
    );
    assert!(root_store.contains::<Vec<p3_koala_bear::KoalaBear>>(witness_labels::SMT_TABLE_PVS));
}
