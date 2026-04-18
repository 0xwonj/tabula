#![allow(dead_code, missing_docs)]

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::sync::OnceLock;

use borsh::to_vec;
use tabula_core::error::TabulaError;
use tabula_core::traits::Hasher;
use tabula_core::{
    ColId, CommittedCellKey, CommittedKey, CommittedPropertyQuery, KeyComponentSchema,
    PortableValue, TableId,
};
use tabula_executor as exec;
use tabula_ir as ir;
use tabula_profile::{TYPE_BOOL_ID, TYPE_BYTES32_ID, TYPE_U64_ID};
use tabula_types::{
    CommittedColumnEntry, NativeKeyPayload, StateRuntimeView, TypeRuntimeRegistry,
    TypedCommittedPropertyQueryResult, TypedValue, bool_typed, encode_structural_u64, u64_typed,
};

pub struct XorHasher;

impl Hasher for XorHasher {
    fn hash(&self, data: &[u8]) -> tabula_core::Digest {
        let mut out = [0u8; 32];
        for (index, byte) in data.iter().enumerate() {
            out[index % 32] ^= byte;
        }
        out
    }

    fn hash_pair(
        &self,
        left: &tabula_core::Digest,
        right: &tabula_core::Digest,
    ) -> Result<tabula_core::Digest, TabulaError> {
        let mut data = Vec::new();
        data.extend_from_slice(left);
        data.extend_from_slice(right);
        Ok(self.hash(&data))
    }
}

pub struct AddOneCapability;

impl exec::CapabilityHandler for AddOneCapability {
    fn id(&self) -> ir::CapabilityId {
        ir::CapabilityId(7)
    }

    fn execute(&self, inputs: &[TypedValue]) -> Result<Vec<TypedValue>, TabulaError> {
        let value = &inputs[0];
        let portable = PortableValue::new(
            value.type_id(),
            to_vec(&(borsh::from_slice::<u64>(value.payload()).unwrap() + 1)).unwrap(),
        );
        Ok(vec![
            TypeRuntimeRegistry::seeded()
                .unwrap()
                .decode_portable(&portable)
                .unwrap(),
        ])
    }
}

pub struct FailOnInputCapability {
    pub fail_on: u64,
}

impl exec::CapabilityHandler for FailOnInputCapability {
    fn id(&self) -> ir::CapabilityId {
        ir::CapabilityId(7)
    }

    fn execute(&self, inputs: &[TypedValue]) -> Result<Vec<TypedValue>, TabulaError> {
        let raw = borsh::from_slice::<u64>(inputs[0].payload())
            .map_err(|error| TabulaError::BorshEncodingError(error.to_string()))?;
        if raw == self.fail_on {
            return Err(TabulaError::AssertionFailed(format!(
                "capability rejected input {raw}"
            )));
        }
        Ok(vec![u64_typed(raw + 1)])
    }
}

pub struct WrongArityCapability;

impl exec::CapabilityHandler for WrongArityCapability {
    fn id(&self) -> ir::CapabilityId {
        ir::CapabilityId(7)
    }

    fn execute(&self, _inputs: &[TypedValue]) -> Result<Vec<TypedValue>, TabulaError> {
        Ok(vec![])
    }
}

pub struct WrongTypeCapability;

impl exec::CapabilityHandler for WrongTypeCapability {
    fn id(&self) -> ir::CapabilityId {
        ir::CapabilityId(7)
    }

    fn execute(&self, _inputs: &[TypedValue]) -> Result<Vec<TypedValue>, TabulaError> {
        Ok(vec![bool_typed(true)])
    }
}

pub fn type_runtimes() -> TypeRuntimeRegistry {
    TypeRuntimeRegistry::seeded().expect("seeded runtimes")
}

pub fn portable_u64(value: u64) -> PortableValue {
    PortableValue::new(TYPE_U64_ID, to_vec(&value).unwrap())
}

pub fn raw_program() -> ir::Program {
    ir::Program {
        program_id: ir::ProgramId(0),
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
        context: ir::ContextSchema {
            fields: vec![ir::ContextField {
                id: ir::ContextFieldId(0),
                symbol: "epoch".into(),
                ty: TYPE_U64_ID,
            }],
        },
        const_pool: ir::ConstantPool {
            entries: vec![ir::ConstantEntry {
                id: ir::ConstId(0),
                ty: TYPE_U64_ID,
                value: portable_u64(5),
            }],
        },
        relation_manifest: ir::RelationManifest {
            entries: vec![
                ir::RelationManifestEntry {
                    id: ir::RelationId(0),
                    descriptor: ir::RelationDescriptor {
                        symbol: "AllowedTier".into(),
                        inputs: vec![TYPE_U64_ID],
                        outputs: vec![],
                    },
                    binding: ir::RelationBinding::EnumSet {
                        values: vec![portable_u64(1), portable_u64(2)],
                    },
                },
                ir::RelationManifestEntry {
                    id: ir::RelationId(1),
                    descriptor: ir::RelationDescriptor {
                        symbol: "FeeForTier".into(),
                        inputs: vec![TYPE_U64_ID],
                        outputs: vec![TYPE_U64_ID],
                    },
                    binding: ir::RelationBinding::Map {
                        rows: vec![
                            ir::RelationRow {
                                inputs: vec![portable_u64(1)],
                                outputs: vec![portable_u64(10)],
                            },
                            ir::RelationRow {
                                inputs: vec![portable_u64(2)],
                                outputs: vec![portable_u64(20)],
                            },
                        ],
                    },
                },
            ],
        },
        capability_manifest: ir::CapabilityManifest {
            entries: vec![ir::CapabilityDescriptor {
                id: ir::CapabilityId(7),
                symbol: "add_one".into(),
                inputs: vec![TYPE_U64_ID],
                outputs: vec![TYPE_U64_ID],
                totality: ir::CapabilityTotality::Total,
                query_policy: ir::CapabilityQueryPolicy::QuerySafe,
                proof_visibility: ir::CapabilityProofVisibility::Journaled,
            }],
        },
        event_manifest: ir::EventManifest {
            entries: vec![ir::EventDescriptor {
                id: ir::EventId(0),
                symbol: "Transfer".into(),
                fields: vec![TYPE_U64_ID, TYPE_U64_ID],
            }],
        },
        entries: vec![
            ir::Entry {
                id: ir::EntryId(0),
                symbol: "balance_of".into(),
                kind: ir::EntryKind::Query,
                params: vec![ir::ParamDecl {
                    id: ir::ParamId(0),
                    symbol: "owner".into(),
                    ty: TYPE_U64_ID,
                }],
                returns: vec![TYPE_U64_ID, TYPE_BYTES32_ID],
                return_policy: ir::ReturnPolicy::Explicit,
                body: ir::Body {
                    locals: vec![
                        ir::LocalDecl {
                            id: ir::LocalId(0),
                            ty: TYPE_U64_ID,
                        },
                        ir::LocalDecl {
                            id: ir::LocalId(1),
                            ty: TYPE_BOOL_ID,
                        },
                        ir::LocalDecl {
                            id: ir::LocalId(2),
                            ty: TYPE_BYTES32_ID,
                        },
                    ],
                    ops: vec![
                        ir::Op::ReadState {
                            guard: None,
                            dst_value: ir::LocalId(0),
                            dst_present: ir::LocalId(1),
                            table: ir::TableId(1),
                            key: ir::ValueTupleRef(vec![ir::ValueRef::Param(ir::ParamId(0))]),
                            field: ir::FieldId(0),
                        },
                        ir::Op::Hash {
                            dst: ir::LocalId(2),
                            family: ir::HashFamily::Poseidon,
                            inputs: ir::ValueTupleRef(vec![ir::ValueRef::Local(ir::LocalId(0))]),
                        },
                        ir::Op::Return {
                            values: ir::ValueTupleRef(vec![
                                ir::ValueRef::Local(ir::LocalId(0)),
                                ir::ValueRef::Local(ir::LocalId(2)),
                            ]),
                        },
                    ],
                },
            },
            ir::Entry {
                id: ir::EntryId(1),
                symbol: "transfer".into(),
                kind: ir::EntryKind::Tx,
                params: vec![
                    ir::ParamDecl {
                        id: ir::ParamId(0),
                        symbol: "from".into(),
                        ty: TYPE_U64_ID,
                    },
                    ir::ParamDecl {
                        id: ir::ParamId(1),
                        symbol: "to".into(),
                        ty: TYPE_U64_ID,
                    },
                    ir::ParamDecl {
                        id: ir::ParamId(2),
                        symbol: "tier".into(),
                        ty: TYPE_U64_ID,
                    },
                ],
                returns: vec![],
                return_policy: ir::ReturnPolicy::Unit,
                body: ir::Body {
                    locals: vec![
                        ir::LocalDecl {
                            id: ir::LocalId(0),
                            ty: TYPE_U64_ID,
                        },
                        ir::LocalDecl {
                            id: ir::LocalId(1),
                            ty: TYPE_BOOL_ID,
                        },
                        ir::LocalDecl {
                            id: ir::LocalId(2),
                            ty: TYPE_U64_ID,
                        },
                    ],
                    ops: vec![
                        ir::Op::AssertRelation {
                            guard: None,
                            relation: ir::RelationId(0),
                            args: ir::ValueTupleRef(vec![ir::ValueRef::Param(ir::ParamId(2))]),
                        },
                        ir::Op::EvalRelation {
                            guard: None,
                            relation: ir::RelationId(1),
                            inputs: ir::ValueTupleRef(vec![ir::ValueRef::Param(ir::ParamId(2))]),
                            dsts: vec![ir::LocalId(0)],
                        },
                        ir::Op::CallCapability {
                            guard: None,
                            capability: ir::CapabilityId(7),
                            inputs: ir::ValueTupleRef(vec![ir::ValueRef::Local(ir::LocalId(0))]),
                            dsts: vec![ir::LocalId(2)],
                        },
                        ir::Op::WriteState {
                            guard: None,
                            table: ir::TableId(1),
                            key: ir::ValueTupleRef(vec![ir::ValueRef::Param(ir::ParamId(1))]),
                            field: ir::FieldId(0),
                            value: ir::ValueRef::Local(ir::LocalId(2)),
                        },
                        ir::Op::EmitEvent {
                            guard: None,
                            event: ir::EventId(0),
                            args: ir::ValueTupleRef(vec![
                                ir::ValueRef::Param(ir::ParamId(0)),
                                ir::ValueRef::Local(ir::LocalId(2)),
                            ]),
                        },
                        ir::Op::Return {
                            values: ir::ValueTupleRef(vec![]),
                        },
                    ],
                },
            },
        ],
    }
}

pub fn validated_program() -> ir::ValidatedProgram {
    ir::ValidatedProgram::try_from(raw_program()).expect("valid canonical program")
}

pub fn resolved_program() -> exec::ResolvedExecutionProgram {
    exec::ResolvedExecutionProgram::from_validated_program(validated_program())
        .expect("resolved execution program")
}

pub fn resolved_program_with_capability(
    totality: ir::CapabilityTotality,
    proof_visibility: ir::CapabilityProofVisibility,
) -> exec::ResolvedExecutionProgram {
    let mut raw = raw_program();
    raw.capability_manifest.entries[0].totality = totality;
    raw.capability_manifest.entries[0].proof_visibility = proof_visibility;
    exec::ResolvedExecutionProgram::from_validated_program(
        ir::ValidatedProgram::try_from(raw).expect("valid modified program"),
    )
    .expect("resolved execution program")
}

pub fn capability_query_program(
    totality: ir::CapabilityTotality,
) -> exec::ResolvedExecutionProgram {
    exec::ResolvedExecutionProgram::from_validated_program(
        ir::ValidatedProgram::try_from(ir::Program {
            program_id: ir::ProgramId(2),
            state: ir::StateSchema { tables: vec![] },
            context: ir::ContextSchema { fields: vec![] },
            const_pool: ir::ConstantPool { entries: vec![] },
            relation_manifest: ir::RelationManifest { entries: vec![] },
            capability_manifest: ir::CapabilityManifest {
                entries: vec![ir::CapabilityDescriptor {
                    id: ir::CapabilityId(7),
                    symbol: "maybe_fail".into(),
                    inputs: vec![TYPE_U64_ID],
                    outputs: vec![TYPE_U64_ID],
                    totality,
                    query_policy: ir::CapabilityQueryPolicy::QuerySafe,
                    proof_visibility: ir::CapabilityProofVisibility::Journaled,
                }],
            },
            event_manifest: ir::EventManifest { entries: vec![] },
            entries: vec![ir::Entry {
                id: ir::EntryId(0),
                symbol: "check".into(),
                kind: ir::EntryKind::Query,
                params: vec![ir::ParamDecl {
                    id: ir::ParamId(0),
                    symbol: "value".into(),
                    ty: TYPE_U64_ID,
                }],
                returns: vec![TYPE_U64_ID],
                return_policy: ir::ReturnPolicy::Explicit,
                body: ir::Body {
                    locals: vec![ir::LocalDecl {
                        id: ir::LocalId(0),
                        ty: TYPE_U64_ID,
                    }],
                    ops: vec![
                        ir::Op::CallCapability {
                            guard: None,
                            capability: ir::CapabilityId(7),
                            inputs: ir::ValueTupleRef(vec![ir::ValueRef::Param(ir::ParamId(0))]),
                            dsts: vec![ir::LocalId(0)],
                        },
                        ir::Op::Return {
                            values: ir::ValueTupleRef(vec![ir::ValueRef::Local(ir::LocalId(0))]),
                        },
                    ],
                },
            }],
        })
        .expect("valid capability query program"),
    )
    .expect("resolved capability query program")
}

#[derive(Default)]
pub struct TestStateRuntime {
    table_key_types: BTreeMap<ir::TableId, Vec<tabula_core::TypeId>>,
    column_types: BTreeMap<(ir::TableId, ir::FieldId), tabula_core::TypeId>,
}

impl TestStateRuntime {
    pub fn with_u64_column(mut self, table: ir::TableId, field: ir::FieldId) -> Self {
        self.table_key_types.insert(table, vec![TYPE_U64_ID]);
        self.column_types.insert((table, field), TYPE_U64_ID);
        self
    }
}

impl StateRuntimeView for TestStateRuntime {
    fn encode_cell_key(
        &self,
        table: ir::TableId,
        field: ir::FieldId,
        key: &[TypedValue],
    ) -> Result<CommittedCellKey, TabulaError> {
        Ok(CommittedCellKey {
            table: TableId(table.0),
            col: ColId(field.0),
            key: self.encode_committed_key(table, key)?,
        })
    }

    fn encode_committed_key(
        &self,
        table: ir::TableId,
        key: &[TypedValue],
    ) -> Result<CommittedKey, TabulaError> {
        let key_types = self.key_component_types(table)?;
        if key_types != vec![TYPE_U64_ID] {
            return Err(TabulaError::InvalidIr(format!(
                "test state runtime only supports [u64] key schema, table {} declared {:?}",
                table.0, key_types
            )));
        }
        let [value] = key else {
            return Err(TabulaError::InvalidIr(
                "test state runtime only supports single-component state keys".into(),
            ));
        };
        if value.type_id() != TYPE_U64_ID {
            return Err(TabulaError::InvalidIr(format!(
                "test state runtime expects state keys to be u64, got {}",
                value.type_id().0
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
        if key_types != vec![TYPE_U64_ID] {
            return Err(TabulaError::InvalidIr(format!(
                "test state runtime only supports [u64] key schema, table {} declared {:?}",
                table.0, key_types
            )));
        }
        if key.0.len() != std::mem::size_of::<u64>() {
            return Err(TabulaError::InvalidIr(format!(
                "expected 8 committed key bytes for table {}, got {}",
                table.0,
                key.0.len()
            )));
        }
        let raw = u64::from_le_bytes(key.0.clone().try_into().expect("u64 bytes"));
        Ok(vec![u64_typed(raw)])
    }

    fn encode_key_payload(
        &self,
        table: ir::TableId,
        key: &CommittedKey,
    ) -> Result<NativeKeyPayload, TabulaError> {
        let [value] = self
            .decode_committed_key(table, key)?
            .try_into()
            .map_err(|_| {
                TabulaError::InvalidIr(
                    "test state runtime expected exactly one key component".into(),
                )
            })?;
        let raw = u64::from_le_bytes(value.payload().try_into().expect("u64 payload"));
        encode_structural_u64::<{ tabula_types::NATIVE_KEY_PAYLOAD_WIDTH }>(raw)?
            .try_into()
            .map_err(|_| TabulaError::ProofError {
                phase: "test_state_runtime_key_payload",
                detail: "failed to build fixed-width key payload".into(),
            })
    }

    fn compare_keys(
        &self,
        table: ir::TableId,
        lhs: &CommittedKey,
        rhs: &CommittedKey,
    ) -> Result<Ordering, TabulaError> {
        let lhs = self.decode_committed_key(table, lhs)?;
        let rhs = self.decode_committed_key(table, rhs)?;
        let [lhs]: [TypedValue; 1] = lhs
            .try_into()
            .map_err(|_| TabulaError::InvalidIr("expected one lhs key component".into()))?;
        let [rhs]: [TypedValue; 1] = rhs
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
        self.table_key_types.get(&table).cloned().ok_or_else(|| {
            TabulaError::InvalidIr(format!("missing key schema for state table {}", table.0))
        })
    }

    fn column_type(
        &self,
        table: ir::TableId,
        field: ir::FieldId,
    ) -> Result<tabula_core::TypeId, TabulaError> {
        self.column_types
            .get(&(table, field))
            .copied()
            .ok_or_else(|| {
                TabulaError::InvalidIr(format!(
                    "missing state column contract {}.{}",
                    table.0, field.0
                ))
            })
    }

    fn resolve_property(
        &self,
        table: ir::TableId,
        field: ir::FieldId,
        query: &CommittedPropertyQuery,
        state: &[CommittedColumnEntry],
    ) -> Result<TypedCommittedPropertyQueryResult, TabulaError> {
        let field_type = self.column_type(table, field)?;
        for entry in state {
            if entry.value.type_id() != field_type {
                return Err(TabulaError::InvalidIr(format!(
                    "committed column {}.{} yielded value type {} but field type is {}",
                    table.0,
                    field.0,
                    entry.value.type_id().0,
                    field_type.0
                )));
            }
        }

        let pick =
            match query {
                CommittedPropertyQuery::Minimum => state
                    .iter()
                    .filter(|entry| !entry.is_null)
                    .min_by(|lhs, rhs| {
                        self.compare_keys(table, &lhs.key, &rhs.key)
                            .expect("compare state keys")
                    }),
                CommittedPropertyQuery::Maximum => state
                    .iter()
                    .filter(|entry| !entry.is_null)
                    .max_by(|lhs, rhs| {
                        self.compare_keys(table, &lhs.key, &rhs.key)
                            .expect("compare state keys")
                    }),
                CommittedPropertyQuery::Successor { key } => state
                    .iter()
                    .filter(|entry| !entry.is_null)
                    .filter(|entry| {
                        self.compare_keys(table, &entry.key, key)
                            .expect("compare state keys")
                            == Ordering::Greater
                    })
                    .min_by(|lhs, rhs| {
                        self.compare_keys(table, &lhs.key, &rhs.key)
                            .expect("compare state keys")
                    }),
                CommittedPropertyQuery::Predecessor { key } => state
                    .iter()
                    .filter(|entry| !entry.is_null)
                    .filter(|entry| {
                        self.compare_keys(table, &entry.key, key)
                            .expect("compare state keys")
                            == Ordering::Less
                    })
                    .max_by(|lhs, rhs| {
                        self.compare_keys(table, &lhs.key, &rhs.key)
                            .expect("compare state keys")
                    }),
                CommittedPropertyQuery::Aggregate { .. } => {
                    return Err(TabulaError::InvalidIr(
                        "Aggregate is not yet supported in test state runtime".into(),
                    ));
                }
                CommittedPropertyQuery::NonExistenceRange { .. } => {
                    return Err(TabulaError::InvalidIr(
                        "NonExistenceRange is not yet supported in test state runtime".into(),
                    ));
                }
            };

        if let Some(entry) = pick {
            Ok(TypedCommittedPropertyQueryResult {
                value: entry.value.clone(),
                key: Some(entry.key.clone()),
                is_null: false,
            })
        } else {
            Ok(TypedCommittedPropertyQueryResult {
                value: u64_typed(0),
                key: None,
                is_null: true,
            })
        }
    }
}

pub fn committed_u64_state(
    table: ir::TableId,
    field: ir::FieldId,
    rows: &[(u64, u64, bool)],
) -> tabula_core::InMemoryState {
    let mut state = tabula_core::InMemoryState::new();
    for (row_key, value, is_null) in rows {
        if *is_null {
            continue;
        }
        state.set(
            CommittedCellKey {
                table: TableId(table.0),
                col: ColId(field.0),
                key: CommittedKey(row_key.to_le_bytes().to_vec()),
            },
            portable_u64(*value),
        );
    }
    state
}

pub fn test_state_runtime() -> &'static TestStateRuntime {
    static RUNTIME: OnceLock<TestStateRuntime> = OnceLock::new();
    RUNTIME
        .get_or_init(|| TestStateRuntime::default().with_u64_column(ir::TableId(1), ir::FieldId(0)))
}

pub fn property_program(
    query: ir::StatePropertyQuery,
    key_ty: tabula_core::TypeId,
) -> exec::ResolvedExecutionProgram {
    exec::ResolvedExecutionProgram::from_validated_program(
        ir::ValidatedProgram::try_from(ir::Program {
            program_id: ir::ProgramId(1),
            state: ir::StateSchema {
                tables: vec![ir::TableSchema {
                    id: ir::TableId(1),
                    symbol: "accounts".into(),
                    keys: vec![KeyComponentSchema {
                        symbol: "id".into(),
                        ty: key_ty,
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
            capability_manifest: ir::CapabilityManifest { entries: vec![] },
            event_manifest: ir::EventManifest { entries: vec![] },
            entries: vec![ir::Entry {
                id: ir::EntryId(0),
                symbol: "property".into(),
                kind: ir::EntryKind::Query,
                params: vec![],
                returns: vec![TYPE_U64_ID, key_ty, TYPE_BOOL_ID],
                return_policy: ir::ReturnPolicy::Explicit,
                body: ir::Body {
                    locals: vec![
                        ir::LocalDecl {
                            id: ir::LocalId(0),
                            ty: TYPE_U64_ID,
                        },
                        ir::LocalDecl {
                            id: ir::LocalId(1),
                            ty: key_ty,
                        },
                        ir::LocalDecl {
                            id: ir::LocalId(2),
                            ty: TYPE_BOOL_ID,
                        },
                    ],
                    ops: vec![
                        ir::Op::ReadStateProperty {
                            guard: None,
                            dst_value: ir::LocalId(0),
                            dst_key_components: vec![ir::LocalId(1)],
                            dst_is_null: ir::LocalId(2),
                            table: ir::TableId(1),
                            field: ir::FieldId(0),
                            query,
                        },
                        ir::Op::Return {
                            values: ir::ValueTupleRef(vec![
                                ir::ValueRef::Local(ir::LocalId(0)),
                                ir::ValueRef::Local(ir::LocalId(1)),
                                ir::ValueRef::Local(ir::LocalId(2)),
                            ]),
                        },
                    ],
                },
            }],
        })
        .expect("valid property program"),
    )
    .expect("resolved property program")
}

pub fn query_exec_context<'a>(runtimes: &'a TypeRuntimeRegistry) -> exec::ExecContext<'a> {
    exec::ExecContext {
        hasher: &XorHasher,
        type_runtimes: runtimes,
        capability_executor: None,
        state_runtime: test_state_runtime(),
    }
}
