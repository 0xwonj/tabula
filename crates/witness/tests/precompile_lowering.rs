#![allow(missing_docs)]

use std::collections::{BTreeMap, BTreeSet};

use tabula_core::{InMemoryStaticTables, PrecompileEvent, Transaction};
use tabula_testing::exec::{compiled_program_from_artifact, core_batch_from_artifact_batch};
use tabula_testing::fixtures::artifacts::precompile_requirement_artifact;
use tabula_testing::fixtures::batch::single_tx_batch;
use tabula_types::{EncodingRuntimeRegistry, TypeRuntimeRegistry, TypedValue, u64_portable};
use tabula_witness::stark::{
    LowerSuccessfulTxInput, LoweringOutput, LoweringPrecompileCall, lower_successful_tx,
};

fn valid_event() -> PrecompileEvent {
    PrecompileEvent {
        tx_index: 0,
        instruction_index: 0,
        precompile_id: 0x0001,
        inputs: vec![],
        outputs: vec![u64_portable(1)],
    }
}

fn lower_with_events(
    events: &[PrecompileEvent],
) -> Result<LoweringOutput, tabula_core::error::TabulaError> {
    let sealed = compiled_program_from_artifact(&precompile_requirement_artifact());
    let batch = core_batch_from_artifact_batch(&single_tx_batch(1, vec![])).expect("core batch");
    let type_runtimes = TypeRuntimeRegistry::seeded().expect("seeded type runtimes");
    let encoding_runtimes = EncodingRuntimeRegistry::seeded().expect("seeded encoding runtimes");
    let tx: &Transaction = &batch.transactions[0];
    let tx_def = sealed.program().resolve(tx.tx_type)?;
    let precompile_calls = events
        .iter()
        .map(|event| typed_precompile_call(event, &type_runtimes))
        .collect::<Result<Vec<_>, _>>()?;
    let tx_lowering = lower_successful_tx::<3>(LowerSuccessfulTxInput {
        tx_index: 0,
        tx,
        tx_def,
        profile_map: &BTreeMap::new(),
        type_runtimes: &type_runtimes,
        encoding_runtimes: &encoding_runtimes,
        static_tables: &InMemoryStaticTables::new(),
        empty_columns: &BTreeSet::new(),
        precompile_signatures: sealed.program().precompiles(),
        access_trace: &[],
        precompile_calls: &precompile_calls,
        property_reads: &[],
    })?;
    Ok(LoweringOutput {
        instruction_records: tx_lowering.instruction_records,
        static_table_rows: tx_lowering.static_table_rows,
        ir_hash_calls: tx_lowering.ir_hash_calls,
    })
}

fn typed_precompile_call(
    event: &PrecompileEvent,
    type_runtimes: &TypeRuntimeRegistry,
) -> Result<LoweringPrecompileCall, tabula_core::error::TabulaError> {
    Ok(LoweringPrecompileCall {
        instruction_index: event.instruction_index,
        precompile_id: tabula_ir::PrecompileId(event.precompile_id),
        inputs: event
            .inputs
            .iter()
            .map(|value| type_runtimes.decode_portable(value))
            .collect::<Result<Vec<TypedValue>, _>>()?,
        outputs: event
            .outputs
            .iter()
            .map(|value| type_runtimes.decode_portable(value))
            .collect::<Result<Vec<TypedValue>, _>>()?,
    })
}

#[test]
fn precompile_lowering_accepts_matching_event() {
    let lowering = lower_with_events(&[valid_event()]).expect("lowering");
    assert_eq!(lowering.instruction_records.len(), 1);
}

#[test]
fn precompile_lowering_rejects_missing_event() {
    let err = lower_with_events(&[]).expect_err("missing event must fail");
    assert!(err.to_string().contains("missing precompile event"));
}

#[test]
fn precompile_lowering_rejects_duplicate_call_key() {
    let event = valid_event();
    let err = lower_with_events(&[event.clone(), event]).expect_err("duplicate event must fail");
    assert!(err.to_string().contains("duplicate precompile event"));
}

#[test]
fn precompile_lowering_rejects_wrong_precompile_id() {
    let mut event = valid_event();
    event.precompile_id = 0x00ff;
    let err = lower_with_events(&[event]).expect_err("wrong precompile id must fail");
    assert!(err.to_string().contains("does not match instruction id"));
}

#[test]
fn precompile_lowering_rejects_wrong_inputs() {
    let mut event = valid_event();
    event.inputs = vec![u64_portable(7)];
    let err = lower_with_events(&[event]).expect_err("wrong inputs must fail");
    assert!(err.to_string().contains("do not match stored event"));
}

#[test]
fn precompile_lowering_rejects_wrong_output_arity() {
    let mut event = valid_event();
    event.outputs.push(u64_portable(2));
    let err = lower_with_events(&[event]).expect_err("wrong output arity must fail");
    assert!(
        err.to_string()
            .contains("reports 2 outputs but IR declares 1")
    );
}

#[test]
fn precompile_lowering_rejects_extra_unmatched_event() {
    let mut extra = valid_event();
    extra.instruction_index = 1;
    let err = lower_with_events(&[valid_event(), extra]).expect_err("extra event must fail");
    assert!(
        err.to_string().contains("duplicate precompile event")
            || err.to_string().contains("unmatched precompile events")
            || err.to_string().contains("missing precompile event")
    );
}
