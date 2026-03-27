#![allow(dead_code, missing_docs)]

use std::collections::BTreeMap;

use borsh::to_vec;
use tabula_core::PortableValue;
use tabula_core::RowKey;
use tabula_core::error::TabulaError;
use tabula_core::traits::Hasher;
use tabula_executor as exec;
use tabula_ir as ir;
use tabula_profile::{TYPE_BOOL_ID, TYPE_BYTES32_ID, TYPE_U64_ID};
use tabula_types::{TypeRuntimeRegistry, TypedValue, bool_typed, u64_typed};

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
                key_tys: vec![TYPE_U64_ID],
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
pub struct TestPropertyReads {
    columns: BTreeMap<(ir::TableId, ir::FieldId), Vec<tabula_types::TypedColumnEntry>>,
}

impl TestPropertyReads {
    pub fn with_u64_column(
        mut self,
        table: ir::TableId,
        field: ir::FieldId,
        rows: &[(u64, u64, bool)],
    ) -> Self {
        self.columns.insert(
            (table, field),
            rows.iter()
                .map(|(row_key, value, is_null)| tabula_types::TypedColumnEntry {
                    row_key: RowKey(*row_key),
                    value: u64_typed(*value),
                    is_null: *is_null,
                })
                .collect(),
        );
        self
    }
}

impl exec::PropertyReadExecutor for TestPropertyReads {
    fn execute(
        &self,
        request: &exec::PropertyReadRequest,
        type_runtimes: &TypeRuntimeRegistry,
    ) -> Result<Vec<TypedValue>, TabulaError> {
        if request.output_arity != 3 {
            return Err(TabulaError::InvalidIr(
                "row-oriented property reads require exactly 3 outputs".into(),
            ));
        }
        if request.key_arity != 1 || request.key_type != TYPE_U64_ID {
            return Err(TabulaError::InvalidIr(format!(
                "V1 canonical executor only supports [u64] key schema, table {} declared arity {} with key type {}",
                request.table.0, request.key_arity, request.key_type.0
            )));
        }
        match &request.query {
            exec::PropertyReadQuery::Aggregate { .. } => {
                return Err(TabulaError::InvalidIr(
                    "ReadStateProperty Aggregate is not yet supported in V1 adapter".into(),
                ));
            }
            exec::PropertyReadQuery::NonExistenceRange { .. } => {
                return Err(TabulaError::InvalidIr(
                    "ReadStateProperty NonExistenceRange is not yet supported in V1 adapter".into(),
                ));
            }
            _ => {}
        }
        let entries = self
            .columns
            .get(&(request.table, request.field))
            .map(Vec::as_slice)
            .ok_or_else(|| {
                TabulaError::InvalidIr(format!(
                    "missing committed column {}.{}",
                    request.table.0, request.field.0
                ))
            })?;
        for entry in entries {
            if entry.value.type_id() != request.field_type {
                return Err(TabulaError::InvalidIr(format!(
                    "committed column {}.{} yielded value type {} but field type is {}",
                    request.table.0,
                    request.field.0,
                    entry.value.type_id().0,
                    request.field_type.0
                )));
            }
        }

        let pick = match &request.query {
            exec::PropertyReadQuery::Minimum => entries
                .iter()
                .filter(|entry| !entry.is_null)
                .min_by_key(|entry| entry.row_key.0),
            exec::PropertyReadQuery::Maximum => entries
                .iter()
                .filter(|entry| !entry.is_null)
                .max_by_key(|entry| entry.row_key.0),
            exec::PropertyReadQuery::Successor { key } => {
                let pivot = decode_single_row_key(key, type_runtimes)?;
                entries
                    .iter()
                    .filter(|entry| !entry.is_null)
                    .filter(|entry| entry.row_key.0 > pivot.0)
                    .min_by_key(|entry| entry.row_key.0)
            }
            exec::PropertyReadQuery::Predecessor { key } => {
                let pivot = decode_single_row_key(key, type_runtimes)?;
                entries
                    .iter()
                    .filter(|entry| !entry.is_null)
                    .filter(|entry| entry.row_key.0 < pivot.0)
                    .max_by_key(|entry| entry.row_key.0)
            }
            exec::PropertyReadQuery::Aggregate { .. }
            | exec::PropertyReadQuery::NonExistenceRange { .. } => {
                unreachable!("unsupported variants return early")
            }
        };

        if let Some(entry) = pick {
            Ok(vec![
                entry.value.clone(),
                row_key_typed(entry.row_key, type_runtimes)?,
                bool_typed(false),
            ])
        } else {
            Ok(vec![
                type_runtimes.zero_of(request.field_type)?,
                type_runtimes.zero_of(request.key_type)?,
                bool_typed(true),
            ])
        }
    }
}

fn row_key_typed(
    row: RowKey,
    type_runtimes: &TypeRuntimeRegistry,
) -> Result<TypedValue, TabulaError> {
    type_runtimes.decode_portable(&PortableValue::new(TYPE_U64_ID, to_vec(&row.0).unwrap()))
}

fn decode_single_row_key(
    values: &[TypedValue],
    type_runtimes: &TypeRuntimeRegistry,
) -> Result<RowKey, TabulaError> {
    if values.len() != 1 {
        return Err(TabulaError::InvalidIr(
            "V1 canonical executor only supports single-component state keys".into(),
        ));
    }
    let value = &values[0];
    if value.type_id() != TYPE_U64_ID {
        return Err(TabulaError::InvalidIr(format!(
            "V1 canonical executor expects state keys to be u64, got {}",
            value.type_id().0
        )));
    }
    let portable = type_runtimes.encode_typed(value)?;
    let raw = borsh::from_slice::<u64>(portable.payload())
        .map_err(|error| TabulaError::BorshEncodingError(error.to_string()))?;
    Ok(RowKey(raw))
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
                    key_tys: vec![key_ty],
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
                            dsts: vec![ir::LocalId(0), ir::LocalId(1), ir::LocalId(2)],
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
        property_reads: None,
    }
}
