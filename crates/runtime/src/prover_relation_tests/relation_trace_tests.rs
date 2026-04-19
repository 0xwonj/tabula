//! Relation-table chip trace generation and prover integration tests.
//!
//! Covers snapshot / execution infrastructure, relation-claim validation,
//! lowering, bundled-root authority, capability rejection, and host-runtime
//! override isolation.

use super::{
    capability_source, enroll_batch, entry_id_for, executor_and_prover_for_source,
    guarded_context, guarded_relation_source, prepare_executor, prove_input, relation_context,
    relation_snapshot, relation_source,
};
use crate::host::HostEnvironment;
use crate::ProveInput;

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use p3_field::PrimeCharacteristicRing;
use p3_koala_bear::KoalaBear;
use tabula_ir as ir;
use tabula_chips::execution::EXECUTION_STANDARD_VALUE_WIDTH;
use tabula_chips::relation_table::{RELATION_TABLE_WITNESS_LABEL, RelationTableWitnessRow};
use tabula_chips::relation_transcript::RELATION_TRANSCRIPT_WITNESS_LABEL;
use tabula_commitment::PoseidonHasher;
use tabula_core::error::TabulaError;
use tabula_core::{EncodingProfileId, PortableValue, TypeId};
use tabula_ext::root::{
    RootBackend, RootBackendBundle, RootWitnessPreparer, SmtRootWitnessPreparer,
};
use tabula_machine::{RootProofBackend, SmtRootProofBackend};
use tabula_profile::{
    CanonicalNullEncoding, EncodingClass, EncodingProfile, FieldFamily, GenericIrFamily,
    HostValueFamily, NullSemantics, TranscriptSerialization, TypeCapabilities, TypeDescriptor,
    ZeroValueSpec,
};
use tabula_testing::exec::{register_program_from_source, register_program_from_source_with_catalogs};
use tabula_types::{ArithmeticOp, EncodingRuntime, TypeRuntime, TypedValue, u64_portable};
use tabula_witness::stark::{LowerSuccessfulTxInput, lower_successful_tx};
use tabula_witness::{RelationClaim, RelationClaimKind, prepare_relation_proof};

const TEST_EXTRA_TYPE_ID: TypeId = TypeId(90_001);
const TEST_EXTRA_ENCODING_ID: EncodingProfileId = EncodingProfileId(90_001);

/// Root-proof backend stub that advertises no supported binding families.
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

/// Root-backend stub that wraps [`EmptyFamilyRootProofBackend`].
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

/// Extra type runtime used to verify host-runtime overrides do not affect
/// the compiler-sealed static relation-table root.
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

/// Extra encoding runtime used alongside [`ExtraTypeRuntime`].
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

#[test]
fn relation_table_rows_reject_claims_missing_from_manifest() {
    let (registered, executor, _prover) = executor_and_prover_for_source(relation_source());
    let error = prepare_relation_proof(
        executor.state.semantic.execution().program(),
        registered.static_table_artifact(),
        &[RelationClaim {
            relation: ir::RelationId(0),
            kind: RelationClaimKind::Assert,
            inputs: vec![tabula_types::u64_typed(9)],
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
    let batch = tabula_testing::exec::tx_batch(vec![ir::EntryCall {
        entry_id: enroll,
        params: vec![
            tabula_types::bool_portable(true),
            u64_portable(0),
            u64_portable(2),
        ],
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
    let batch = tabula_testing::exec::tx_batch(vec![ir::EntryCall {
        entry_id: entry_id_for(&executor, "maybe_promote"),
        params: vec![
            tabula_types::bool_portable(false),
            u64_portable(0),
            u64_portable(2),
        ],
    }]);
    let context = guarded_context(7);
    let snapshot = relation_snapshot(&registered);
    let executed = executor
        .execute_batch(&snapshot, &batch, &context)
        .expect("execute guarded batch");

    let (machine_input, _public_statement) = crate::proof_artifacts::prepare_proof_machine_input(
        &prover.state,
        &prover.root_backend_bundle,
        &prover.kit_registry,
        &prove_input(&snapshot, &batch, &context, &executed),
    )
    .expect("prepare proof request");

    let transcript_calls = machine_input
        .execution
        .store
        .get::<Vec<tabula_chips::relation_transcript::RelationTranscriptCall>>(
            RELATION_TRANSCRIPT_WITNESS_LABEL,
        )
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
    let registered =
        register_program_from_source_with_catalogs(capability_source(), &catalogs);

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

    let batch = enroll_batch(&executor);
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
        prover.state.static_table_artifact.root,
        registered.static_table_artifact().root
    );
    verifier
        .verify(&proved.proof, &proved.public_statement)
        .expect("verify proof under custom host environment");
}
