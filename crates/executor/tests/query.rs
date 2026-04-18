#![allow(missing_docs)]

mod common;

use common::{
    FailOnInputCapability, XorHasher, capability_query_program, portable_u64, query_exec_context,
    resolved_program, test_state_runtime, type_runtimes,
};
use tabula_core::{ColId, CommittedCellKey, CommittedKey, TableId};
use tabula_executor as exec;
use tabula_ir as ir;
use tabula_profile::TYPE_BYTES32_ID;
use tabula_types::{ContextValues, u64_typed};

#[test]
fn query_reads_state_and_hashes_result() {
    let runtimes = type_runtimes();
    let exec = query_exec_context(&runtimes);
    let mut state = tabula_core::InMemoryState::new();
    state.set(
        CommittedCellKey {
            table: TableId(1),
            col: ColId(0),
            key: CommittedKey(9u64.to_le_bytes().to_vec()),
        },
        portable_u64(42),
    );
    let mut context = ContextValues::new();
    context.insert(ir::ContextFieldId(0), u64_typed(7));

    let result = exec::execute_query(
        &resolved_program(),
        ir::EntryId(0),
        &[u64_typed(9)],
        &context,
        &state,
        &exec,
    )
    .expect("query succeeds");

    assert_eq!(result.returns[0], u64_typed(42));
    assert_eq!(result.returns[1].type_id(), TYPE_BYTES32_ID);
    assert_eq!(result.state_effects.len(), 1);
    assert!(result.state_summary.write_set_final.is_empty());
}

#[test]
fn query_checked_capability_failure_surfaces_execute_error() {
    let runtimes = type_runtimes();
    let mut capabilities = exec::CapabilityRegistry::new();
    capabilities
        .register(FailOnInputCapability { fail_on: 0 })
        .unwrap();
    let exec = exec::ExecContext {
        hasher: &XorHasher,
        type_runtimes: &runtimes,
        capability_executor: Some(&capabilities),
        state_runtime: test_state_runtime(),
    };
    let state = tabula_core::InMemoryState::new();
    let context = ContextValues::new();

    let error = exec::execute_query(
        &capability_query_program(ir::CapabilityTotality::Checked),
        ir::EntryId(0),
        &[u64_typed(0)],
        &context,
        &state,
        &exec,
    )
    .expect_err("checked capability should fail query");

    assert!(
        error
            .error
            .to_string()
            .contains("capability rejected input 0")
    );
}

#[test]
fn query_total_capability_failure_still_surfaces_execute_error() {
    let runtimes = type_runtimes();
    let mut capabilities = exec::CapabilityRegistry::new();
    capabilities
        .register(FailOnInputCapability { fail_on: 0 })
        .unwrap();
    let exec = exec::ExecContext {
        hasher: &XorHasher,
        type_runtimes: &runtimes,
        capability_executor: Some(&capabilities),
        state_runtime: test_state_runtime(),
    };
    let state = tabula_core::InMemoryState::new();
    let context = ContextValues::new();

    let error = exec::execute_query(
        &capability_query_program(ir::CapabilityTotality::Total),
        ir::EntryId(0),
        &[u64_typed(0)],
        &context,
        &state,
        &exec,
    )
    .expect_err("total capability failure should fail query");

    assert!(
        error
            .error
            .to_string()
            .contains("capability rejected input 0")
    );
}
