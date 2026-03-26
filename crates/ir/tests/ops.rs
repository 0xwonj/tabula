mod common;

use common::{base_program, u64_literal};
use tabula_core::PortableValue;
use tabula_ir::validate_program;
use tabula_ir::*;
use tabula_profile::{TYPE_BOOL_ID, TYPE_U64_ID};

#[test]
fn rejects_query_write() {
    let program = base_program(Entry {
        id: EntryId(0),
        symbol: "bad".into(),
        kind: EntryKind::Query,
        params: vec![ParamDecl {
            id: ParamId(0),
            symbol: "owner".into(),
            ty: TYPE_U64_ID,
        }],
        returns: vec![TYPE_U64_ID],
        return_policy: ReturnPolicy::Explicit,
        body: Body {
            locals: vec![],
            ops: vec![
                Op::WriteState {
                    guard: None,
                    table: TableId(1),
                    key: ValueTupleRef(vec![ValueRef::Param(ParamId(0))]),
                    field: FieldId(0),
                    value: ValueRef::Literal(PortableValue::new(
                        TYPE_U64_ID,
                        1u64.to_le_bytes().to_vec(),
                    )),
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
fn rejects_non_bool_guard_local() {
    let program = base_program(Entry {
        id: EntryId(0),
        symbol: "bad_guard".into(),
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
            ],
            ops: vec![
                Op::Arith {
                    dst: LocalId(1),
                    op: ArithOp::Add,
                    lhs: u64_literal(1),
                    rhs: u64_literal(2),
                },
                Op::ReadState {
                    guard: Some(GuardRef(LocalId(1))),
                    dst_value: LocalId(0),
                    dst_present: LocalId(2),
                    table: TableId(1),
                    key: ValueTupleRef(vec![u64_literal(0)]),
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
