//! Proof-program semantic types: slot-indexed views over the IR for witness generation.

use std::collections::BTreeMap;
use std::sync::Arc;

use p3_field::PrimeCharacteristicRing;
use p3_koala_bear::KoalaBear;
use tabula_commitment::NativeDigest;
use tabula_contract::PublicStatement;
use tabula_contract::format::public_statement_transcript::{
    EncodedTranscriptValue, event_arg_block, event_header_block, event_transcript_header_block,
    public_context_header_block, public_context_item_block, tx_batch_header_block, tx_header_block,
    tx_param_block,
};
use tabula_contract::format::typed_tuple::TupleEncodingDefaults;
use tabula_core::error::TabulaError;
use tabula_core::{Digest, PortableValue};
use tabula_executor as exec;
use tabula_ir as ir;
use tabula_types::{EncodingRuntimeRegistry, TypeRuntimeRegistry, TypedValue};

/// A state column slot in the proof layout, identifying a (table, field) pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofStateSlot {
    /// Target table.
    pub table: ir::TableId,
    /// Target field within the table.
    pub field: ir::FieldId,
    /// Type of the field value.
    pub ty: ir::TypeRef,
}

/// A capability slot in the proof layout, covering all journaled invocations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityProofSlot {
    /// The capability ID this slot covers.
    pub capability: ir::CapabilityId,
    /// Source-level capability name.
    pub symbol: String,
}

/// A relation slot in the proof layout, covering all lookups for one relation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationProofSlot {
    /// The relation ID this slot covers.
    pub relation: ir::RelationId,
    /// Source-level relation name.
    pub symbol: String,
}

/// An IR program pre-indexed for proof witness generation and slot allocation.
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
    /// Build a resolved proof program from a validated program, taking ownership.
    pub fn from_validated_program(program: ir::ValidatedProgram) -> Result<Self, TabulaError> {
        Self::from_shared_program(Arc::new(program))
    }

    /// Build a resolved proof program from a shared validated program reference.
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

    /// Borrow the underlying validated program.
    pub fn validated_program(&self) -> &ir::ValidatedProgram {
        self.program.as_ref()
    }

    /// Borrow the raw IR program.
    pub fn program(&self) -> &ir::Program {
        self.program.as_program()
    }

    /// All state column slots in proof layout order.
    pub fn state_slots(&self) -> &[ProofStateSlot] {
        &self.state_slots
    }

    /// All journaled capability slots in proof layout order.
    pub fn capability_slots(&self) -> &[CapabilityProofSlot] {
        &self.capability_slots
    }

    /// All relation slots in proof layout order.
    pub fn relation_slots(&self) -> &[RelationProofSlot] {
        &self.relation_slots
    }
}

/// A paired execution + proof program view over one validated program.
#[derive(Debug, Clone)]
pub struct RuntimeProgram {
    execution: exec::ResolvedExecutionProgram,
    proof: ResolvedProofProgram,
}

impl RuntimeProgram {
    /// Build a runtime program from a validated program, constructing both views.
    pub fn from_validated_program(program: ir::ValidatedProgram) -> Result<Self, TabulaError> {
        let shared = Arc::new(program);
        let execution = exec::ResolvedExecutionProgram::from_shared_program(shared.clone())?;
        let proof = ResolvedProofProgram::from_shared_program(shared)?;
        Ok(Self { execution, proof })
    }

    /// Borrow the execution-facing program view.
    pub fn execution(&self) -> &exec::ResolvedExecutionProgram {
        &self.execution
    }

    /// Borrow the proof-facing program view.
    pub fn proof(&self) -> &ResolvedProofProgram {
        &self.proof
    }
}

/// Per-slot execution effects for a single state column in one batch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofStateSlotJournal {
    /// The state slot this journal covers.
    pub slot: ProofStateSlot,
    /// All state cell reads/writes/deletes on this column.
    pub state_effects: Vec<exec::TypedStateEffect>,
    /// All structural property reads on this column.
    pub property_effects: Vec<exec::StatePropertyEffect>,
}

/// Per-slot execution effects for a single journaled capability in one batch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofCapabilitySlotJournal {
    /// The capability slot this journal covers.
    pub slot: CapabilityProofSlot,
    /// All invocations of this capability.
    pub effects: Vec<exec::CapabilityEffect>,
}

/// Per-slot execution effects for a single relation in one batch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofRelationSlotJournal {
    /// The relation slot this journal covers.
    pub slot: RelationProofSlot,
    /// All lookups (assertions and evaluations) of this relation.
    pub effects: Vec<exec::RelationEffect>,
}

/// The complete proof-visible journal for one executed batch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProofJournal {
    /// Committed public context values.
    pub public_context: Vec<PublicContextBinding>,
    /// Per-state-column effect journals in proof layout order.
    pub state_slots: Vec<ProofStateSlotJournal>,
    /// Per-capability effect journals in proof layout order.
    pub capability_slots: Vec<ProofCapabilitySlotJournal>,
    /// Per-relation effect journals in proof layout order.
    pub relation_slots: Vec<ProofRelationSlotJournal>,
    /// All event emissions in execution order.
    pub event_effects: Vec<ProofEventEffect>,
}

/// Execution-derived values supplied by the runtime when materializing one public statement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PublicStatementMaterialization {
    /// Canonical digest of the applied transaction batch.
    pub applied_tx_digest: Digest,
    /// Root before batch execution.
    pub old_state_root: Digest,
    /// Root after batch execution.
    pub new_state_root: Digest,
}

/// Proof-visible emitted event with its batch position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofEventEffect {
    /// Zero-based transaction index within the batch.
    pub tx_index: u32,
    /// The concrete emitted event effect.
    pub effect: exec::TypedEventEffect,
}

/// Internal canonical public-context binding used while materializing proof-visible statements.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PublicContextBinding {
    /// The context field identifier.
    pub field: ir::ContextFieldId,
    /// The portable serialized value.
    pub value: PortableValue,
}

/// Reduce an execution journal into per-slot proof journals.
pub(crate) fn reduce_execution_journal(
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
        event_effects.extend(
            tx.event_effects
                .iter()
                .cloned()
                .map(|effect| ProofEventEffect {
                    tx_index: tx.tx_index,
                    effect,
                }),
        );
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

/// Materialize the proved public statement from program, context, and execution journal.
pub(crate) fn materialize_public_statement(
    resolved_program: &ResolvedProofProgram,
    context: &exec::ContextValues,
    execution_journal: &exec::ExecutionJournal,
    materialization: PublicStatementMaterialization,
    type_runtimes: &TypeRuntimeRegistry,
    encoding_runtimes: &EncodingRuntimeRegistry,
    tuple_encoding_defaults: &TupleEncodingDefaults,
) -> Result<PublicStatement, TabulaError> {
    let proof_journal =
        reduce_execution_journal(resolved_program, context, execution_journal, type_runtimes)?;
    build_public_statement_from_journal(
        &proof_journal,
        materialization,
        type_runtimes,
        encoding_runtimes,
        tuple_encoding_defaults,
    )
}

/// Build the proved public statement directly from one pre-reduced [`ProofJournal`].
pub(crate) fn build_public_statement_from_journal(
    proof_journal: &ProofJournal,
    materialization: PublicStatementMaterialization,
    type_runtimes: &TypeRuntimeRegistry,
    encoding_runtimes: &EncodingRuntimeRegistry,
    tuple_encoding_defaults: &TupleEncodingDefaults,
) -> Result<PublicStatement, TabulaError> {
    let public_context_digest = compute_public_context_digest(
        &proof_journal.public_context,
        type_runtimes,
        encoding_runtimes,
        tuple_encoding_defaults,
    )?;
    Ok(PublicStatement {
        old_root: parse_native_digest(materialization.old_state_root, "old_state_root")?,
        new_root: parse_native_digest(materialization.new_state_root, "new_state_root")?,
        public_context_digest: parse_native_digest(public_context_digest, "public_context_digest")?,
        applied_tx_digest: parse_native_digest(
            materialization.applied_tx_digest,
            "applied_tx_digest",
        )?,
        event_digest: parse_native_digest(
            compute_event_digest(
                &proof_journal.event_effects,
                encoding_runtimes,
                tuple_encoding_defaults,
            )?,
            "event_digest",
        )?,
    })
}

fn parse_native_digest(bytes: Digest, label: &'static str) -> Result<NativeDigest, TabulaError> {
    NativeDigest::from_bytes(&bytes).map_err(|error| TabulaError::ProofError {
        phase: label,
        detail: error.to_string(),
    })
}

pub(crate) fn encode_public_context(
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

pub(crate) fn canonical_public_context(
    bindings: &[PublicContextBinding],
) -> Result<Vec<PublicContextBinding>, TabulaError> {
    let mut bindings = bindings.to_vec();
    bindings.sort_unstable_by_key(|binding| binding.field);
    for window in bindings.windows(2) {
        if window[0].field == window[1].field {
            return Err(TabulaError::ProofError {
                phase: "public_context_digest",
                detail: format!(
                    "duplicate public context binding for field {}",
                    window[0].field.0
                ),
            });
        }
    }
    Ok(bindings)
}

/// Canonical field-block payload for the public-context commitment.
pub(crate) fn canonical_public_context_payload(
    bindings: &[PublicContextBinding],
    type_runtimes: &TypeRuntimeRegistry,
    encoding_runtimes: &EncodingRuntimeRegistry,
    tuple_encoding_defaults: &TupleEncodingDefaults,
) -> Result<Vec<[KoalaBear; 8]>, TabulaError> {
    let bindings = canonical_public_context(bindings)?;
    let mut blocks = Vec::with_capacity(bindings.len() + 1);
    blocks.push(public_context_header_block(bindings.len()));
    for binding in &bindings {
        let typed = type_runtimes.decode_portable(&binding.value)?;
        let encoded =
            encode_public_statement_value(&typed, encoding_runtimes, tuple_encoding_defaults)?;
        blocks.push(public_context_item_block(binding.field, &encoded));
    }
    Ok(blocks)
}

/// Compute the canonical public-context digest from canonicalized context bindings.
pub(crate) fn compute_public_context_digest(
    bindings: &[PublicContextBinding],
    type_runtimes: &TypeRuntimeRegistry,
    encoding_runtimes: &EncodingRuntimeRegistry,
    tuple_encoding_defaults: &TupleEncodingDefaults,
) -> Result<Digest, TabulaError> {
    Ok(digest_from_blocks(&canonical_public_context_payload(
        bindings,
        type_runtimes,
        encoding_runtimes,
        tuple_encoding_defaults,
    )?))
}

/// Canonical field-block payload for the applied transaction batch commitment.
pub fn canonical_batch_payload(
    batch: &ir::EntryBatch,
    type_runtimes: &TypeRuntimeRegistry,
    encoding_runtimes: &EncodingRuntimeRegistry,
    tuple_encoding_defaults: &TupleEncodingDefaults,
) -> Result<Vec<[KoalaBear; 8]>, TabulaError> {
    let mut blocks = Vec::new();
    blocks.push(tx_batch_header_block(batch.calls.len()));
    for (tx_index, call) in batch.calls.iter().enumerate() {
        blocks.push(tx_header_block(
            tx_index as u32,
            call.entry_id,
            call.params.len(),
        ));
        for (param_index, value) in call.params.iter().enumerate() {
            let typed = type_runtimes.decode_portable(value)?;
            let encoded =
                encode_public_statement_value(&typed, encoding_runtimes, tuple_encoding_defaults)?;
            blocks.push(tx_param_block(tx_index as u32, param_index, &encoded));
        }
    }
    Ok(blocks)
}

/// Compute the canonical applied transaction digest for one batch input.
pub fn compute_applied_tx_digest(
    batch: &ir::EntryBatch,
    type_runtimes: &TypeRuntimeRegistry,
    encoding_runtimes: &EncodingRuntimeRegistry,
    tuple_encoding_defaults: &TupleEncodingDefaults,
) -> Result<Digest, TabulaError> {
    Ok(digest_from_blocks(&canonical_batch_payload(
        batch,
        type_runtimes,
        encoding_runtimes,
        tuple_encoding_defaults,
    )?))
}

/// Canonical field-block payload for the emitted-event commitment.
pub fn canonical_event_log_payload(
    events: &[ProofEventEffect],
    encoding_runtimes: &EncodingRuntimeRegistry,
    tuple_encoding_defaults: &TupleEncodingDefaults,
) -> Result<Vec<[KoalaBear; 8]>, TabulaError> {
    let mut blocks = Vec::new();
    blocks.push(event_transcript_header_block(events.len()));
    for event in events {
        blocks.push(event_header_block(
            event.tx_index,
            event.effect.op_index,
            event.effect.effect_ordinal_in_entry,
            event.effect.event,
            event.effect.args.len(),
        ));
        for (arg_index, value) in event.effect.args.iter().enumerate() {
            let encoded =
                encode_public_statement_value(value, encoding_runtimes, tuple_encoding_defaults)?;
            blocks.push(event_arg_block(
                event.tx_index,
                event.effect.effect_ordinal_in_entry,
                arg_index,
                &encoded,
            ));
        }
    }
    Ok(blocks)
}

/// Compute the canonical emitted-event digest for one proof journal.
pub fn compute_event_digest(
    events: &[ProofEventEffect],
    encoding_runtimes: &EncodingRuntimeRegistry,
    tuple_encoding_defaults: &TupleEncodingDefaults,
) -> Result<Digest, TabulaError> {
    Ok(digest_from_blocks(&canonical_event_log_payload(
        events,
        encoding_runtimes,
        tuple_encoding_defaults,
    )?))
}

pub(crate) fn encode_public_statement_value(
    value: &TypedValue,
    encoding_runtimes: &EncodingRuntimeRegistry,
    tuple_encoding_defaults: &TupleEncodingDefaults,
) -> Result<EncodedTranscriptValue, TabulaError> {
    let encoding_profile_id = tuple_encoding_defaults.resolve(value.type_id())?;
    let mut field_elements =
        encoding_runtimes.encode_field_elements_for_profile(encoding_profile_id, value)?;
    if field_elements.len() > 3 {
        return Err(TabulaError::ProofError {
            phase: "public_statement_transcript",
            detail: format!(
                "value type {} encoded width {} exceeds public-statement transcript width 3",
                value.type_id().0,
                field_elements.len()
            ),
        });
    }
    field_elements.resize(3, KoalaBear::ZERO);
    Ok(EncodedTranscriptValue {
        type_id: value.type_id(),
        field_elements: [field_elements[0], field_elements[1], field_elements[2]],
    })
}

fn digest_from_blocks(blocks: &[[KoalaBear; 8]]) -> Digest {
    tabula_contract::format::public_statement_transcript::compute_public_statement_transcript_digest(
        blocks.iter(),
    )
    .to_bytes()
}

#[cfg(test)]
mod tests {
    use borsh::to_vec;
    use tabula_contract::{TupleEncodingDefaults, TupleEncodingSelection};
    use tabula_core::traits::Hasher;
    use tabula_core::{
        ColId, CommittedCellKey, CommittedKey, CommittedPropertyQuery, InMemoryState,
        PortableValue, TableId,
    };
    use tabula_profile::{ENCODING_U64_ID, TYPE_U64_ID};
    use tabula_types::{
        CommittedColumnEntry, EncodingRuntimeRegistry, NativeKeyPayload, TypeRuntimeRegistry,
        TypedValue, u64_typed,
    };

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

        fn hash_pair(&self, left: &Digest, right: &Digest) -> Result<Digest, TabulaError> {
            let mut data = Vec::new();
            data.extend_from_slice(left);
            data.extend_from_slice(right);
            Ok(self.hash(&data))
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

    fn portable_u64(value: u64) -> PortableValue {
        PortableValue::new(TYPE_U64_ID, to_vec(&value).unwrap())
    }

    fn test_encoding_runtimes() -> EncodingRuntimeRegistry {
        EncodingRuntimeRegistry::seeded().expect("seeded encoding runtimes")
    }

    fn test_tuple_encoding_defaults() -> TupleEncodingDefaults {
        TupleEncodingDefaults::new(vec![TupleEncodingSelection {
            type_id: TYPE_U64_ID,
            encoding_profile_id: ENCODING_U64_ID,
        }])
        .expect("tuple encoding defaults")
    }

    #[derive(Default)]
    struct TestStateRuntime;

    impl exec::StateRuntimeView for TestStateRuntime {
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
            _table: ir::TableId,
            key: &[TypedValue],
        ) -> Result<CommittedKey, TabulaError> {
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
            _table: ir::TableId,
            key: &CommittedKey,
        ) -> Result<Vec<TypedValue>, TabulaError> {
            if key.0.len() != std::mem::size_of::<u64>() {
                return Err(TabulaError::InvalidIr(format!(
                    "expected 8 committed key bytes, got {}",
                    key.0.len()
                )));
            }
            Ok(vec![u64_typed(u64::from_le_bytes(
                key.0.clone().try_into().expect("u64 bytes"),
            ))])
        }

        fn encode_key_payload(
            &self,
            _table: ir::TableId,
            key: &CommittedKey,
        ) -> Result<NativeKeyPayload, TabulaError> {
            if key.0.len() != std::mem::size_of::<u64>() {
                return Err(TabulaError::InvalidIr(format!(
                    "expected 8 committed key bytes, got {}",
                    key.0.len()
                )));
            }
            let limbs = tabula_commitment::primitives::encode_u64_limbs(u64::from_le_bytes(
                key.0.clone().try_into().expect("u64 bytes"),
            ));
            let mut payload = tabula_types::zero_key_payload();
            payload[0] = limbs[2];
            payload[1] = limbs[1];
            payload[2] = limbs[0];
            Ok(payload)
        }

        fn compare_keys(
            &self,
            _table: ir::TableId,
            lhs: &CommittedKey,
            rhs: &CommittedKey,
        ) -> Result<std::cmp::Ordering, TabulaError> {
            Ok(lhs.cmp(rhs))
        }

        fn key_component_types(
            &self,
            _table: ir::TableId,
        ) -> Result<Vec<tabula_core::TypeId>, TabulaError> {
            Ok(vec![TYPE_U64_ID])
        }

        fn column_type(
            &self,
            _table: ir::TableId,
            _field: ir::FieldId,
        ) -> Result<tabula_core::TypeId, TabulaError> {
            Ok(TYPE_U64_ID)
        }

        fn resolve_property(
            &self,
            _table: ir::TableId,
            _field: ir::FieldId,
            query: &CommittedPropertyQuery,
            state: &[CommittedColumnEntry],
        ) -> Result<tabula_types::TypedCommittedPropertyQueryResult, TabulaError> {
            let pick = match query {
                CommittedPropertyQuery::Minimum => state
                    .iter()
                    .filter(|entry| !entry.is_null)
                    .min_by(|lhs, rhs| lhs.key.cmp(&rhs.key)),
                CommittedPropertyQuery::Maximum => state
                    .iter()
                    .filter(|entry| !entry.is_null)
                    .max_by(|lhs, rhs| lhs.key.cmp(&rhs.key)),
                CommittedPropertyQuery::Successor { key } => state
                    .iter()
                    .filter(|entry| !entry.is_null)
                    .filter(|entry| entry.key > *key)
                    .min_by(|lhs, rhs| lhs.key.cmp(&rhs.key)),
                CommittedPropertyQuery::Predecessor { key } => state
                    .iter()
                    .filter(|entry| !entry.is_null)
                    .filter(|entry| entry.key < *key)
                    .max_by(|lhs, rhs| lhs.key.cmp(&rhs.key)),
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
            Ok(if let Some(entry) = pick {
                tabula_types::TypedCommittedPropertyQueryResult {
                    value: entry.value.clone(),
                    key: Some(entry.key.clone()),
                    is_null: false,
                }
            } else {
                tabula_types::TypedCommittedPropertyQueryResult {
                    value: u64_typed(0),
                    key: None,
                    is_null: true,
                }
            })
        }
    }

    fn test_state_runtime() -> &'static TestStateRuntime {
        static RUNTIME: std::sync::OnceLock<TestStateRuntime> = std::sync::OnceLock::new();
        RUNTIME.get_or_init(TestStateRuntime::default)
    }

    fn validated_program() -> ir::ValidatedProgram {
        ir::ValidatedProgram::try_from(ir::Program {
            program_id: ir::ProgramId(99),
            state: ir::StateSchema {
                tables: vec![ir::TableSchema {
                    id: ir::TableId(1),
                    symbol: "accounts".into(),
                    keys: vec![tabula_core::KeyComponentSchema {
                        symbol: "id".into(),
                        ty: TYPE_U64_ID,
                    }],
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
        .expect("valid program")
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
    fn runtime_reduction_builds_proof_journal_and_public_statement() {
        let runtimes = TypeRuntimeRegistry::seeded().expect("seeded runtimes");
        let runtime_program =
            RuntimeProgram::from_validated_program(validated_program()).expect("runtime program");
        let mut capabilities = exec::CapabilityRegistry::new();
        capabilities.register(AddOneCapability).unwrap();
        let exec_ctx = exec::ExecContext {
            hasher: &XorHasher,
            type_runtimes: &runtimes,
            capability_executor: Some(&capabilities),
            state_runtime: test_state_runtime(),
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
        let encoding_runtimes = test_encoding_runtimes();
        let tuple_encoding_defaults = test_tuple_encoding_defaults();
        let public_statement = materialize_public_statement(
            runtime_program.proof(),
            &context,
            &journal,
            PublicStatementMaterialization {
                applied_tx_digest: [0x22; 32],
                old_state_root: [0x33; 32],
                new_state_root: [0x44; 32],
            },
            &runtimes,
            &encoding_runtimes,
            &tuple_encoding_defaults,
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
        assert_ne!(public_statement.event_digest.to_bytes(), [0u8; 32]);
        assert_eq!(public_statement.applied_tx_digest.to_bytes(), [0x22; 32]);
        assert_eq!(public_statement.old_root.to_bytes(), [0x33; 32]);
        assert_eq!(public_statement.new_root.to_bytes(), [0x44; 32]);
        assert_eq!(
            public_statement.public_context_digest.to_bytes(),
            compute_public_context_digest(
                &proof_journal.public_context,
                &runtimes,
                &encoding_runtimes,
                &tuple_encoding_defaults,
            )
            .expect("context digest"),
        );
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
            capability_executor: Some(&capabilities),
            state_runtime: test_state_runtime(),
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
                    keys: vec![tabula_core::KeyComponentSchema {
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
                            dst_value: ir::LocalId(0),
                            dst_key_components: vec![ir::LocalId(1)],
                            dst_is_null: ir::LocalId(2),
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
        let exec_ctx = exec::ExecContext {
            hasher: &XorHasher,
            type_runtimes: &runtimes,
            capability_executor: None,
            state_runtime: test_state_runtime(),
        };
        let mut state = InMemoryState::new();
        state.set(
            CommittedCellKey {
                table: TableId(1),
                col: ColId(0),
                key: CommittedKey(1u64.to_le_bytes().to_vec()),
            },
            portable_u64(5),
        );
        state.set(
            CommittedCellKey {
                table: TableId(1),
                col: ColId(0),
                key: CommittedKey(3u64.to_le_bytes().to_vec()),
            },
            portable_u64(8),
        );
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
            proof_journal.state_slots[0].property_effects[0].result,
            tabula_types::TypedCommittedPropertyQueryResult {
                value: u64_typed(8),
                key: Some(CommittedKey(3u64.to_le_bytes().to_vec())),
                is_null: false,
            }
        );
    }

    #[test]
    fn public_statement_materialization_is_deterministic_for_same_journal() {
        let runtimes = TypeRuntimeRegistry::seeded().expect("seeded runtimes");
        let runtime_program =
            RuntimeProgram::from_validated_program(validated_program()).expect("runtime program");
        let mut capabilities = exec::CapabilityRegistry::new();
        capabilities.register(AddOneCapability).unwrap();
        let exec_ctx = exec::ExecContext {
            hasher: &XorHasher,
            type_runtimes: &runtimes,
            capability_executor: Some(&capabilities),
            state_runtime: test_state_runtime(),
        };
        let mut state = InMemoryState::new();
        state.set(
            CommittedCellKey {
                table: TableId(1),
                col: ColId(0),
                key: CommittedKey(2u64.to_le_bytes().to_vec()),
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

        let encoding_runtimes = test_encoding_runtimes();
        let tuple_encoding_defaults = test_tuple_encoding_defaults();
        let first = materialize_public_statement(
            runtime_program.proof(),
            &context,
            &journal,
            PublicStatementMaterialization {
                applied_tx_digest: [1; 32],
                old_state_root: [2; 32],
                new_state_root: [3; 32],
            },
            &runtimes,
            &encoding_runtimes,
            &tuple_encoding_defaults,
        )
        .expect("public statement");
        let second = materialize_public_statement(
            runtime_program.proof(),
            &context,
            &journal,
            PublicStatementMaterialization {
                applied_tx_digest: [1; 32],
                old_state_root: [2; 32],
                new_state_root: [3; 32],
            },
            &runtimes,
            &encoding_runtimes,
            &tuple_encoding_defaults,
        )
        .expect("public statement");

        assert_eq!(first, second);
    }
}
