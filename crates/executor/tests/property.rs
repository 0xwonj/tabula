#![allow(missing_docs)]

mod common;

use common::{TestPropertyReads, XorHasher, property_program, type_runtimes};
use tabula_executor as exec;
use tabula_ir as ir;
use tabula_profile::{TYPE_BYTES32_ID, TYPE_U64_ID};
use tabula_types::{bool_typed, u64_typed};

#[test]
fn property_read_minimum_records_effect_and_returns_row_tuple() {
    let runtimes = type_runtimes();
    let committed = TestPropertyReads::default().with_u64_column(
        ir::TableId(1),
        ir::FieldId(0),
        &[(10, 100, false), (5, 50, false), (20, 200, false)],
    );
    let exec = exec::ExecContext {
        hasher: &XorHasher,
        type_runtimes: &runtimes,
        capability_executor: None,
        property_reads: Some(&committed),
    };
    let state = tabula_core::InMemoryState::new();
    let context = exec::ContextValues::new();
    let program = property_program(ir::StatePropertyQuery::Minimum, TYPE_U64_ID);

    let result = exec::execute_query(&program, ir::EntryId(0), &[], &context, &state, &exec)
        .expect("property read succeeds");

    assert_eq!(
        result.returns,
        vec![u64_typed(50), u64_typed(5), bool_typed(false)]
    );
    assert_eq!(result.property_effects.len(), 1);
    assert_eq!(result.property_effects[0].outputs, result.returns);
}

#[test]
fn property_read_maximum_returns_greatest_row_key() {
    let runtimes = type_runtimes();
    let committed = TestPropertyReads::default().with_u64_column(
        ir::TableId(1),
        ir::FieldId(0),
        &[(10, 100, false), (5, 50, false), (20, 200, false)],
    );
    let exec = exec::ExecContext {
        hasher: &XorHasher,
        type_runtimes: &runtimes,
        capability_executor: None,
        property_reads: Some(&committed),
    };
    let state = tabula_core::InMemoryState::new();
    let context = exec::ContextValues::new();
    let program = property_program(ir::StatePropertyQuery::Maximum, TYPE_U64_ID);

    let result = exec::execute_query(&program, ir::EntryId(0), &[], &context, &state, &exec)
        .expect("property read succeeds");

    assert_eq!(
        result.returns,
        vec![u64_typed(200), u64_typed(20), bool_typed(false)]
    );
}

#[test]
fn property_read_successor_and_predecessor_are_structural() {
    let runtimes = type_runtimes();
    let committed = TestPropertyReads::default().with_u64_column(
        ir::TableId(1),
        ir::FieldId(0),
        &[(10, 100, false), (5, 50, false), (20, 200, false)],
    );
    let exec = exec::ExecContext {
        hasher: &XorHasher,
        type_runtimes: &runtimes,
        capability_executor: None,
        property_reads: Some(&committed),
    };
    let state = tabula_core::InMemoryState::new();
    let context = exec::ContextValues::new();
    let successor = property_program(
        ir::StatePropertyQuery::Successor {
            key: ir::ValueTupleRef(vec![ir::ValueRef::Literal(common::portable_u64(10))]),
        },
        TYPE_U64_ID,
    );
    let predecessor = property_program(
        ir::StatePropertyQuery::Predecessor {
            key: ir::ValueTupleRef(vec![ir::ValueRef::Literal(common::portable_u64(10))]),
        },
        TYPE_U64_ID,
    );

    let successor_result =
        exec::execute_query(&successor, ir::EntryId(0), &[], &context, &state, &exec)
            .expect("successor succeeds");
    let predecessor_result =
        exec::execute_query(&predecessor, ir::EntryId(0), &[], &context, &state, &exec)
            .expect("predecessor succeeds");

    assert_eq!(
        successor_result.returns,
        vec![u64_typed(200), u64_typed(20), bool_typed(false)]
    );
    assert_eq!(
        predecessor_result.returns,
        vec![u64_typed(50), u64_typed(5), bool_typed(false)]
    );
}

#[test]
fn property_read_no_match_returns_defaults_and_true_null_flag() {
    let runtimes = type_runtimes();
    let committed = TestPropertyReads::default().with_u64_column(
        ir::TableId(1),
        ir::FieldId(0),
        &[(10, 100, false)],
    );
    let exec = exec::ExecContext {
        hasher: &XorHasher,
        type_runtimes: &runtimes,
        capability_executor: None,
        property_reads: Some(&committed),
    };
    let state = tabula_core::InMemoryState::new();
    let context = exec::ContextValues::new();
    let program = property_program(
        ir::StatePropertyQuery::Successor {
            key: ir::ValueTupleRef(vec![ir::ValueRef::Literal(common::portable_u64(10))]),
        },
        TYPE_U64_ID,
    );

    let result = exec::execute_query(&program, ir::EntryId(0), &[], &context, &state, &exec)
        .expect("property read succeeds");

    assert_eq!(
        result.returns,
        vec![u64_typed(0), u64_typed(0), bool_typed(true)]
    );
}

#[test]
fn property_read_aggregate_is_unsupported_in_v1_adapter() {
    let runtimes = type_runtimes();
    let committed = TestPropertyReads::default();
    let exec = exec::ExecContext {
        hasher: &XorHasher,
        type_runtimes: &runtimes,
        capability_executor: None,
        property_reads: Some(&committed),
    };
    let state = tabula_core::InMemoryState::new();
    let context = exec::ContextValues::new();
    let program = property_program(
        ir::StatePropertyQuery::Aggregate {
            kind: ir::AggregateKind::Sum,
        },
        TYPE_U64_ID,
    );

    let error = exec::execute_query(&program, ir::EntryId(0), &[], &context, &state, &exec)
        .expect_err("aggregate should be unsupported");
    assert!(
        error
            .error
            .to_string()
            .contains("Aggregate is not yet supported in V1 adapter")
    );
}

#[test]
fn property_read_non_existence_range_is_unsupported_in_v1_adapter() {
    let runtimes = type_runtimes();
    let committed = TestPropertyReads::default();
    let exec = exec::ExecContext {
        hasher: &XorHasher,
        type_runtimes: &runtimes,
        capability_executor: None,
        property_reads: Some(&committed),
    };
    let state = tabula_core::InMemoryState::new();
    let context = exec::ContextValues::new();
    let program = property_program(
        ir::StatePropertyQuery::NonExistenceRange {
            lower: ir::ValueTupleRef(vec![ir::ValueRef::Literal(common::portable_u64(1))]),
            upper: ir::ValueTupleRef(vec![ir::ValueRef::Literal(common::portable_u64(2))]),
        },
        TYPE_U64_ID,
    );

    let error = exec::execute_query(&program, ir::EntryId(0), &[], &context, &state, &exec)
        .expect_err("non-existence range should be unsupported");
    assert!(
        error
            .error
            .to_string()
            .contains("NonExistenceRange is not yet supported in V1 adapter")
    );
}

#[test]
fn property_read_rejects_non_u64_key_schema_in_v1_executor() {
    let runtimes = type_runtimes();
    let committed = TestPropertyReads::default();
    let exec = exec::ExecContext {
        hasher: &XorHasher,
        type_runtimes: &runtimes,
        capability_executor: None,
        property_reads: Some(&committed),
    };
    let state = tabula_core::InMemoryState::new();
    let context = exec::ContextValues::new();
    let program = property_program(ir::StatePropertyQuery::Minimum, TYPE_BYTES32_ID);

    let error = exec::execute_query(&program, ir::EntryId(0), &[], &context, &state, &exec)
        .expect_err("non-u64 key schema should fail in V1");
    assert!(
        error
            .error
            .to_string()
            .contains("only supports [u64] key schema")
    );
}
