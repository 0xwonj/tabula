mod common;

use common::{base_program, u64_literal};
use tabula_ir::validate_program;
use tabula_ir::*;
use tabula_profile::{TYPE_BOOL_ID, TYPE_U64_ID};

#[test]
fn rejects_duplicate_entry_ids() {
    let entry = Entry {
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
    };
    let mut program = base_program(entry.clone());
    program.entries.push(entry);
    assert!(validate_program(&program).is_err());
}

#[test]
fn rejects_unknown_local_reference() {
    let program = base_program(Entry {
        id: EntryId(0),
        symbol: "bad_ref".into(),
        kind: EntryKind::Query,
        params: vec![],
        returns: vec![TYPE_U64_ID],
        return_policy: ReturnPolicy::Explicit,
        body: Body {
            locals: vec![],
            ops: vec![Op::Return {
                values: ValueTupleRef(vec![ValueRef::Local(LocalId(99))]),
            }],
        },
    });
    assert!(validate_program(&program).is_err());
}

#[test]
fn rejects_return_arity_mismatch() {
    let program = base_program(Entry {
        id: EntryId(0),
        symbol: "bad_return".into(),
        kind: EntryKind::Query,
        params: vec![],
        returns: vec![TYPE_U64_ID],
        return_policy: ReturnPolicy::Explicit,
        body: Body {
            locals: vec![],
            ops: vec![Op::Return {
                values: ValueTupleRef(vec![]),
            }],
        },
    });
    assert!(validate_program(&program).is_err());
}

#[test]
fn validated_program_rejects_invalid_raw_program() {
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
    program.state.tables[0].key_tys.clear();
    assert!(ValidatedProgram::try_from(program).is_err());
}
