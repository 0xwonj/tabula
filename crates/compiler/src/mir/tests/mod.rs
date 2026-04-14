use std::cmp::Ordering;
use std::collections::BTreeSet;

use tabula_core::error::TabulaError;
use tabula_core::testing::{Blake3Hasher, InMemoryState};
use tabula_core::{CommittedCellKey, CommittedKey, CommittedPropertyQuery, TableId};
use tabula_core::{KeyComponentSchema, PortableValue};
use tabula_executor as exec;
use tabula_ir as ir;
use tabula_profile::{TYPE_BOOL_ID, TYPE_BYTES32_ID, TYPE_U64_ID};
use tabula_runtime::semantics::RuntimeProgram;
use tabula_types::{
    CommittedColumnEntry, NativeKeyPayload, TypeRuntimeRegistry, TypedCommittedPropertyQueryResult,
    TypedValue, bool_typed, encode_structural_u64, u64_typed,
};

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
                keys: vec![KeyComponentSchema {
                    symbol: "id".into(),
                    ty: TYPE_U64_ID,
                }],
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

struct IrStateRuntime<'a> {
    program: &'a ir::Program,
}

impl<'a> IrStateRuntime<'a> {
    fn table(&self, table: ir::TableId) -> Result<&ir::TableSchema, TabulaError> {
        self.program
            .state
            .tables
            .iter()
            .find(|schema| schema.id == table)
            .ok_or_else(|| TabulaError::InvalidIr(format!("unknown state table {}", table.0)))
    }
}

impl exec::StateRuntimeView for IrStateRuntime<'_> {
    fn encode_cell_key(
        &self,
        table: ir::TableId,
        field: ir::FieldId,
        key: &[TypedValue],
    ) -> Result<CommittedCellKey, TabulaError> {
        Ok(CommittedCellKey {
            table: TableId(table.0),
            col: field.into(),
            key: self.encode_committed_key(table, key)?,
        })
    }

    fn encode_committed_key(
        &self,
        table: ir::TableId,
        key: &[TypedValue],
    ) -> Result<CommittedKey, TabulaError> {
        let key_types = self.key_component_types(table)?;
        let [value] = key else {
            return Err(TabulaError::InvalidIr(
                "compiler MIR tests expect single-component keys".into(),
            ));
        };
        if key_types != vec![TYPE_U64_ID] || value.type_id() != TYPE_U64_ID {
            return Err(TabulaError::InvalidIr(format!(
                "compiler MIR tests only support [u64] state keys, table {} declared {:?}",
                table.0, key_types
            )));
        }
        Ok(CommittedKey(value.payload().to_vec()))
    }

    fn decode_committed_key(
        &self,
        table: ir::TableId,
        key: &CommittedKey,
    ) -> Result<Vec<TypedValue>, TabulaError> {
        let key_types = self.key_component_types(table)?;
        if key_types != vec![TYPE_U64_ID] || key.0.len() != std::mem::size_of::<u64>() {
            return Err(TabulaError::InvalidIr(format!(
                "compiler MIR tests only support canonical [u64] keys for table {}",
                table.0
            )));
        }
        Ok(vec![u64_typed(u64::from_le_bytes(
            key.0.clone().try_into().expect("u64 key bytes"),
        ))])
    }

    fn encode_key_payload(
        &self,
        table: ir::TableId,
        key: &CommittedKey,
    ) -> Result<NativeKeyPayload, TabulaError> {
        let [value]: [TypedValue; 1] = self
            .decode_committed_key(table, key)?
            .try_into()
            .map_err(|_| TabulaError::InvalidIr("expected one key component".into()))?;
        let raw = u64::from_le_bytes(value.payload().try_into().expect("u64 payload"));
        encode_structural_u64::<{ tabula_types::NATIVE_KEY_PAYLOAD_WIDTH }>(raw)?
            .try_into()
            .map_err(|_| TabulaError::ProofError {
                phase: "compiler_mir_test_key_payload",
                detail: "failed to build fixed-width key payload".into(),
            })
    }

    fn compare_keys(
        &self,
        table: ir::TableId,
        lhs: &CommittedKey,
        rhs: &CommittedKey,
    ) -> Result<Ordering, TabulaError> {
        let [lhs]: [TypedValue; 1] = self
            .decode_committed_key(table, lhs)?
            .try_into()
            .map_err(|_| TabulaError::InvalidIr("expected one lhs key component".into()))?;
        let [rhs]: [TypedValue; 1] = self
            .decode_committed_key(table, rhs)?
            .try_into()
            .map_err(|_| TabulaError::InvalidIr("expected one rhs key component".into()))?;
        let lhs = u64::from_le_bytes(lhs.payload().try_into().expect("u64 payload"));
        let rhs = u64::from_le_bytes(rhs.payload().try_into().expect("u64 payload"));
        Ok(lhs.cmp(&rhs))
    }

    fn key_component_types(
        &self,
        table: ir::TableId,
    ) -> Result<Vec<tabula_core::TypeId>, TabulaError> {
        Ok(self.table(table)?.keys.iter().map(|key| key.ty).collect())
    }

    fn column_type(
        &self,
        table: ir::TableId,
        field: ir::FieldId,
    ) -> Result<tabula_core::TypeId, TabulaError> {
        self.table(table)?
            .fields
            .iter()
            .find(|schema| schema.id == field)
            .map(|schema| schema.ty)
            .ok_or_else(|| {
                TabulaError::InvalidIr(format!("unknown state field {}.{}", table.0, field.0))
            })
    }

    fn resolve_property(
        &self,
        _table: ir::TableId,
        _field: ir::FieldId,
        _query: &CommittedPropertyQuery,
        _state: &[CommittedColumnEntry],
    ) -> Result<TypedCommittedPropertyQueryResult, TabulaError> {
        Err(TabulaError::InvalidIr(
            "compiler MIR tests do not use property reads".into(),
        ))
    }
}
