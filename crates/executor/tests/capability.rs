#![allow(missing_docs)]

mod common;

use common::{
    FailOnInputCapability, WrongArityCapability, WrongTypeCapability, XorHasher, resolved_program,
    resolved_program_with_capability, type_runtimes,
};
use tabula_executor as exec;
use tabula_ir as ir;
use tabula_types::u64_typed;

#[test]
fn checked_capability_failure_rolls_back_only_one_tx() {
    let runtimes = type_runtimes();
    let mut capabilities = exec::CapabilityRegistry::new();
    capabilities
        .register(FailOnInputCapability { fail_on: 10 })
        .unwrap();
    let exec = exec::ExecContext {
        hasher: &XorHasher,
        type_runtimes: &runtimes,
        capability_executor: Some(&capabilities),
        property_reads: None,
    };
    let state = tabula_core::InMemoryState::new();
    let mut context = exec::ContextValues::new();
    context.insert(ir::ContextFieldId(0), u64_typed(7));

    let journal = exec::execute_batch(
        &resolved_program_with_capability(
            ir::CapabilityTotality::Checked,
            ir::CapabilityProofVisibility::Journaled,
        ),
        &[
            exec::TxCall {
                entry_id: ir::EntryId(1),
                params: vec![u64_typed(1), u64_typed(2), u64_typed(1)],
            },
            exec::TxCall {
                entry_id: ir::EntryId(1),
                params: vec![u64_typed(3), u64_typed(4), u64_typed(2)],
            },
        ],
        &context,
        &state,
        &exec,
    )
    .expect("checked failure should not abort batch");

    assert!(matches!(
        journal.txs[0],
        exec::TxExecutionOutcome::Failed(_)
    ));
    assert!(matches!(
        journal.txs[1],
        exec::TxExecutionOutcome::Success(_)
    ));
    assert_eq!(journal.state_summary.write_set_final.len(), 1);
}

#[test]
fn total_capability_failure_aborts_batch() {
    let runtimes = type_runtimes();
    let mut capabilities = exec::CapabilityRegistry::new();
    capabilities
        .register(FailOnInputCapability { fail_on: 10 })
        .unwrap();
    let exec = exec::ExecContext {
        hasher: &XorHasher,
        type_runtimes: &runtimes,
        capability_executor: Some(&capabilities),
        property_reads: None,
    };
    let state = tabula_core::InMemoryState::new();
    let mut context = exec::ContextValues::new();
    context.insert(ir::ContextFieldId(0), u64_typed(7));

    let error = exec::execute_batch(
        &resolved_program_with_capability(
            ir::CapabilityTotality::Total,
            ir::CapabilityProofVisibility::Journaled,
        ),
        &[exec::TxCall {
            entry_id: ir::EntryId(1),
            params: vec![u64_typed(1), u64_typed(2), u64_typed(1)],
        }],
        &context,
        &state,
        &exec,
    )
    .expect_err("total capability failure should abort batch");

    assert!(error.to_string().contains("capability rejected input 10"));
}

#[test]
fn capability_output_arity_mismatch_is_fatal() {
    let runtimes = type_runtimes();
    let mut capabilities = exec::CapabilityRegistry::new();
    capabilities.register(WrongArityCapability).unwrap();
    let exec = exec::ExecContext {
        hasher: &XorHasher,
        type_runtimes: &runtimes,
        capability_executor: Some(&capabilities),
        property_reads: None,
    };
    let state = tabula_core::InMemoryState::new();
    let mut context = exec::ContextValues::new();
    context.insert(ir::ContextFieldId(0), u64_typed(7));

    let error = exec::execute_batch(
        &resolved_program(),
        &[exec::TxCall {
            entry_id: ir::EntryId(1),
            params: vec![u64_typed(1), u64_typed(2), u64_typed(1)],
        }],
        &context,
        &state,
        &exec,
    )
    .expect_err("wrong output arity should abort batch");

    assert!(error.to_string().contains("returned 0 values"));
}

#[test]
fn capability_output_type_mismatch_is_fatal() {
    let runtimes = type_runtimes();
    let mut capabilities = exec::CapabilityRegistry::new();
    capabilities.register(WrongTypeCapability).unwrap();
    let exec = exec::ExecContext {
        hasher: &XorHasher,
        type_runtimes: &runtimes,
        capability_executor: Some(&capabilities),
        property_reads: None,
    };
    let state = tabula_core::InMemoryState::new();
    let mut context = exec::ContextValues::new();
    context.insert(ir::ContextFieldId(0), u64_typed(7));

    let error = exec::execute_batch(
        &resolved_program(),
        &[exec::TxCall {
            entry_id: ir::EntryId(1),
            params: vec![u64_typed(1), u64_typed(2), u64_typed(1)],
        }],
        &context,
        &state,
        &exec,
    )
    .expect_err("wrong output type should abort batch");

    assert!(error.to_string().contains("returned wrong output type"));
}
