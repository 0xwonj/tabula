#![allow(missing_docs)]

mod common;

use common::{
    AddOneCapability, XorHasher, resolved_program, resolved_program_with_capability,
    test_state_runtime, type_runtimes,
};
use tabula_executor as exec;
use tabula_ir as ir;
use tabula_types::u64_typed;

#[test]
fn batch_tx_writes_state_records_relation_capability_and_event_effects() {
    let runtimes = type_runtimes();
    let mut capabilities = exec::CapabilityRegistry::new();
    capabilities.register(AddOneCapability).unwrap();
    let exec = exec::ExecContext {
        hasher: &XorHasher,
        type_runtimes: &runtimes,
        capability_executor: Some(&capabilities),
        state_runtime: test_state_runtime(),
    };
    let state = tabula_core::InMemoryState::new();
    let mut context = exec::ContextValues::new();
    context.insert(ir::ContextFieldId(0), u64_typed(7));

    let journal = exec::execute_batch(
        &resolved_program(),
        &[exec::TxCall {
            entry_id: ir::EntryId(1),
            params: vec![u64_typed(1), u64_typed(2), u64_typed(1)],
        }],
        &context,
        &state,
        &exec,
    )
    .expect("batch succeeds");

    let tx = journal.successful_txs().next().expect("successful tx");
    assert_eq!(tx.relation_effects.len(), 2);
    assert_eq!(tx.capability_effects.len(), 1);
    assert_eq!(tx.event_effects.len(), 1);
    assert_eq!(journal.state_summary.write_set_final.len(), 1);
}

#[test]
fn batch_keeps_failed_txs_separate_from_success_effects() {
    let runtimes = type_runtimes();
    let mut capabilities = exec::CapabilityRegistry::new();
    capabilities.register(AddOneCapability).unwrap();
    let exec = exec::ExecContext {
        hasher: &XorHasher,
        type_runtimes: &runtimes,
        capability_executor: Some(&capabilities),
        state_runtime: test_state_runtime(),
    };
    let state = tabula_core::InMemoryState::new();
    let mut context = exec::ContextValues::new();
    context.insert(ir::ContextFieldId(0), u64_typed(7));

    let journal = exec::execute_batch(
        &resolved_program(),
        &[
            exec::TxCall {
                entry_id: ir::EntryId(999),
                params: vec![],
            },
            exec::TxCall {
                entry_id: ir::EntryId(1),
                params: vec![u64_typed(1), u64_typed(2), u64_typed(1)],
            },
        ],
        &context,
        &state,
        &exec,
    )
    .expect("batch completes");

    assert_eq!(journal.txs.len(), 2);
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
fn opaque_capability_effects_stay_in_execution_journal() {
    let runtimes = type_runtimes();
    let mut capabilities = exec::CapabilityRegistry::new();
    capabilities.register(AddOneCapability).unwrap();
    let exec = exec::ExecContext {
        hasher: &XorHasher,
        type_runtimes: &runtimes,
        capability_executor: Some(&capabilities),
        state_runtime: test_state_runtime(),
    };
    let state = tabula_core::InMemoryState::new();
    let mut context = exec::ContextValues::new();
    context.insert(ir::ContextFieldId(0), u64_typed(7));

    let journal = exec::execute_batch(
        &resolved_program_with_capability(
            ir::CapabilityTotality::Total,
            ir::CapabilityProofVisibility::OpaqueRuntimeOnly,
        ),
        &[exec::TxCall {
            entry_id: ir::EntryId(1),
            params: vec![u64_typed(1), u64_typed(2), u64_typed(1)],
        }],
        &context,
        &state,
        &exec,
    )
    .expect("batch succeeds");

    let tx = journal.successful_txs().next().expect("successful tx");
    assert_eq!(tx.capability_effects.len(), 1);
}
