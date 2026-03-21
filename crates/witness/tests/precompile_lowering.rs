#![allow(missing_docs)]

use std::collections::{BTreeMap, BTreeSet};

use tabula_core::{
    BatchResult, InMemoryStaticTables, PrecompileEvent, TableId, TableSchema, TxResult, Value,
};
use tabula_testing::exec::{compiled_program_from_artifact, core_batch_from_artifact_batch};
use tabula_testing::fixtures::artifacts::precompile_requirement_artifact;
use tabula_testing::fixtures::batch::single_tx_batch;
use tabula_witness::stark::lower_program_batch;

fn batch_result_with_precompile_events(events: Vec<PrecompileEvent>) -> BatchResult {
    BatchResult {
        read_set_old: vec![],
        write_set_final: vec![],
        txs: vec![TxResult::Success {
            emitted: vec![],
            access_trace: vec![],
            precompile_events: events,
            property_reads: vec![],
        }],
    }
}

fn valid_event() -> PrecompileEvent {
    PrecompileEvent {
        tx_index: 0,
        instruction_index: 0,
        precompile_id: 0x0001,
        inputs: vec![],
        outputs: vec![Value::U64(1)],
    }
}

fn lower_with_events(
    events: Vec<PrecompileEvent>,
) -> Result<tabula_witness::stark::LoweringOutput, tabula_core::error::TabulaError> {
    let sealed = compiled_program_from_artifact(&precompile_requirement_artifact());
    let batch = core_batch_from_artifact_batch(&single_tx_batch(1, vec![])).expect("core batch");
    let schemas: BTreeMap<TableId, TableSchema> = sealed
        .table_schemas()
        .iter()
        .cloned()
        .map(|schema| (schema.id, schema))
        .collect();
    lower_program_batch::<3>(
        sealed.program(),
        &batch,
        &batch_result_with_precompile_events(events),
        &schemas,
        &InMemoryStaticTables::new(),
        &BTreeSet::new(),
    )
}

#[test]
fn precompile_lowering_accepts_matching_event() {
    let lowering = lower_with_events(vec![valid_event()]).expect("lowering");
    assert_eq!(lowering.instruction_records.len(), 1);
}

#[test]
fn precompile_lowering_rejects_missing_event() {
    let err = lower_with_events(vec![]).expect_err("missing event must fail");
    assert!(err.to_string().contains("missing precompile event"));
}

#[test]
fn precompile_lowering_rejects_duplicate_call_key() {
    let event = valid_event();
    let err = lower_with_events(vec![event.clone(), event]).expect_err("duplicate event must fail");
    assert!(err.to_string().contains("duplicate precompile event"));
}

#[test]
fn precompile_lowering_rejects_wrong_precompile_id() {
    let mut event = valid_event();
    event.precompile_id = 0x00ff;
    let err = lower_with_events(vec![event]).expect_err("wrong precompile id must fail");
    assert!(err.to_string().contains("does not match instruction id"));
}

#[test]
fn precompile_lowering_rejects_wrong_inputs() {
    let mut event = valid_event();
    event.inputs = vec![Value::U64(7)];
    let err = lower_with_events(vec![event]).expect_err("wrong inputs must fail");
    assert!(err.to_string().contains("do not match stored event"));
}

#[test]
fn precompile_lowering_rejects_wrong_output_arity() {
    let mut event = valid_event();
    event.outputs.push(Value::U64(2));
    let err = lower_with_events(vec![event]).expect_err("wrong output arity must fail");
    assert!(
        err.to_string()
            .contains("reports 2 outputs but IR declares 1")
    );
}

#[test]
fn precompile_lowering_rejects_extra_unmatched_event() {
    let mut extra = valid_event();
    extra.instruction_index = 1;
    let err = lower_with_events(vec![valid_event(), extra]).expect_err("extra event must fail");
    assert!(
        err.to_string().contains("duplicate precompile event")
            || err.to_string().contains("unmatched precompile events")
            || err.to_string().contains("missing precompile event")
    );
}
