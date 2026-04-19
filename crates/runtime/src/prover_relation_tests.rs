//! Integration tests for the relation-proof path.
//!
//! Included into [`crate::prover`] via `#[path]` so tests retain
//! `pub(crate)` access to `PreparedProver` fields without adding
//! accessors that are not needed outside of tests.

use super::*;
use crate::host::HostEnvironment;
use crate::verifier::relation_table_root_from_proof;
use crate::{PreparedExecutor, prepare_executor};
use tabula_core::error::TabulaError;

use std::cmp::Ordering;
use std::sync::Arc;

use p3_field::PrimeCharacteristicRing;
use p3_koala_bear::KoalaBear;
use std::collections::{BTreeMap, BTreeSet};
use tabula_chips::event_transcript::EVENT_TRANSCRIPT_WITNESS_LABEL;
use tabula_chips::execution::EXECUTION_STANDARD_VALUE_WIDTH;
use tabula_chips::execution::trace::{InstructionRecord, Opcode};
use tabula_chips::relation_table::RELATION_TABLE_CHIP_ID;
use tabula_chips::relation_table::{RELATION_TABLE_WITNESS_LABEL, RelationTableWitnessRow};
use tabula_chips::relation_transcript::{
    RELATION_TRANSCRIPT_WITNESS_LABEL, RelationTranscriptCall,
};
use tabula_commitment::PoseidonHasher;
use tabula_contract::format::typed_tuple::{TypedTupleRole, compute_typed_tuple_digest};
use tabula_core::{EncodingProfileId, PortableValue, TypeId};
use tabula_ext::root::{
    RootBackend, RootBackendBundle, RootWitnessPreparer, SmtRootWitnessPreparer,
};
use tabula_machine::{BackendProver, RootProofBackend, SmtRootProofBackend};
use tabula_profile::{
    CanonicalNullEncoding, EncodingClass, EncodingProfile, FieldFamily, GenericIrFamily,
    HostValueFamily, NullSemantics, TranscriptSerialization, TypeCapabilities, TypeDescriptor,
    ZeroValueSpec,
};
use tabula_stark::trace::witness_labels;
use tabula_testing::exec::{
    context_input, register_program_from_source, register_program_from_source_with_catalogs,
    tx_batch,
};
use tabula_types::{
    ArithmeticOp, EncodingRuntime, TypeRuntime, TypedValue, bool_portable, u64_portable, u64_typed,
};
use tabula_witness::stark::{LowerSuccessfulTxInput, lower_successful_tx};
use tabula_witness::{RelationClaim, RelationClaimKind, prepare_relation_proof};

const TEST_EXTRA_TYPE_ID: TypeId = TypeId(90_001);
const TEST_EXTRA_ENCODING_ID: EncodingProfileId = EncodingProfileId(90_001);

fn relation_source() -> &'static str {
    r#"
program RelationProof

context {
  caller: u64;
  epoch: u64;
}

state {
  table accounts(key id: u64) {
    tier: u64 @ssmc;
  }
}

relation AllowedTier(tier: u64) = enum { 0, 1, 2, 3 };
relation ValidEpoch(epoch: u64) = range(10, 13);
relation PreferredCaller(actor: u64) = set { 7, 8 };
relation PromoteTier(tier: u64) -> promoted: u64 = map {
  0 => 1,
  1 => 2,
  2 => 3,
  3 => 3,
};

tx enroll(flag: bool, id: u64, tier: u64) {
  assert relation AllowedTier(tier);
  assert relation ValidEpoch(epoch);
  if flag {
    let promoted = eval relation PromoteTier(tier);
    accounts[id].tier = promoted;
  } else {
    assert relation PreferredCaller(caller);
  }
  return;
}
"#
}

fn event_debug_source() -> &'static str {
    r#"
program EventTranscriptDebug

context {
  caller: u64;
}

event Registered(id: u64, actor: u64);

tx register(id: u64) {
  emit Registered(id, caller);
  return;
}
"#
}

fn extract_event_items(records: &[InstructionRecord]) -> Vec<(u32, [KoalaBear; 8])> {
    let mut items = records
        .iter()
        .filter_map(|record| match record.opcode {
            Opcode::EmitEventHeader => Some((
                record.proof_meta0.expect("event header item index"),
                [
                    KoalaBear::ONE,
                    KoalaBear::new(record.tx_index),
                    KoalaBear::new(
                        record
                            .instruction_index
                            .expect("event header instruction index"),
                    ),
                    KoalaBear::new(record.proof_meta1.expect("event header ordinal")),
                    KoalaBear::new(record.proof_meta2.expect("event header id")),
                    KoalaBear::new(record.proof_meta3.expect("event header arg count")),
                    KoalaBear::ZERO,
                    KoalaBear::ZERO,
                ],
            )),
            Opcode::EmitEventArg => Some((
                record.proof_meta0.expect("event arg item index"),
                [
                    KoalaBear::new(2),
                    KoalaBear::new(record.tx_index),
                    KoalaBear::new(record.proof_meta1.expect("event arg ordinal")),
                    KoalaBear::new(record.proof_meta2.expect("event arg index")),
                    KoalaBear::new(record.proof_meta3.expect("event arg type id")),
                    *record.src1_val.first().expect("event arg limb 0"),
                    *record.src1_val.get(1).expect("event arg limb 1"),
                    *record.src1_val.get(2).expect("event arg limb 2"),
                ],
            )),
            _ => None,
        })
        .collect::<Vec<_>>();
    items.sort_unstable_by_key(|(item_index, _)| *item_index);
    items
}

fn guarded_relation_source() -> &'static str {
    r#"
program GuardedRelation

context {
  caller: u64;
}

state {
  table accounts(key id: u64) {
    tier: u64 @ssmc;
  }
}

relation PromoteTier(tier: u64) -> promoted: u64 = map {
  1 => 2,
  2 => 3,
  3 => 3,
};

tx maybe_promote(flag: bool, id: u64, tier: u64) {
  if flag {
    let promoted = eval relation PromoteTier(tier);
    accounts[id].tier = promoted;
  } else {
    assert true;
  }
  return;
}
"#
}

fn capability_source() -> &'static str {
    r#"
use capability demo_hash;

program DeferredCapability

tx scan(id: u64) {
  let digest = demo_hash(id);
  assert true;
  return;
}
"#
}

fn relation_context(caller: u64, epoch: u64) -> ir::ContextInput {
    context_input([
        (ir::ContextFieldId(0), u64_portable(caller)),
        (ir::ContextFieldId(1), u64_portable(epoch)),
    ])
}

fn guarded_context(caller: u64) -> ir::ContextInput {
    context_input([(ir::ContextFieldId(0), u64_portable(caller))])
}

fn relation_snapshot(registered: &RegisteredProgram) -> CommittedStateSnapshot {
    let opts = crate::PreparedOptions::try_standard().expect("standard options");
    let executor = prepare_executor(Arc::new(registered.clone()), &opts).expect("build executor");
    executor
        .materialize_logical_state([
            (
                ir::TableId(0),
                vec![u64_portable(0)],
                ir::FieldId(0),
                u64_portable(0),
            ),
            (
                ir::TableId(0),
                vec![u64_portable(1)],
                ir::FieldId(0),
                u64_portable(0),
            ),
        ])
        .expect("build relation snapshot")
}

fn executor_and_prover_for_source(
    source: &str,
) -> (RegisteredProgram, PreparedExecutor, crate::PreparedProver) {
    let registered = register_program_from_source(source);
    let opts = crate::PreparedOptions::try_standard().expect("standard options");
    let executor = prepare_executor(Arc::new(registered.clone()), &opts).expect("build executor");
    let prover =
        crate::prepare_prover(Arc::new(registered.clone()), &opts).expect("build prepared prover");
    (registered, executor, prover)
}

#[derive(Debug)]
struct EmptyFamilyRootProofBackend;

impl RootProofBackend for EmptyFamilyRootProofBackend {
    fn name(&self) -> &str {
        "empty_family_root_proof"
    }

    fn supported_root_binding_families(&self) -> &'static [tabula_core::RootProfileId] {
        &[]
    }

    fn airs(&self) -> Vec<Box<dyn tabula_machine::backend::AnyRap>> {
        SmtRootProofBackend.airs()
    }

    fn dyn_chips(&self) -> Vec<Box<dyn tabula_stark::trace::DynChip>> {
        SmtRootProofBackend.dyn_chips()
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct EmptyFamilyRootBackend;

impl RootBackend for EmptyFamilyRootBackend {
    fn name(&self) -> &str {
        "empty_family_root"
    }

    fn proof_backend(&self) -> Arc<dyn RootProofBackend> {
        Arc::new(EmptyFamilyRootProofBackend)
    }

    fn witness_preparer(&self) -> Arc<dyn RootWitnessPreparer> {
        Arc::new(SmtRootWitnessPreparer)
    }
}

#[test]
fn committed_snapshot_decode_rejects_duplicate_cells() {
    let (_registered, executor, _prover) = executor_and_prover_for_source(relation_source());
    let error = executor
        .decode_committed_snapshot([
            (
                ir::TableId(0),
                0u64.to_le_bytes().to_vec(),
                ir::FieldId(0),
                u64_portable(1),
            ),
            (
                ir::TableId(0),
                0u64.to_le_bytes().to_vec(),
                ir::FieldId(0),
                u64_portable(2),
            ),
        ])
        .expect_err("duplicate committed cells must fail");

    assert!(
        error
            .to_string()
            .contains("duplicate committed cell 0.0 key"),
        "unexpected error: {error}"
    );
}

#[test]
fn logical_state_materialization_rejects_duplicate_cells() {
    let (_registered, executor, _prover) = executor_and_prover_for_source(relation_source());
    let error = executor
        .materialize_logical_state([
            (
                ir::TableId(0),
                vec![u64_portable(0)],
                ir::FieldId(0),
                u64_portable(1),
            ),
            (
                ir::TableId(0),
                vec![u64_portable(0)],
                ir::FieldId(0),
                u64_portable(2),
            ),
        ])
        .expect_err("duplicate logical cells must fail");

    assert!(
        error
            .to_string()
            .contains("duplicate logical state cell 0.0 key"),
        "unexpected error: {error}"
    );
}

fn entry_id_for(executor: &PreparedExecutor, symbol: &str) -> ir::EntryId {
    executor
        .entry_id_by_symbol(symbol)
        .unwrap_or_else(|| panic!("missing entry '{symbol}'"))
}

fn prove_input<'a>(
    snapshot: &'a CommittedStateSnapshot,
    batch: &'a ir::EntryBatch,
    context: &'a ir::ContextInput,
    executed: &'a exec::ExecutionJournal,
) -> ProveInput<'a> {
    ProveInput {
        snapshot,
        batch,
        context,
        executed,
    }
}

#[derive(Clone)]
struct ExtraTypeRuntime {
    descriptor: TypeDescriptor,
}

impl ExtraTypeRuntime {
    fn new() -> Self {
        let descriptor = TypeDescriptor::new(
            TEST_EXTRA_TYPE_ID,
            "test-extra-u64",
            Some("extra runtime used only to prove host overrides do not affect static relation roots".to_string()),
            HostValueFamily::UnsignedInt { bits: 64 },
            GenericIrFamily::UnsignedInteger,
            TypeCapabilities {
                equality: true,
                ordering: true,
                arithmetic: true,
            },
            ZeroValueSpec::IntegerZero,
            NullSemantics::NullableWithCanonicalZero,
        )
        .expect("build extra type descriptor");
        Self { descriptor }
    }
}

impl TypeRuntime for ExtraTypeRuntime {
    fn type_id(&self) -> TypeId {
        self.descriptor.type_id
    }

    fn descriptor(&self) -> &TypeDescriptor {
        &self.descriptor
    }

    fn zero_typed(&self) -> TypedValue {
        TypedValue::new(self.type_id(), 0u64.to_le_bytes().to_vec())
    }

    fn encode_portable(&self, value: &TypedValue) -> Result<PortableValue, TabulaError> {
        Ok(value.clone().into_portable())
    }

    fn decode_portable(&self, value: &PortableValue) -> Result<TypedValue, TabulaError> {
        Ok(TypedValue::new(value.type_id(), value.payload().to_vec()))
    }

    fn validate(&self, value: &TypedValue) -> Result<(), TabulaError> {
        if value.type_id() != self.type_id() {
            return Err(TabulaError::Custom(
                "unexpected type id for extra runtime".to_string(),
            ));
        }
        Ok(())
    }

    fn eq_value(&self, lhs: &TypedValue, rhs: &TypedValue) -> Result<bool, TabulaError> {
        self.validate(lhs)?;
        self.validate(rhs)?;
        Ok(lhs.payload() == rhs.payload())
    }

    fn cmp_value(&self, lhs: &TypedValue, rhs: &TypedValue) -> Result<Ordering, TabulaError> {
        self.validate(lhs)?;
        self.validate(rhs)?;
        Ok(lhs.payload().cmp(rhs.payload()))
    }

    fn apply_arithmetic(
        &self,
        _op: ArithmeticOp,
        _lhs: &TypedValue,
        _rhs: &TypedValue,
    ) -> Result<TypedValue, TabulaError> {
        Err(TabulaError::Custom(
            "extra runtime arithmetic is not used in this test".to_string(),
        ))
    }

    fn divmod(
        &self,
        _lhs: &TypedValue,
        _rhs: &TypedValue,
    ) -> Result<(TypedValue, TypedValue), TabulaError> {
        Err(TabulaError::Custom(
            "extra runtime divmod is not used in this test".to_string(),
        ))
    }

    fn debug_display(&self, value: &TypedValue) -> Result<String, TabulaError> {
        self.validate(value)?;
        Ok(format!("extra({:?})", value.payload()))
    }
}

#[derive(Clone)]
struct ExtraEncodingRuntime {
    descriptor: EncodingProfile,
}

impl ExtraEncodingRuntime {
    fn new(type_descriptor: &TypeDescriptor) -> Self {
        let descriptor = EncodingProfile::new(
            TEST_EXTRA_ENCODING_ID,
            "test-extra-u64-encoding",
            Some("extra encoding used only to prove host overrides do not affect static relation roots".to_string()),
            type_descriptor,
            EncodingClass::FieldElementArray,
            FieldFamily::KoalaBear31,
            2,
            Some(8),
            CanonicalNullEncoding::SeparateNullFlagWithZeroValue,
            TranscriptSerialization::FieldElementsWithNullFlag,
            true,
            true,
        )
        .expect("build extra encoding profile");
        Self { descriptor }
    }
}

impl EncodingRuntime for ExtraEncodingRuntime {
    fn encoding_profile_id(&self) -> EncodingProfileId {
        self.descriptor.encoding_profile_id
    }

    fn descriptor(&self) -> &EncodingProfile {
        &self.descriptor
    }

    fn encode_field_elements(&self, value: &TypedValue) -> Result<Vec<KoalaBear>, TabulaError> {
        if value.type_id() != self.descriptor.type_id {
            return Err(TabulaError::Custom(
                "unexpected type id for extra encoding runtime".to_string(),
            ));
        }
        Ok(vec![KoalaBear::ZERO, KoalaBear::ZERO])
    }

    fn decode_field_elements(
        &self,
        _field_elements: &[KoalaBear],
    ) -> Result<TypedValue, TabulaError> {
        Ok(TypedValue::new(
            self.descriptor.type_id,
            0u64.to_le_bytes().to_vec(),
        ))
    }

    fn encode_transcript_atoms(&self, value: &TypedValue) -> Result<Vec<KoalaBear>, TabulaError> {
        self.encode_field_elements(value)
    }

    fn trace_width(&self) -> usize {
        self.descriptor.width as usize
    }
}

#[test]
fn relation_table_rows_reject_claims_missing_from_manifest() {
    let (registered, executor, _prover) = executor_and_prover_for_source(relation_source());
    let error = prepare_relation_proof(
        executor.state.semantic.execution().program(),
        registered.static_table_artifact(),
        &[RelationClaim {
            relation: ir::RelationId(0),
            kind: RelationClaimKind::Assert,
            inputs: vec![u64_typed(9)],
            input_digest: [9; 8],
            outputs: vec![],
            output_digest: [0; 8],
            tx_index: 0,
            effect_ordinal_in_tx: 0,
            op_index: 0,
        }],
    )
    .expect_err("manifest mismatch must fail");

    assert!(
        error
            .to_string()
            .contains("was not present in the sealed manifest"),
        "unexpected error: {error}"
    );
}

#[test]
fn lowering_rejects_duplicate_relation_effect_origins() {
    let (_registered, executor, _prover) = executor_and_prover_for_source(relation_source());
    let enroll = entry_id_for(&executor, "enroll");
    let batch = tx_batch(vec![ir::EntryCall {
        entry_id: enroll,
        params: vec![bool_portable(true), u64_portable(0), u64_portable(2)],
    }]);
    let context = relation_context(7, 11);
    let snapshot = executor.empty_state_snapshot();
    let executed = executor
        .execute_batch(&snapshot, &batch, &context)
        .expect("execute batch");
    let tx = executed
        .successful_txs()
        .next()
        .expect("successful tx")
        .clone();

    let mut duplicated_effects = tx.relation_effects.clone();
    duplicated_effects.push(
        tx.relation_effects
            .first()
            .expect("relation effect")
            .clone(),
    );

    let state = &*executor.state;
    let typed_context =
        crate::prelude::decode_context_input_on_state(state, &context).expect("typed context");
    let typed_txs =
        crate::prelude::decode_entry_batch_on_state(state, &batch).expect("typed batch");
    let entry = state
        .semantic
        .execution()
        .entry_definition(enroll)
        .expect("resolved entry");
    let context_slots = Vec::new();
    let param_slots = Vec::new();
    let event_item_bases = BTreeMap::new();

    let mut kit_scratch = tabula_stark::witness_kit::KitScratch::new();
    let error = lower_successful_tx::<EXECUTION_STANDARD_VALUE_WIDTH>(
        LowerSuccessfulTxInput {
            tx_index: tx.tx_index,
            program: state.semantic.execution().program(),
            call: &typed_txs[0],
            entry,
            context: &typed_context,
            state_effects: &tx.state_effects,
            event_effects: &tx.event_effects,
            property_effects: &tx.property_effects,
            relation_effects: &duplicated_effects,
            empty_columns: &BTreeSet::new(),
            type_runtimes: executor.type_runtimes(),
            encoding_runtimes: executor.encoding_runtimes(),
            tuple_encoding_defaults: &state.tuple_encoding_defaults,
            hasher: &PoseidonHasher::new(),
            state_runtime: &state.state,
            context_slots: &context_slots,
            param_slots: &param_slots,
            aux_slot_limit: tabula_chips::execution::MAX_SLOTS,
            event_item_bases: &event_item_bases,
        },
        &mut kit_scratch,
    )
    .expect_err("duplicate relation effects must fail");

    assert!(
        error.to_string().contains("duplicate relation effect"),
        "unexpected error: {error}"
    );
}

#[test]
fn untaken_relation_branches_emit_no_relation_claims_or_positive_lookup_counts() {
    let (registered, executor, prover) = executor_and_prover_for_source(guarded_relation_source());
    let batch = tx_batch(vec![ir::EntryCall {
        entry_id: entry_id_for(&executor, "maybe_promote"),
        params: vec![bool_portable(false), u64_portable(0), u64_portable(2)],
    }]);
    let context = guarded_context(7);
    let snapshot = relation_snapshot(&registered);
    let executed = executor
        .execute_batch(&snapshot, &batch, &context)
        .expect("execute guarded batch");

    let (machine_input, _public_statement) = crate::proof_artifacts::prepare_proof_machine_input(
        &prover.runtime_program,
        &prover.root_backend_bundle,
        &prover.kit_registry,
        &prove_input(&snapshot, &batch, &context, &executed),
    )
    .expect("prepare proof request");

    let transcript_calls = machine_input
        .execution
        .store
        .get::<Vec<RelationTranscriptCall>>(RELATION_TRANSCRIPT_WITNESS_LABEL)
        .expect("relation transcript calls");
    let lookup_rows = machine_input
        .execution
        .store
        .get::<Vec<RelationTableWitnessRow>>(RELATION_TABLE_WITNESS_LABEL)
        .expect("relation lookup rows");

    assert!(transcript_calls.is_empty());
    assert!(
        lookup_rows.iter().all(|row| row.lookup_mult == 0),
        "untaken branches must not contribute positive relation lookup multiplicities",
    );
}

#[test]
fn tampering_relation_table_rows_breaks_proving() {
    let (registered, executor, prover) = executor_and_prover_for_source(relation_source());
    let batch = tx_batch(vec![ir::EntryCall {
        entry_id: entry_id_for(&executor, "enroll"),
        params: vec![bool_portable(true), u64_portable(0), u64_portable(2)],
    }]);
    let context = relation_context(7, 11);
    let snapshot = relation_snapshot(&registered);
    let executed = executor
        .execute_batch(&snapshot, &batch, &context)
        .expect("execute batch");

    let (mut machine_input, _public_statement) =
        crate::proof_artifacts::prepare_proof_machine_input(
            &prover.runtime_program,
            &prover.root_backend_bundle,
            &prover.kit_registry,
            &prove_input(&snapshot, &batch, &context, &executed),
        )
        .expect("prepare proof request");

    let mut rows = machine_input
        .execution
        .store
        .get::<Vec<RelationTableWitnessRow>>(RELATION_TABLE_WITNESS_LABEL)
        .expect("relation lookup rows")
        .clone();
    assert!(!rows.is_empty(), "expected relation lookup rows");
    let tampered = rows
        .iter_mut()
        .find(|row| row.lookup_mult > 0)
        .expect("expected at least one consumed relation lookup row");
    tampered.output_digest[0] = tampered.output_digest[0].wrapping_add(1);
    machine_input
        .execution
        .store
        .put(RELATION_TABLE_WITNESS_LABEL, rows);

    assert!(
        BackendProver::new(prover.machine())
            .prove_envelope(machine_input)
            .is_err(),
        "tampered relation lookup rows must fail proving"
    );
}

#[test]
fn tampering_execution_bound_relation_outputs_breaks_proving() {
    let (registered, executor, prover) = executor_and_prover_for_source(relation_source());
    let batch = tx_batch(vec![ir::EntryCall {
        entry_id: entry_id_for(&executor, "enroll"),
        params: vec![bool_portable(true), u64_portable(0), u64_portable(2)],
    }]);
    let context = relation_context(7, 11);
    let snapshot = relation_snapshot(&registered);
    let executed = executor
        .execute_batch(&snapshot, &batch, &context)
        .expect("execute batch");

    let (mut machine_input, _public_statement) =
        crate::proof_artifacts::prepare_proof_machine_input(
            &prover.runtime_program,
            &prover.root_backend_bundle,
            &prover.kit_registry,
            &prove_input(&snapshot, &batch, &context, &executed),
        )
        .expect("prepare proof request");

    let mut records = machine_input
        .execution
        .store
        .get::<Vec<InstructionRecord>>(witness_labels::EXECUTION_RECORDS)
        .expect("execution records")
        .clone();
    let eval_record = records
        .iter_mut()
        .find(|record| record.opcode == Opcode::RelationProof && record.relation_is_eval)
        .expect("relation eval execution record");
    eval_record.relation_output_vals[0][0] += KoalaBear::ONE;

    machine_input
        .execution
        .store
        .put(witness_labels::EXECUTION_RECORDS, records);

    assert!(
        BackendProver::new(prover.machine())
            .prove_envelope(machine_input)
            .is_err(),
        "tampered relation output binding must fail proving"
    );
}

#[test]
fn tampering_relation_effect_identity_breaks_proving() {
    let (registered, executor, prover) = executor_and_prover_for_source(relation_source());
    let batch = tx_batch(vec![ir::EntryCall {
        entry_id: entry_id_for(&executor, "enroll"),
        params: vec![bool_portable(true), u64_portable(0), u64_portable(2)],
    }]);
    let context = relation_context(7, 11);
    let snapshot = relation_snapshot(&registered);
    let executed = executor
        .execute_batch(&snapshot, &batch, &context)
        .expect("execute batch");

    let (mut machine_input, _public_statement) =
        crate::proof_artifacts::prepare_proof_machine_input(
            &prover.runtime_program,
            &prover.root_backend_bundle,
            &prover.kit_registry,
            &prove_input(&snapshot, &batch, &context, &executed),
        )
        .expect("prepare proof request");

    let mut calls = machine_input
        .execution
        .store
        .get::<Vec<RelationTranscriptCall>>(RELATION_TRANSCRIPT_WITNESS_LABEL)
        .expect("relation transcript calls")
        .clone();
    assert!(
        calls.len() >= 4,
        "expected multiple relation transcript calls"
    );
    calls[2].effect_ordinal_in_tx = calls[0].effect_ordinal_in_tx;
    machine_input
        .execution
        .store
        .put(RELATION_TRANSCRIPT_WITNESS_LABEL, calls);

    assert!(
        BackendProver::new(prover.machine())
            .prove_envelope(machine_input)
            .is_err(),
        "tampered relation effect identity must fail proving"
    );
}

#[test]
fn relation_table_rows_use_empty_output_digest_for_enum_relations() {
    let (registered, executor, _prover) = executor_and_prover_for_source(relation_source());
    let empty_digest = compute_typed_tuple_digest(TypedTupleRole::RelationOutput, &[])
        .expect("empty tuple digest");
    let allowed_rows = registered
        .static_table_artifact()
        .rows
        .iter()
        .filter(|row| row.relation_id == 0)
        .collect::<Vec<_>>();
    assert_eq!(allowed_rows.len(), 4);
    assert!(
        allowed_rows
            .iter()
            .all(|row| row.output_digest == empty_digest)
    );

    let chosen = allowed_rows[2];
    let proof_rows = prepare_relation_proof(
        executor.state.semantic.execution().program(),
        registered.static_table_artifact(),
        &[RelationClaim {
            relation: ir::RelationId(0),
            kind: RelationClaimKind::Assert,
            inputs: vec![u64_typed(2)],
            input_digest: chosen.input_digest,
            outputs: vec![],
            output_digest: chosen.output_digest,
            tx_index: 0,
            effect_ordinal_in_tx: 0,
            op_index: 0,
        }],
    )
    .expect("prepare relation proof rows");
    let rows = proof_rows
        .table_rows()
        .iter()
        .filter(|row| row.relation_id == 0)
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), 4);
    assert!(rows.iter().all(|row| row.output_digest == empty_digest));
    assert_eq!(rows.iter().map(|row| row.lookup_mult).sum::<u32>(), 1);
}

#[test]
fn relation_proof_root_matches_registered_artifact_and_chip_public_values() {
    let (registered, executor, prover) = executor_and_prover_for_source(relation_source());
    let verifier = {
        let opts = crate::PreparedOptions::try_standard().expect("standard options");
        crate::prepare_verifier(Arc::new(registered.sealed().clone()), &opts)
            .expect("build verifier")
    };
    let batch = tx_batch(vec![ir::EntryCall {
        entry_id: entry_id_for(&executor, "enroll"),
        params: vec![bool_portable(true), u64_portable(0), u64_portable(2)],
    }]);
    let context = relation_context(7, 11);
    let snapshot = relation_snapshot(&registered);
    let executed = executor
        .execute_batch(&snapshot, &batch, &context)
        .expect("execute batch");
    let proved = prover
        .prove_and_verify(
            &verifier,
            &ProveInput {
                snapshot: &snapshot,
                batch: &batch,
                context: &context,
                executed: &executed,
            },
        )
        .expect("prove relation batch");
    let chip_root = relation_table_root_from_proof(&proved.proof, prover.machine())
        .expect("extract relation chip root");

    assert_eq!(
        prover.runtime_program.static_table_artifact.root,
        registered.static_table_artifact().root
    );
    assert_eq!(
        chip_root,
        Some(registered.static_table_artifact().root),
        "relation table chip root must match the registered artifact root",
    );
    assert_eq!(
        runtime_ir::compute_applied_tx_digest(
            &batch,
            prover.type_runtimes(),
            prover.encoding_runtimes(),
            &prover.runtime_program.tuple_encoding_defaults,
        )
        .expect("batch digest"),
        proved.public_statement.applied_tx_digest.to_bytes()
    );
    assert_eq!(
        executed.successful_txs().count(),
        1,
        "sanity-check proof came from the expected execution batch",
    );
}

#[test]
fn relation_chip_public_values_truncation_fails_verification() {
    let (registered, executor, prover) = executor_and_prover_for_source(relation_source());
    let snapshot = relation_snapshot(&registered);
    let verifier = {
        let opts = crate::PreparedOptions::try_standard().expect("standard options");
        crate::prepare_verifier(Arc::new(registered.sealed().clone()), &opts)
            .expect("build verifier")
    };
    let batch = tx_batch(vec![ir::EntryCall {
        entry_id: entry_id_for(&executor, "enroll"),
        params: vec![bool_portable(true), u64_portable(0), u64_portable(2)],
    }]);
    let context = relation_context(7, 11);
    let executed = executor
        .execute_batch(&snapshot, &batch, &context)
        .expect("execute batch");
    let mut proved = prover
        .prove(&ProveInput {
            snapshot: &snapshot,
            batch: &batch,
            context: &context,
            executed: &executed,
        })
        .expect("prove relation batch");
    let relation_opening = proved
        .proof
        .execution
        .chip_openings
        .iter_mut()
        .find(|opening| opening.chip_id == RELATION_TABLE_CHIP_ID)
        .expect("relation chip opening");
    relation_opening.public_values.pop();

    let verifier_err = verifier
        .verify(&proved.proof, &proved.public_statement)
        .expect_err("truncated relation chip public values must fail verifier validation");
    assert!(
        verifier_err
            .to_string()
            .contains("machine metadata requires 8"),
        "unexpected verifier error: {verifier_err}"
    );
}

#[test]
fn relation_chip_public_values_append_fails_verification() {
    let (registered, executor, prover) = executor_and_prover_for_source(relation_source());
    let snapshot = relation_snapshot(&registered);
    let verifier = {
        let opts = crate::PreparedOptions::try_standard().expect("standard options");
        crate::prepare_verifier(Arc::new(registered.sealed().clone()), &opts)
            .expect("build verifier")
    };
    let batch = tx_batch(vec![ir::EntryCall {
        entry_id: entry_id_for(&executor, "enroll"),
        params: vec![bool_portable(true), u64_portable(0), u64_portable(2)],
    }]);
    let context = relation_context(7, 11);
    let executed = executor
        .execute_batch(&snapshot, &batch, &context)
        .expect("execute batch");
    let mut proved = prover
        .prove(&ProveInput {
            snapshot: &snapshot,
            batch: &batch,
            context: &context,
            executed: &executed,
        })
        .expect("prove relation batch");
    let relation_opening = proved
        .proof
        .execution
        .chip_openings
        .iter_mut()
        .find(|opening| opening.chip_id == RELATION_TABLE_CHIP_ID)
        .expect("relation chip opening");
    relation_opening.public_values.push(KoalaBear::ZERO);

    let verifier_err = verifier
        .verify(&proved.proof, &proved.public_statement)
        .expect_err("extended relation chip public values must fail verifier validation");
    assert!(
        verifier_err
            .to_string()
            .contains("machine metadata requires 8"),
        "unexpected verifier error: {verifier_err}"
    );
}

#[test]
fn missing_relation_chip_opening_still_fails_verification() {
    let (registered, executor, prover) = executor_and_prover_for_source(relation_source());
    let snapshot = relation_snapshot(&registered);
    let verifier = {
        let opts = crate::PreparedOptions::try_standard().expect("standard options");
        crate::prepare_verifier(Arc::new(registered.sealed().clone()), &opts)
            .expect("build verifier")
    };
    let batch = tx_batch(vec![ir::EntryCall {
        entry_id: entry_id_for(&executor, "enroll"),
        params: vec![bool_portable(true), u64_portable(0), u64_portable(2)],
    }]);
    let context = relation_context(7, 11);
    let executed = executor
        .execute_batch(&snapshot, &batch, &context)
        .expect("execute batch");
    let mut proved = prover
        .prove(&ProveInput {
            snapshot: &snapshot,
            batch: &batch,
            context: &context,
            executed: &executed,
        })
        .expect("prove relation batch");
    proved
        .proof
        .execution
        .chip_openings
        .retain(|opening| opening.chip_id != RELATION_TABLE_CHIP_ID);

    let verifier_err = verifier
        .verify(&proved.proof, &proved.public_statement)
        .expect_err("missing relation chip opening must fail verifier validation");
    assert!(
        verifier_err
            .to_string()
            .contains("relation table chip opening is missing"),
        "unexpected verifier error: {verifier_err}"
    );
}

#[test]
fn bundled_root_authority_rejects_unsupported_binding_families() {
    let registered = register_program_from_source(relation_source());
    let opts = crate::PreparedOptions::try_standard()
        .expect("standard options")
        .with_root_backend(crate::RootBackend::from_bundle(RootBackendBundle::new(
            EmptyFamilyRootBackend,
        )));
    let err = crate::prepare_prover(Arc::new(registered.clone()), &opts)
        .expect_err("prover build must reject unsupported bundled root families");
    assert!(
        err.to_string()
            .contains("bundled root authority does not support binding family"),
        "unexpected prover build error: {err}"
    );

    let err = crate::prepare_verifier(Arc::new(registered.sealed().clone()), &opts)
        .expect_err("verifier build must reject unsupported bundled root families");
    assert!(
        err.to_string()
            .contains("bundled root authority does not support binding family"),
        "unexpected verifier build error: {err}"
    );
}

#[test]
fn event_transcript_witness_matches_execution_event_rows() {
    let registered = register_program_from_source(event_debug_source());
    let opts = crate::PreparedOptions::try_standard().expect("standard options");
    let executor = prepare_executor(Arc::new(registered.clone()), &opts).expect("build executor");
    let prover = crate::prepare_prover(Arc::new(registered), &opts).expect("build prover");
    let snapshot = executor.empty_state_snapshot();
    let register = executor
        .entry_id_by_symbol("register")
        .expect("register entry");
    let batch = tx_batch(vec![ir::EntryCall {
        entry_id: register,
        params: vec![u64_portable(1)],
    }]);
    let context = context_input([(ir::ContextFieldId(0), u64_portable(7))]);
    let executed = executor
        .execute_batch(&snapshot, &batch, &context)
        .expect("execute event batch");
    let state = &*executor.state;
    let typed_context =
        crate::prelude::decode_context_input_on_state(state, &context).expect("decode context");
    let typed_txs =
        crate::prelude::decode_entry_batch_on_state(state, &batch).expect("decode batch");

    let prepared = crate::proof_artifacts::prepare_proof_artifacts(
        &prover.runtime_program,
        &prover.root_backend_bundle,
        &prover.kit_registry,
        &snapshot,
        &typed_txs,
        &typed_context,
        &executed,
    )
    .expect("prepare proof artifacts");

    let records = prepared
        .execution
        .store
        .get::<Vec<InstructionRecord>>(witness_labels::EXECUTION_RECORDS)
        .expect("execution records");
    let transcript_items = prepared
        .execution
        .store
        .get::<Vec<[KoalaBear; 8]>>(EVENT_TRANSCRIPT_WITNESS_LABEL)
        .expect("event transcript items");

    let execution_items = extract_event_items(records);
    let witness_items = transcript_items
        .iter()
        .copied()
        .enumerate()
        .map(|(index, block)| (index as u32, block))
        .collect::<Vec<_>>();

    assert_eq!(execution_items, witness_items);
}

#[test]
fn native_runtime_rejects_capability_calls_with_explicit_subset_error() {
    let catalogs = tabula_compiler::CompilerCatalogs::standard()
        .expect("standard catalogs")
        .with_capability_descriptor(tabula_compiler::SourceCapabilityDescriptor {
            path: "demo_hash".into(),
            inputs: vec![tabula_profile::TYPE_U64_ID],
            outputs: vec![tabula_profile::TYPE_BYTES32_ID],
            totality: ir::CapabilityTotality::Total,
            query_policy: ir::CapabilityQueryPolicy::QuerySafe,
            proof_visibility: ir::CapabilityProofVisibility::OpaqueRuntimeOnly,
            hash_family: None,
        })
        .expect("demo hash capability descriptor");
    let registered = register_program_from_source_with_catalogs(capability_source(), &catalogs);

    // Executor path (prepare_executor) runs validate_core_first_program which
    // rejects capability calls. The verifier path is IR-free and does not
    // run this check — the binding-digest gate serves as the primary gating
    // mechanism there.
    let opts = crate::PreparedOptions::try_standard().expect("standard options");
    let err = prepare_executor(Arc::new(registered.clone()), &opts)
        .expect_err("capability-backed program must be rejected before native proving");
    let rendered = err.to_string();
    assert!(
        rendered.contains("outside the current native proving subset"),
        "unexpected executor build error: {rendered}"
    );
    assert!(
        rendered.contains("CallCapability"),
        "unexpected executor build error: {rendered}"
    );
}

#[test]
fn host_runtime_overrides_do_not_change_compiler_sealed_static_table_root() {
    let registered = register_program_from_source(relation_source());
    let extra_type = ExtraTypeRuntime::new();
    let extra_encoding = ExtraEncodingRuntime::new(extra_type.descriptor());
    let host_environment = HostEnvironment::standard()
        .expect("standard host environment")
        .with_runtime_registries(
            crate::host::RuntimeRegistries::standard()
                .expect("standard runtime registries")
                .with_type_runtime(extra_type.clone())
                .expect("register extra type runtime")
                .with_encoding_runtime(extra_encoding)
                .expect("register extra encoding runtime"),
        );

    let opts = crate::PreparedOptions::try_standard()
        .expect("standard options")
        .with_host_environment(host_environment.clone());
    let executor = prepare_executor(Arc::new(registered.clone()), &opts)
        .expect("build executor with extra host runtimes");
    let prover = crate::prepare_prover(Arc::new(registered.clone()), &opts)
        .expect("build prover with extra host runtimes");
    let verifier = crate::prepare_verifier(Arc::new(registered.sealed().clone()), &opts)
        .expect("build verifier with extra host runtimes");

    let batch = tx_batch(vec![ir::EntryCall {
        entry_id: entry_id_for(&executor, "enroll"),
        params: vec![bool_portable(true), u64_portable(0), u64_portable(2)],
    }]);
    let context = relation_context(7, 11);
    let snapshot = relation_snapshot(&registered);
    let executed = executor
        .execute_batch(&snapshot, &batch, &context)
        .expect("execute relation batch under custom host environment");
    let proved = prover
        .prove(&ProveInput {
            snapshot: &snapshot,
            batch: &batch,
            context: &context,
            executed: &executed,
        })
        .expect("prove relation batch under custom host environment");

    assert_eq!(
        prover.runtime_program.static_table_artifact.root,
        registered.static_table_artifact().root
    );
    verifier
        .verify(&proved.proof, &proved.public_statement)
        .expect("verify proof under custom host environment");
}
