//! Tests for IR relation reference validation.

mod common;

use common::{base_program, u64_literal};
use tabula_ir::validate_program;
use tabula_ir::*;
use tabula_profile::TYPE_U64_ID;

#[test]
fn rejects_unknown_relation_reference() {
    let program = base_program(Entry {
        id: EntryId(0),
        symbol: "bad_relation".into(),
        kind: EntryKind::Query,
        params: vec![],
        returns: vec![TYPE_U64_ID],
        return_policy: ReturnPolicy::Explicit,
        body: Body {
            locals: vec![LocalDecl {
                id: LocalId(0),
                ty: TYPE_U64_ID,
            }],
            ops: vec![
                Op::AssertRelation {
                    guard: None,
                    relation: RelationId(99),
                    args: ValueTupleRef(vec![u64_literal(1)]),
                },
                Op::Return {
                    values: ValueTupleRef(vec![u64_literal(0)]),
                },
            ],
        },
    });
    assert!(validate_program(&program).is_err());
}
