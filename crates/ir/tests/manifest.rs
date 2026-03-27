//! Tests for IR capability manifest validation.

mod common;

use common::{base_program, u64_literal};
use tabula_ir::validate_program;
use tabula_ir::*;
use tabula_profile::TYPE_U64_ID;

#[test]
fn rejects_query_tx_only_capability() {
    let mut program = base_program(Entry {
        id: EntryId(0),
        symbol: "bad_capability".into(),
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
                Op::CallCapability {
                    guard: None,
                    capability: CapabilityId(7),
                    inputs: ValueTupleRef(vec![u64_literal(1)]),
                    dsts: vec![LocalId(0)],
                },
                Op::Return {
                    values: ValueTupleRef(vec![ValueRef::Local(LocalId(0))]),
                },
            ],
        },
    });
    program
        .capability_manifest
        .entries
        .push(CapabilityDescriptor {
            id: CapabilityId(7),
            symbol: "tx_only".into(),
            inputs: vec![TYPE_U64_ID],
            outputs: vec![TYPE_U64_ID],
            totality: CapabilityTotality::Total,
            query_policy: CapabilityQueryPolicy::TxOnly,
            proof_visibility: CapabilityProofVisibility::Journaled,
        });
    assert!(validate_program(&program).is_err());
}
