use std::collections::BTreeMap;
use std::sync::Arc;

use borsh::{BorshDeserialize, BorshSerialize};
use tabula_core::error::TabulaError;
use tabula_core::traits::Hasher;
use tabula_core::{Digest, PortableValue};
use tabula_executor::ir_next as exec;
use tabula_ir_next as ir;
use tabula_types::TypeRuntimeRegistry;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofStateSlot {
    pub table: ir::TableId,
    pub field: ir::FieldId,
    pub ty: ir::TypeRef,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityProofSlot {
    pub capability: ir::CapabilityId,
    pub symbol: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationProofSlot {
    pub relation: ir::RelationId,
    pub symbol: String,
}

#[derive(Debug, Clone)]
pub struct ResolvedProofProgram {
    program: Arc<ir::ValidatedProgram>,
    state_slots: Vec<ProofStateSlot>,
    state_index: BTreeMap<(ir::TableId, ir::FieldId), usize>,
    capability_slots: Vec<CapabilityProofSlot>,
    capability_visibility: BTreeMap<ir::CapabilityId, ir::CapabilityProofVisibility>,
    relation_slots: Vec<RelationProofSlot>,
    capability_index: BTreeMap<ir::CapabilityId, usize>,
    relation_index: BTreeMap<ir::RelationId, usize>,
}

impl ResolvedProofProgram {
    pub fn from_validated_program(program: ir::ValidatedProgram) -> Result<Self, TabulaError> {
        Self::from_shared_program(Arc::new(program))
    }

    pub fn from_shared_program(program: Arc<ir::ValidatedProgram>) -> Result<Self, TabulaError> {
        let raw = program.as_program();
        let mut state_slots = Vec::new();
        let mut state_index = BTreeMap::new();
        for table in &raw.state.tables {
            for field in &table.fields {
                let slot_index = state_slots.len();
                state_index.insert((table.id, field.id), slot_index);
                state_slots.push(ProofStateSlot {
                    table: table.id,
                    field: field.id,
                    ty: field.ty,
                });
            }
        }

        let capability_visibility = raw
            .capability_manifest
            .entries
            .iter()
            .map(|entry| (entry.id, entry.proof_visibility))
            .collect();
        let capability_slots = raw
            .capability_manifest
            .entries
            .iter()
            .filter(|entry| entry.proof_visibility == ir::CapabilityProofVisibility::Journaled)
            .map(|entry| CapabilityProofSlot {
                capability: entry.id,
                symbol: entry.symbol.clone(),
            })
            .collect::<Vec<_>>();
        let capability_index = capability_slots
            .iter()
            .enumerate()
            .map(|(index, slot)| (slot.capability, index))
            .collect();

        let relation_slots = raw
            .relation_manifest
            .entries
            .iter()
            .map(|entry| RelationProofSlot {
                relation: entry.id,
                symbol: entry.descriptor.symbol.clone(),
            })
            .collect::<Vec<_>>();
        let relation_index = relation_slots
            .iter()
            .enumerate()
            .map(|(index, slot)| (slot.relation, index))
            .collect();

        Ok(Self {
            program,
            state_slots,
            state_index,
            capability_slots,
            capability_visibility,
            relation_slots,
            capability_index,
            relation_index,
        })
    }

    pub fn validated_program(&self) -> &ir::ValidatedProgram {
        self.program.as_ref()
    }

    pub fn program(&self) -> &ir::Program {
        self.program.as_program()
    }

    pub fn state_slots(&self) -> &[ProofStateSlot] {
        &self.state_slots
    }

    pub fn capability_slots(&self) -> &[CapabilityProofSlot] {
        &self.capability_slots
    }

    pub fn relation_slots(&self) -> &[RelationProofSlot] {
        &self.relation_slots
    }
}

#[derive(Debug, Clone)]
pub struct RuntimeProgram {
    execution: exec::ResolvedExecutionProgram,
    proof: ResolvedProofProgram,
}

impl RuntimeProgram {
    pub fn from_validated_program(program: ir::ValidatedProgram) -> Result<Self, TabulaError> {
        let shared = Arc::new(program);
        let execution = exec::ResolvedExecutionProgram::from_shared_program(shared.clone())?;
        let proof = ResolvedProofProgram::from_shared_program(shared)?;
        Ok(Self { execution, proof })
    }

    pub fn execution(&self) -> &exec::ResolvedExecutionProgram {
        &self.execution
    }

    pub fn proof(&self) -> &ResolvedProofProgram {
        &self.proof
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicContextBinding {
    pub field: ir::ContextFieldId,
    pub value: PortableValue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofStateSlotJournal {
    pub slot: ProofStateSlot,
    pub state_effects: Vec<exec::TypedStateEffect>,
    pub property_effects: Vec<exec::StatePropertyEffect>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofCapabilitySlotJournal {
    pub slot: CapabilityProofSlot,
    pub effects: Vec<exec::CapabilityEffect>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofRelationSlotJournal {
    pub slot: RelationProofSlot,
    pub effects: Vec<exec::RelationEffect>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofJournal {
    pub public_context: Vec<PublicContextBinding>,
    pub state_slots: Vec<ProofStateSlotJournal>,
    pub capability_slots: Vec<ProofCapabilitySlotJournal>,
    pub relation_slots: Vec<ProofRelationSlotJournal>,
    pub event_effects: Vec<exec::TypedEventEffect>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicStatement {
    pub program_id: ir::ProgramId,
    pub public_context: Vec<PublicContextBinding>,
    pub event_digest: Digest,
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct PortableEventRecord {
    event: ir::EventId,
    args: Vec<PortableValue>,
}

pub fn reduce_execution_journal(
    resolved_program: &ResolvedProofProgram,
    context: &exec::ContextValues,
    execution_journal: &exec::ExecutionJournal,
    type_runtimes: &TypeRuntimeRegistry,
) -> Result<ProofJournal, TabulaError> {
    let public_context = encode_public_context(resolved_program, context, type_runtimes)?;
    let mut state_slots = resolved_program
        .state_slots
        .iter()
        .cloned()
        .map(|slot| ProofStateSlotJournal {
            slot,
            state_effects: Vec::new(),
            property_effects: Vec::new(),
        })
        .collect::<Vec<_>>();
    let mut capability_slots = resolved_program
        .capability_slots
        .iter()
        .cloned()
        .map(|slot| ProofCapabilitySlotJournal {
            slot,
            effects: Vec::new(),
        })
        .collect::<Vec<_>>();
    let mut relation_slots = resolved_program
        .relation_slots
        .iter()
        .cloned()
        .map(|slot| ProofRelationSlotJournal {
            slot,
            effects: Vec::new(),
        })
        .collect::<Vec<_>>();
    let mut event_effects = Vec::new();

    for tx in execution_journal.successful_txs() {
        for effect in &tx.state_effects {
            let slot = resolved_program
                .state_index
                .get(&(
                    ir::TableId(effect.key.table.0),
                    ir::FieldId(effect.key.col.0),
                ))
                .ok_or_else(|| {
                    TabulaError::InvalidIr(format!(
                        "journal referenced unknown proof-visible state slot {}.{}",
                        effect.key.table.0, effect.key.col.0
                    ))
                })?;
            state_slots[*slot].state_effects.push(effect.clone());
        }
        for effect in &tx.property_effects {
            let slot = resolved_program
                .state_index
                .get(&(effect.table, effect.field))
                .ok_or_else(|| {
                    TabulaError::InvalidIr(format!(
                        "journal referenced unknown proof-visible property slot {}.{}",
                        effect.table.0, effect.field.0
                    ))
                })?;
            state_slots[*slot].property_effects.push(effect.clone());
        }
        event_effects.extend(tx.event_effects.iter().cloned());
        for effect in &tx.capability_effects {
            match resolved_program
                .capability_visibility
                .get(&effect.capability)
                .copied()
            {
                Some(ir::CapabilityProofVisibility::Journaled) => {
                    let slot = resolved_program
                        .capability_index
                        .get(&effect.capability)
                        .ok_or_else(|| {
                            TabulaError::InvalidIr(format!(
                                "journal referenced unknown proof-visible capability {}",
                                effect.capability.0
                            ))
                        })?;
                    capability_slots[*slot].effects.push(effect.clone());
                }
                Some(ir::CapabilityProofVisibility::OpaqueRuntimeOnly) => {}
                None => {
                    return Err(TabulaError::InvalidIr(format!(
                        "journal referenced unknown capability {}",
                        effect.capability.0
                    )));
                }
            }
        }
        for effect in &tx.relation_effects {
            let slot = resolved_program
                .relation_index
                .get(&effect.relation)
                .ok_or_else(|| {
                    TabulaError::InvalidIr(format!(
                        "journal referenced unknown relation {}",
                        effect.relation.0
                    ))
                })?;
            relation_slots[*slot].effects.push(effect.clone());
        }
    }

    Ok(ProofJournal {
        public_context,
        state_slots,
        capability_slots,
        relation_slots,
        event_effects,
    })
}

pub fn build_public_statement(
    resolved_program: &ResolvedProofProgram,
    context: &exec::ContextValues,
    execution_journal: &exec::ExecutionJournal,
    hasher: &dyn Hasher,
    type_runtimes: &TypeRuntimeRegistry,
) -> Result<PublicStatement, TabulaError> {
    let proof_journal =
        reduce_execution_journal(resolved_program, context, execution_journal, type_runtimes)?;
    build_public_statement_from_journal(&proof_journal, resolved_program, hasher)
}

pub fn build_public_statement_from_journal(
    proof_journal: &ProofJournal,
    resolved_program: &ResolvedProofProgram,
    hasher: &dyn Hasher,
) -> Result<PublicStatement, TabulaError> {
    Ok(PublicStatement {
        program_id: resolved_program.program().program_id,
        public_context: proof_journal.public_context.clone(),
        event_digest: compute_event_digest(hasher, &proof_journal.event_effects)?,
    })
}

fn encode_public_context(
    resolved_program: &ResolvedProofProgram,
    context: &exec::ContextValues,
    type_runtimes: &TypeRuntimeRegistry,
) -> Result<Vec<PublicContextBinding>, TabulaError> {
    let schema = &resolved_program.program().context.fields;
    if context.fields.len() != schema.len() {
        return Err(TabulaError::ParamSchemaMismatch(format!(
            "program expects {} context values but got {}",
            schema.len(),
            context.fields.len()
        )));
    }

    schema
        .iter()
        .map(|field| {
            let value = context.fields.get(&field.id).ok_or_else(|| {
                TabulaError::ParamSchemaMismatch(format!("missing context field {}", field.symbol))
            })?;
            if value.type_id() != field.ty {
                return Err(TabulaError::ParamSchemaMismatch(format!(
                    "context field {} expects type {} but got {}",
                    field.symbol,
                    field.ty.0,
                    value.type_id().0
                )));
            }
            Ok(PublicContextBinding {
                field: field.id,
                value: type_runtimes.encode_typed(value)?,
            })
        })
        .collect()
}

fn compute_event_digest(
    hasher: &dyn Hasher,
    events: &[exec::TypedEventEffect],
) -> Result<Digest, TabulaError> {
    let mut items = Vec::with_capacity(events.len() + 1);
    items.push(b"tabula.runtime.ir_next.statement.v1".to_vec());
    for event in events {
        let record = PortableEventRecord {
            event: event.event,
            args: event
                .args
                .iter()
                .map(|arg| PortableValue::new(arg.type_id(), arg.payload().to_vec()))
                .collect(),
        };
        items.push(
            borsh::to_vec(&record)
                .map_err(|error| TabulaError::BorshEncodingError(error.to_string()))?,
        );
    }
    let refs = items.iter().map(Vec::as_slice).collect::<Vec<_>>();
    Ok(hasher.hash_many(&refs))
}

#[cfg(test)]
mod tests {
    use borsh::to_vec;
    use tabula_core::traits::Hasher;
    use tabula_core::{CellKey, InMemoryState, PortableValue, RowKey};
    use tabula_profile::TYPE_U64_ID;
    use tabula_types::{TypeRuntimeRegistry, TypedColumnEntry, TypedValue, u64_typed};

    use super::*;

    struct XorHasher;

    impl Hasher for XorHasher {
        fn hash(&self, data: &[u8]) -> Digest {
            let mut out = [0u8; 32];
            for (index, byte) in data.iter().enumerate() {
                out[index % 32] ^= byte;
            }
            out
        }

        fn hash_pair(&self, left: &Digest, right: &Digest) -> Digest {
            let mut data = Vec::new();
            data.extend_from_slice(left);
            data.extend_from_slice(right);
            self.hash(&data)
        }
    }

    struct AddOneCapability;

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

    struct FixedCommittedColumns {
        columns: BTreeMap<(ir::TableId, ir::FieldId), Vec<TypedColumnEntry>>,
    }

    impl exec::CommittedColumnProvider for FixedCommittedColumns {
        fn get_column(
            &self,
            table: ir::TableId,
            field: ir::FieldId,
        ) -> Result<Vec<TypedColumnEntry>, TabulaError> {
            self.columns.get(&(table, field)).cloned().ok_or_else(|| {
                TabulaError::InvalidIr(format!("missing committed column {}.{}", table.0, field.0))
            })
        }
    }

    fn portable_u64(value: u64) -> PortableValue {
        PortableValue::new(TYPE_U64_ID, to_vec(&value).unwrap())
    }

    fn validated_program() -> ir::ValidatedProgram {
        ir::ValidatedProgram::try_from(ir::Program {
            program_id: ir::ProgramId(99),
            state: ir::StateSchema {
                tables: vec![ir::TableSchema {
                    id: ir::TableId(1),
                    symbol: "accounts".into(),
                    key_tys: vec![TYPE_U64_ID],
                    fields: vec![
                        ir::FieldSchema {
                            id: ir::FieldId(0),
                            symbol: "balance".into(),
                            ty: TYPE_U64_ID,
                        },
                        ir::FieldSchema {
                            id: ir::FieldId(1),
                            symbol: "nonce".into(),
                            ty: TYPE_U64_ID,
                        },
                    ],
                }],
            },
            context: ir::ContextSchema {
                fields: vec![ir::ContextField {
                    id: ir::ContextFieldId(0),
                    symbol: "epoch".into(),
                    ty: TYPE_U64_ID,
                }],
            },
            const_pool: ir::ConstantPool { entries: vec![] },
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
                            rows: vec![ir::RelationRow {
                                inputs: vec![portable_u64(1)],
                                outputs: vec![portable_u64(10)],
                            }],
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
            entries: vec![ir::Entry {
                id: ir::EntryId(0),
                symbol: "transfer".into(),
                kind: ir::EntryKind::Tx,
                params: vec![
                    ir::ParamDecl {
                        id: ir::ParamId(0),
                        symbol: "to".into(),
                        ty: TYPE_U64_ID,
                    },
                    ir::ParamDecl {
                        id: ir::ParamId(1),
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
                            ty: TYPE_U64_ID,
                        },
                    ],
                    ops: vec![
                        ir::Op::AssertRelation {
                            guard: None,
                            relation: ir::RelationId(0),
                            args: ir::ValueTupleRef(vec![ir::ValueRef::Param(ir::ParamId(1))]),
                        },
                        ir::Op::EvalRelation {
                            guard: None,
                            relation: ir::RelationId(1),
                            inputs: ir::ValueTupleRef(vec![ir::ValueRef::Param(ir::ParamId(1))]),
                            dsts: vec![ir::LocalId(0)],
                        },
                        ir::Op::CallCapability {
                            guard: None,
                            capability: ir::CapabilityId(7),
                            inputs: ir::ValueTupleRef(vec![ir::ValueRef::Local(ir::LocalId(0))]),
                            dsts: vec![ir::LocalId(1)],
                        },
                        ir::Op::WriteState {
                            guard: None,
                            table: ir::TableId(1),
                            key: ir::ValueTupleRef(vec![ir::ValueRef::Param(ir::ParamId(0))]),
                            field: ir::FieldId(0),
                            value: ir::ValueRef::Local(ir::LocalId(1)),
                        },
                        ir::Op::EmitEvent {
                            guard: None,
                            event: ir::EventId(0),
                            args: ir::ValueTupleRef(vec![
                                ir::ValueRef::Param(ir::ParamId(0)),
                                ir::ValueRef::Local(ir::LocalId(1)),
                            ]),
                        },
                        ir::Op::Return {
                            values: ir::ValueTupleRef(vec![]),
                        },
                    ],
                },
            }],
        })
        .expect("valid next program")
    }

    #[test]
    fn runtime_program_builds_parallel_execution_and_proof_contracts() {
        let runtime_program =
            RuntimeProgram::from_validated_program(validated_program()).expect("runtime program");

        assert_eq!(runtime_program.proof().state_slots().len(), 2);
        assert_eq!(
            runtime_program.proof().state_slots()[0].field,
            ir::FieldId(0)
        );
        assert_eq!(
            runtime_program.proof().state_slots()[1].field,
            ir::FieldId(1)
        );
        assert_eq!(runtime_program.proof().capability_slots().len(), 1);
        assert_eq!(runtime_program.proof().relation_slots().len(), 2);
        assert_eq!(
            runtime_program.execution().program().program_id,
            runtime_program.proof().program().program_id
        );
    }

    #[test]
    fn runtime_reduction_builds_proof_journal_and_statement() {
        let runtimes = TypeRuntimeRegistry::seeded().expect("seeded runtimes");
        let runtime_program =
            RuntimeProgram::from_validated_program(validated_program()).expect("runtime program");
        let mut capabilities = exec::CapabilityRegistry::new();
        capabilities.register(AddOneCapability).unwrap();
        let exec_ctx = exec::ExecContext {
            hasher: &XorHasher,
            type_runtimes: &runtimes,
            capabilities: Some(&capabilities),
            committed_columns: None,
        };
        let state = InMemoryState::new();
        let mut context = exec::ContextValues::new();
        context.insert(ir::ContextFieldId(0), u64_typed(7));

        let journal = exec::execute_batch(
            runtime_program.execution(),
            &[exec::TxCall {
                entry_id: ir::EntryId(0),
                params: vec![u64_typed(2), u64_typed(1)],
            }],
            &context,
            &state,
            &exec_ctx,
        )
        .expect("batch executes");

        let proof_journal =
            reduce_execution_journal(runtime_program.proof(), &context, &journal, &runtimes)
                .expect("proof journal");
        let statement = build_public_statement(
            runtime_program.proof(),
            &context,
            &journal,
            &XorHasher,
            &runtimes,
        )
        .expect("public statement");

        assert_eq!(proof_journal.public_context.len(), 1);
        assert_eq!(proof_journal.state_slots.len(), 2);
        assert_eq!(proof_journal.state_slots[0].slot.field, ir::FieldId(0));
        assert_eq!(proof_journal.state_slots[0].state_effects.len(), 1);
        assert_eq!(proof_journal.state_slots[0].property_effects.len(), 0);
        assert_eq!(proof_journal.state_slots[1].slot.field, ir::FieldId(1));
        assert!(proof_journal.state_slots[1].state_effects.is_empty());
        assert!(proof_journal.state_slots[1].property_effects.is_empty());
        assert_eq!(proof_journal.capability_slots.len(), 1);
        assert_eq!(proof_journal.capability_slots[0].effects.len(), 1);
        assert_eq!(proof_journal.relation_slots.len(), 2);
        assert_eq!(proof_journal.relation_slots[0].effects.len(), 1);
        assert_eq!(proof_journal.relation_slots[1].effects.len(), 1);
        assert_eq!(proof_journal.event_effects.len(), 1);
        assert_ne!(statement.event_digest, [0u8; 32]);
        assert_eq!(statement.program_id, ir::ProgramId(99));
        assert_eq!(statement.public_context, proof_journal.public_context);
    }

    #[test]
    fn opaque_capability_effects_are_filtered_out_of_proof_journal() {
        let runtimes = TypeRuntimeRegistry::seeded().expect("seeded runtimes");
        let mut raw = validated_program().into_program();
        raw.capability_manifest.entries[0].proof_visibility =
            ir::CapabilityProofVisibility::OpaqueRuntimeOnly;
        let runtime_program = RuntimeProgram::from_validated_program(
            ir::ValidatedProgram::try_from(raw).expect("revalidated program"),
        )
        .expect("runtime program");
        let mut capabilities = exec::CapabilityRegistry::new();
        capabilities.register(AddOneCapability).unwrap();
        let exec_ctx = exec::ExecContext {
            hasher: &XorHasher,
            type_runtimes: &runtimes,
            capabilities: Some(&capabilities),
            committed_columns: None,
        };
        let state = InMemoryState::new();
        let mut context = exec::ContextValues::new();
        context.insert(ir::ContextFieldId(0), u64_typed(9));

        let journal = exec::execute_batch(
            runtime_program.execution(),
            &[exec::TxCall {
                entry_id: ir::EntryId(0),
                params: vec![u64_typed(2), u64_typed(1)],
            }],
            &context,
            &state,
            &exec_ctx,
        )
        .expect("batch executes");

        let tx = journal.successful_txs().next().expect("successful tx");
        assert_eq!(tx.capability_effects.len(), 1);

        let proof_journal =
            reduce_execution_journal(runtime_program.proof(), &context, &journal, &runtimes)
                .expect("proof journal");
        assert!(proof_journal.capability_slots.is_empty());
    }

    #[test]
    fn property_effects_are_grouped_into_their_state_slot() {
        let runtimes = TypeRuntimeRegistry::seeded().expect("seeded runtimes");
        let property_program = ir::ValidatedProgram::try_from(ir::Program {
            program_id: ir::ProgramId(100),
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
            capability_manifest: ir::CapabilityManifest { entries: vec![] },
            event_manifest: ir::EventManifest { entries: vec![] },
            entries: vec![ir::Entry {
                id: ir::EntryId(0),
                symbol: "scan".into(),
                kind: ir::EntryKind::Tx,
                params: vec![],
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
                            ty: TYPE_U64_ID,
                        },
                        ir::LocalDecl {
                            id: ir::LocalId(2),
                            ty: tabula_profile::TYPE_BOOL_ID,
                        },
                    ],
                    ops: vec![
                        ir::Op::ReadStateProperty {
                            guard: None,
                            dsts: vec![ir::LocalId(0), ir::LocalId(1), ir::LocalId(2)],
                            table: ir::TableId(1),
                            field: ir::FieldId(0),
                            query: ir::StatePropertyQuery::Maximum,
                        },
                        ir::Op::Return {
                            values: ir::ValueTupleRef(vec![]),
                        },
                    ],
                },
            }],
        })
        .expect("property program");
        let runtime_program =
            RuntimeProgram::from_validated_program(property_program).expect("runtime program");
        let committed_columns = FixedCommittedColumns {
            columns: BTreeMap::from([(
                (ir::TableId(1), ir::FieldId(0)),
                vec![
                    TypedColumnEntry {
                        row_key: RowKey(1),
                        value: u64_typed(5),
                        is_null: false,
                    },
                    TypedColumnEntry {
                        row_key: RowKey(3),
                        value: u64_typed(8),
                        is_null: false,
                    },
                ],
            )]),
        };
        let exec_ctx = exec::ExecContext {
            hasher: &XorHasher,
            type_runtimes: &runtimes,
            capabilities: None,
            committed_columns: Some(&committed_columns),
        };
        let state = InMemoryState::new();
        let context = exec::ContextValues::new();

        let journal = exec::execute_batch(
            runtime_program.execution(),
            &[exec::TxCall {
                entry_id: ir::EntryId(0),
                params: vec![],
            }],
            &context,
            &state,
            &exec_ctx,
        )
        .expect("batch executes");
        let proof_journal =
            reduce_execution_journal(runtime_program.proof(), &context, &journal, &runtimes)
                .expect("proof journal");

        assert_eq!(proof_journal.state_slots.len(), 1);
        assert!(proof_journal.state_slots[0].state_effects.is_empty());
        assert_eq!(proof_journal.state_slots[0].property_effects.len(), 1);
        assert_eq!(
            proof_journal.state_slots[0].property_effects[0].outputs,
            vec![u64_typed(8), u64_typed(3), tabula_types::bool_typed(false)]
        );
    }

    #[test]
    fn public_statement_is_deterministic_for_same_journal() {
        let runtimes = TypeRuntimeRegistry::seeded().expect("seeded runtimes");
        let runtime_program =
            RuntimeProgram::from_validated_program(validated_program()).expect("runtime program");
        let mut capabilities = exec::CapabilityRegistry::new();
        capabilities.register(AddOneCapability).unwrap();
        let exec_ctx = exec::ExecContext {
            hasher: &XorHasher,
            type_runtimes: &runtimes,
            capabilities: Some(&capabilities),
            committed_columns: None,
        };
        let mut state = InMemoryState::new();
        state.set(
            CellKey {
                table: tabula_core::TableId(1),
                col: tabula_core::ColId(0),
                row: RowKey(2),
            },
            portable_u64(1),
        );
        let mut context = exec::ContextValues::new();
        context.insert(ir::ContextFieldId(0), u64_typed(11));

        let journal = exec::execute_batch(
            runtime_program.execution(),
            &[exec::TxCall {
                entry_id: ir::EntryId(0),
                params: vec![u64_typed(2), u64_typed(1)],
            }],
            &context,
            &state,
            &exec_ctx,
        )
        .expect("batch executes");

        let first = build_public_statement(
            runtime_program.proof(),
            &context,
            &journal,
            &XorHasher,
            &runtimes,
        )
        .expect("statement");
        let second = build_public_statement(
            runtime_program.proof(),
            &context,
            &journal,
            &XorHasher,
            &runtimes,
        )
        .expect("statement");

        assert_eq!(first, second);
    }
}
