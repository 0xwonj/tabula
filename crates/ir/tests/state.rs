//! Tests for IR state read/write validation.

mod common;

use common::{base_program, u64_literal};
use tabula_core::PortableValue;
use tabula_ir::validate_program;
use tabula_ir::*;
use tabula_profile::{TYPE_BOOL_ID, TYPE_U64_ID};

#[test]
fn accepts_minimal_query() {
    let program = base_program(Entry {
        id: EntryId(0),
        symbol: "balance".into(),
        kind: EntryKind::Query,
        params: vec![ParamDecl {
            id: ParamId(0),
            symbol: "owner".into(),
            ty: TYPE_U64_ID,
        }],
        returns: vec![TYPE_U64_ID],
        return_policy: ReturnPolicy::Explicit,
        body: Body {
            locals: vec![
                LocalDecl {
                    id: LocalId(0),
                    ty: TYPE_U64_ID,
                },
                LocalDecl {
                    id: LocalId(1),
                    ty: TYPE_BOOL_ID,
                },
            ],
            ops: vec![
                Op::ReadState {
                    guard: None,
                    dst_value: LocalId(0),
                    dst_present: LocalId(1),
                    table: TableId(1),
                    key: ValueTupleRef(vec![ValueRef::Param(ParamId(0))]),
                    field: FieldId(0),
                },
                Op::Return {
                    values: ValueTupleRef(vec![ValueRef::Local(LocalId(0))]),
                },
            ],
        },
    });
    validate_program(&program).unwrap();
}

#[test]
fn rejects_empty_key_schema() {
    let mut program = base_program(Entry {
        id: EntryId(0),
        symbol: "ok".into(),
        kind: EntryKind::Query,
        params: vec![],
        returns: vec![TYPE_U64_ID],
        return_policy: ReturnPolicy::Explicit,
        body: Body {
            locals: vec![],
            ops: vec![Op::Return {
                values: ValueTupleRef(vec![u64_literal(0)]),
            }],
        },
    });
    program.state.tables[0].keys.clear();
    assert!(validate_program(&program).is_err());
}

#[test]
fn rejects_state_key_arity_mismatch() {
    let program = base_program(Entry {
        id: EntryId(0),
        symbol: "bad_key_arity".into(),
        kind: EntryKind::Query,
        params: vec![],
        returns: vec![TYPE_U64_ID],
        return_policy: ReturnPolicy::Explicit,
        body: Body {
            locals: vec![
                LocalDecl {
                    id: LocalId(0),
                    ty: TYPE_U64_ID,
                },
                LocalDecl {
                    id: LocalId(1),
                    ty: TYPE_BOOL_ID,
                },
            ],
            ops: vec![
                Op::ReadState {
                    guard: None,
                    dst_value: LocalId(0),
                    dst_present: LocalId(1),
                    table: TableId(1),
                    key: ValueTupleRef(vec![u64_literal(1), u64_literal(2)]),
                    field: FieldId(0),
                },
                Op::Return {
                    values: ValueTupleRef(vec![ValueRef::Local(LocalId(0))]),
                },
            ],
        },
    });
    assert!(validate_program(&program).is_err());
}

#[test]
fn rejects_state_key_type_mismatch() {
    let program = base_program(Entry {
        id: EntryId(0),
        symbol: "bad_key_type".into(),
        kind: EntryKind::Query,
        params: vec![],
        returns: vec![TYPE_U64_ID],
        return_policy: ReturnPolicy::Explicit,
        body: Body {
            locals: vec![
                LocalDecl {
                    id: LocalId(0),
                    ty: TYPE_U64_ID,
                },
                LocalDecl {
                    id: LocalId(1),
                    ty: TYPE_BOOL_ID,
                },
            ],
            ops: vec![
                Op::ReadState {
                    guard: None,
                    dst_value: LocalId(0),
                    dst_present: LocalId(1),
                    table: TableId(1),
                    key: ValueTupleRef(vec![ValueRef::Literal(PortableValue::new(
                        TYPE_BOOL_ID,
                        vec![1],
                    ))]),
                    field: FieldId(0),
                },
                Op::Return {
                    values: ValueTupleRef(vec![ValueRef::Local(LocalId(0))]),
                },
            ],
        },
    });
    assert!(validate_program(&program).is_err());
}

#[test]
fn rejects_property_query_embedded_key_type_mismatch() {
    let program = base_program(Entry {
        id: EntryId(0),
        symbol: "bad_property_key".into(),
        kind: EntryKind::Query,
        params: vec![],
        returns: vec![TYPE_U64_ID],
        return_policy: ReturnPolicy::Explicit,
        body: Body {
            locals: vec![
                LocalDecl {
                    id: LocalId(0),
                    ty: TYPE_U64_ID,
                },
                LocalDecl {
                    id: LocalId(1),
                    ty: TYPE_U64_ID,
                },
                LocalDecl {
                    id: LocalId(2),
                    ty: TYPE_BOOL_ID,
                },
                LocalDecl {
                    id: LocalId(3),
                    ty: TYPE_U64_ID,
                },
            ],
            ops: vec![
                Op::ReadStateProperty {
                    guard: None,
                    dst_value: LocalId(0),
                    dst_key_components: vec![LocalId(1)],
                    dst_is_null: LocalId(2),
                    table: TableId(1),
                    field: FieldId(0),
                    query: StatePropertyQuery::Successor {
                        key: ValueTupleRef(vec![ValueRef::Literal(PortableValue::new(
                            TYPE_BOOL_ID,
                            vec![1],
                        ))]),
                    },
                },
                Op::Return {
                    values: ValueTupleRef(vec![ValueRef::Literal(PortableValue::new(
                        TYPE_U64_ID,
                        0u64.to_le_bytes().to_vec(),
                    ))]),
                },
            ],
        },
    });
    assert!(validate_program(&program).is_err());
}

#[test]
fn accepts_multi_component_key_schema_for_row_property_queries() {
    let mut program = base_program(Entry {
        id: EntryId(0),
        symbol: "multi_key_property".into(),
        kind: EntryKind::Query,
        params: vec![],
        returns: vec![TYPE_U64_ID, TYPE_U64_ID, TYPE_U64_ID, TYPE_BOOL_ID],
        return_policy: ReturnPolicy::Explicit,
        body: Body {
            locals: vec![
                LocalDecl {
                    id: LocalId(0),
                    ty: TYPE_U64_ID,
                },
                LocalDecl {
                    id: LocalId(1),
                    ty: TYPE_U64_ID,
                },
                LocalDecl {
                    id: LocalId(2),
                    ty: TYPE_U64_ID,
                },
                LocalDecl {
                    id: LocalId(3),
                    ty: TYPE_BOOL_ID,
                },
            ],
            ops: vec![
                Op::ReadStateProperty {
                    guard: None,
                    dst_value: LocalId(0),
                    dst_key_components: vec![LocalId(1), LocalId(2)],
                    dst_is_null: LocalId(3),
                    table: TableId(1),
                    field: FieldId(0),
                    query: StatePropertyQuery::Minimum,
                },
                Op::Return {
                    values: ValueTupleRef(vec![
                        ValueRef::Local(LocalId(0)),
                        ValueRef::Local(LocalId(1)),
                        ValueRef::Local(LocalId(2)),
                        ValueRef::Local(LocalId(3)),
                    ]),
                },
            ],
        },
    });
    program.state.tables[0].keys = vec![
        tabula_core::KeyComponentSchema {
            symbol: "a".into(),
            ty: TYPE_U64_ID,
        },
        tabula_core::KeyComponentSchema {
            symbol: "b".into(),
            ty: TYPE_U64_ID,
        },
    ];
    assert!(validate_program(&program).is_ok());
}
