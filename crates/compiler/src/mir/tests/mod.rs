use std::collections::BTreeSet;

use tabula_core::PortableValue;
use tabula_core::testing::{Blake3Hasher, InMemoryState};
use tabula_executor as exec;
use tabula_ir as ir;
use tabula_profile::{TYPE_BOOL_ID, TYPE_BYTES32_ID, TYPE_U64_ID};
use tabula_runtime::semantics::RuntimeProgram;
use tabula_types::{TypeRuntimeRegistry, bool_typed, u64_typed};

use super::{
    Body, Callable, CallableId, CallableKind, LocalDecl, MatchArm, MatchPattern, Op, Program,
    Region, Terminator, ValueOp, analyze_program, canonicalize_program, inline_functions,
    lower_to_canonical, verify_program,
};

mod analysis;
mod lower;
mod transforms;
mod validate;

fn bool_lit(value: bool) -> PortableValue {
    PortableValue::new(TYPE_BOOL_ID, vec![u8::from(value)])
}

fn u64_lit(value: u64) -> PortableValue {
    PortableValue::new(TYPE_U64_ID, borsh::to_vec(&value).expect("u64 literal"))
}

fn base_program() -> Program {
    Program {
        program_id: ir::ProgramId(1),
        state: ir::StateSchema {
            tables: vec![ir::TableSchema {
                id: ir::TableId(1),
                symbol: "accounts".into(),
                key_tys: vec![TYPE_U64_ID],
                fields: vec![ir::FieldSchema {
                    id: ir::FieldId(0),
                    symbol: "balance".into(),
                    ty: TYPE_U64_ID,
                }],
            }],
        },
        context: ir::ContextSchema { fields: vec![] },
        const_pool: ir::ConstantPool { entries: vec![] },
        relation_manifest: ir::RelationManifest { entries: vec![] },
        capability_manifest: ir::CapabilityManifest {
            entries: vec![
                ir::CapabilityDescriptor {
                    id: ir::CapabilityId(1),
                    symbol: "checked_cap".into(),
                    inputs: vec![TYPE_U64_ID],
                    outputs: vec![TYPE_U64_ID],
                    totality: ir::CapabilityTotality::Checked,
                    query_policy: ir::CapabilityQueryPolicy::QuerySafe,
                    proof_visibility: ir::CapabilityProofVisibility::Journaled,
                },
                ir::CapabilityDescriptor {
                    id: ir::CapabilityId(2),
                    symbol: "total_cap".into(),
                    inputs: vec![TYPE_U64_ID],
                    outputs: vec![TYPE_U64_ID],
                    totality: ir::CapabilityTotality::Total,
                    query_policy: ir::CapabilityQueryPolicy::QuerySafe,
                    proof_visibility: ir::CapabilityProofVisibility::OpaqueRuntimeOnly,
                },
            ],
        },
        event_manifest: ir::EventManifest {
            entries: vec![ir::EventDescriptor {
                id: ir::EventId(1),
                symbol: "branch".into(),
                fields: vec![TYPE_BOOL_ID],
            }],
        },
        callables: vec![],
    }
}

fn region_contains_call_function(region: &Region) -> bool {
    region.ops.iter().any(|op| match op {
        Op::CallFunction { .. } => true,
        Op::If {
            then_region,
            else_region,
            ..
        } => {
            region_contains_call_function(then_region) || region_contains_call_function(else_region)
        }
        Op::Match { arms, default, .. } => {
            arms.iter()
                .any(|arm| region_contains_call_function(&arm.region))
                || default.as_ref().is_some_and(region_contains_call_function)
        }
        _ => false,
    })
}
