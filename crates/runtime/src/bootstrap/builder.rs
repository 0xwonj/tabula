//! Fluent builder for [`TabulaRuntime`](crate::TabulaRuntime) construction.
//!
//! The runtime consumes compiler-owned semantic artifacts plus host-installed
//! capabilities and machine-side proving configuration. Stable extension
//! authoring lives in `tabula-ext`; the runtime only materializes those
//! contracts during `build()`.

use std::collections::BTreeSet;
use std::sync::Arc;

use tabula_chips::ir_hash::IrHashChip;
use tabula_chips::poseidon::PoseidonChip;
use tabula_chips::precompile_transcript::PrecompileTranscriptChip;
use tabula_compiler::SealedProgram;
use tabula_core::TableId;
use tabula_executor::precompile::PrecompileHandler;
use tabula_ext::backend::precompile::PrecompileProofSystem;
use tabula_ir::Instruction;
use tabula_machine::backend::AnyRap;
use tabula_machine::backend::extension::ExecutionTierExtension;

use crate::bootstrap::machine::MachineConfig;
use crate::bootstrap::materialize::{
    materialize_column_backends, materialize_precompile_runtime_backends,
};
use crate::bootstrap::registries::{build_precompile_registry, build_property_query_registry};
use crate::bootstrap::validation::{
    validate_compiler_owned_profiles, validate_precompile_requirements,
};
use crate::error::RuntimeError;
use crate::host::HostEnvironment;
use crate::program::{ColumnProofSlot, PrecompileProofSlot, ProofPlan, RuntimeProgram};
use crate::runtime::TabulaRuntime;

/// Fluent builder for [`TabulaRuntime`](crate::TabulaRuntime).
pub struct RuntimeBuilder {
    compiled_program: SealedProgram,
    host_environment: HostEnvironment,
    machine_config: MachineConfig,
}

impl RuntimeBuilder {
    /// Create a builder with a compiler-produced artifact.
    pub(crate) fn new(compiled_program: SealedProgram) -> Self {
        Self {
            compiled_program,
            host_environment: HostEnvironment::standard(),
            machine_config: MachineConfig::standard(),
        }
    }

    /// Replace the host-installed runtime capabilities used for execution and proving.
    pub fn with_host_environment(mut self, host_environment: HostEnvironment) -> Self {
        self.host_environment = host_environment;
        self
    }

    /// Replace the machine-side proving and verification configuration.
    pub fn with_machine_config(mut self, machine_config: MachineConfig) -> Self {
        self.machine_config = machine_config;
        self
    }

    /// Build the runtime, materializing host-installed capabilities against the sealed program.
    pub fn build(self) -> Result<TabulaRuntime, RuntimeError> {
        self.validate()?;

        let resolved_runtime = materialize_column_backends(
            &self.compiled_program,
            self.host_environment.schemes().factories(),
            self.host_environment.runtime_registries().type_runtimes(),
            self.host_environment
                .runtime_registries()
                .encoding_runtimes(),
            self.machine_config.supported_root_binding_families(),
        )?;
        let proof_columns: Vec<_> = resolved_runtime
            .column_backends
            .values()
            .map(|backend| Arc::clone(&backend.proof_column))
            .collect();
        let proof_slots: Vec<_> = resolved_runtime
            .column_backends
            .values()
            .map(|backend| ColumnProofSlot {
                table: backend.table_id,
                col: backend.col_id,
                proof_backend: Arc::clone(&backend.proof_backend),
            })
            .collect();
        let precompile_slots = materialize_precompile_runtime_backends(
            self.compiled_program.precompile_manifest(),
            self.host_environment.precompiles().factories(),
            self.host_environment
                .runtime_registries()
                .encoding_runtimes(),
        )?;
        let mut machine_builder = self
            .machine_config
            .build_machine_builder()
            .with_columns(proof_columns);
        if program_uses_ir_hash(self.compiled_program.program()) {
            machine_builder = machine_builder
                .with_backend_execution_extension_boxed(Box::new(InternalIrHashExtension));
        }
        if !self.compiled_program.precompile_manifest().is_empty() {
            machine_builder = machine_builder.with_backend_execution_extension_boxed(Box::new(
                InternalPrecompileTranscriptExtension,
            ));
        }
        let mut precompile_handlers = Vec::with_capacity(precompile_slots.len());
        let mut precompile_proof_slots = Vec::with_capacity(precompile_slots.len());
        for slot in precompile_slots {
            precompile_handlers.push(boxed_precompile_handler(slot.handler));
            precompile_proof_slots.push(PrecompileProofSlot {
                descriptor: slot.descriptor,
                preparer: slot.preparer,
            });
            machine_builder = machine_builder.with_backend_execution_extension_boxed(Box::new(
                InternalPrecompileExtension {
                    system: slot.system,
                },
            ));
        }
        let proof_plan = ProofPlan::new(proof_slots, precompile_proof_slots);
        let runtime_program = RuntimeProgram::from_compiled_program(
            &self.compiled_program,
            resolved_runtime,
            self.host_environment
                .runtime_registries()
                .type_runtimes()
                .clone(),
            self.host_environment
                .runtime_registries()
                .encoding_runtimes()
                .clone(),
            proof_plan,
        )?;

        let machine = machine_builder
            .build()
            .map_err(RuntimeError::MachineSetup)?;
        let precompiles = build_precompile_registry(precompile_handlers)?;
        let property_queries = build_property_query_registry(
            runtime_program.proof().runtime_columns(),
            runtime_program.proof().column_backends(),
        )?;

        Ok(TabulaRuntime::from_parts(
            runtime_program,
            machine,
            precompiles,
            property_queries,
        ))
    }

    fn validate(&self) -> Result<(), RuntimeError> {
        self.validate_schemas()?;
        self.validate_compiler_owned_profiles()?;
        self.validate_table_references()?;
        self.validate_precompile_references()?;
        Ok(())
    }

    fn validate_schemas(&self) -> Result<(), RuntimeError> {
        for schema in self.compiled_program.table_schemas() {
            if schema.columns.is_empty() {
                return Err(RuntimeError::ValidationFailed {
                    detail: format!(
                        "table '{}' (id={}) has no columns",
                        schema.name, schema.id.0,
                    ),
                });
            }
        }
        Ok(())
    }

    fn validate_table_references(&self) -> Result<(), RuntimeError> {
        let schema_ids: BTreeSet<TableId> = self
            .compiled_program
            .table_schemas()
            .iter()
            .map(|schema| schema.id)
            .collect();
        for id in self.compiled_program.program().referenced_table_ids() {
            if !schema_ids.contains(&id) {
                return Err(RuntimeError::ValidationFailed {
                    detail: format!("program references table {} not found in schemas", id.0),
                });
            }
        }
        Ok(())
    }

    fn validate_compiler_owned_profiles(&self) -> Result<(), RuntimeError> {
        validate_compiler_owned_profiles(&self.compiled_program)
    }

    fn validate_precompile_references(&self) -> Result<(), RuntimeError> {
        let installed = self
            .host_environment
            .precompiles()
            .factories()
            .keys()
            .copied()
            .collect();
        validate_precompile_requirements(&self.compiled_program, &installed, "precompile backend")
    }
}

fn boxed_precompile_handler(handler: Arc<dyn PrecompileHandler>) -> Box<dyn PrecompileHandler> {
    Box::new(SharedPrecompileHandler(handler))
}

struct SharedPrecompileHandler(Arc<dyn PrecompileHandler>);

impl PrecompileHandler for SharedPrecompileHandler {
    fn id(&self) -> tabula_ir::PrecompileId {
        self.0.id()
    }

    fn signature(&self) -> &tabula_ir::PrecompileSignature {
        self.0.signature()
    }

    fn execute(
        &self,
        inputs: &[tabula_types::TypedValue],
    ) -> Result<Vec<tabula_types::TypedValue>, tabula_core::error::TabulaError> {
        self.0.execute(inputs)
    }
}

struct InternalPrecompileExtension {
    system: Arc<dyn PrecompileProofSystem>,
}

struct InternalIrHashExtension;

struct InternalPrecompileTranscriptExtension;

impl ExecutionTierExtension for InternalPrecompileExtension {
    fn name(&self) -> &str {
        self.system.name()
    }

    fn airs(&self) -> Vec<Box<dyn AnyRap>> {
        self.system.airs()
    }

    fn dyn_chips(&self) -> Vec<Box<dyn tabula_stark::trace::DynChip>> {
        self.system.dyn_chips()
    }

    fn bus_consumers(&self) -> Vec<Box<dyn tabula_stark::trace::column_commitment::BusConsumer>> {
        self.system.bus_consumers()
    }
}

impl ExecutionTierExtension for InternalIrHashExtension {
    fn name(&self) -> &str {
        "ir_hash"
    }

    fn airs(&self) -> Vec<Box<dyn AnyRap>> {
        vec![Box::new(IrHashChip)]
    }

    fn dyn_chips(&self) -> Vec<Box<dyn tabula_stark::trace::DynChip>> {
        vec![Box::new(IrHashChip)]
    }
}

impl ExecutionTierExtension for InternalPrecompileTranscriptExtension {
    fn name(&self) -> &str {
        "precompile_transcript"
    }

    fn airs(&self) -> Vec<Box<dyn AnyRap>> {
        vec![Box::new(PrecompileTranscriptChip), Box::new(PoseidonChip)]
    }

    fn dyn_chips(&self) -> Vec<Box<dyn tabula_stark::trace::DynChip>> {
        vec![Box::new(PrecompileTranscriptChip), Box::new(PoseidonChip)]
    }

    fn bus_consumers(&self) -> Vec<Box<dyn tabula_stark::trace::column_commitment::BusConsumer>> {
        vec![Box::new(PoseidonChip)]
    }
}

fn program_uses_ir_hash(program: &tabula_ir::Program) -> bool {
    program
        .all_types()
        .iter()
        .flat_map(|tx| tx.body.iter())
        .any(|instruction| matches!(instruction, Instruction::Hash { .. }))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tabula_artifact::PrecompileDescriptor;
    use tabula_compiler::CompilerCatalogs;
    use tabula_ext::backend::AnyRap;
    use tabula_ext::backend::precompile::{
        PrecompileBackendFactory, PrecompileProofContext, PrecompileProofPreparer,
        PrecompileProofSystem, PreparedPrecompileProof, ResolvedPrecompile,
    };
    use tabula_ext::{ColumnBackendFactoryBundle, ExtError, PrecompileBackendFactoryBundle};
    use tabula_ir::{PrecompileId, PrecompileSignature, PrecompileValueProfile, TxTypeDef};
    use tabula_profile::{ENCODING_BYTES32_ID, ENCODING_U64_ID, TYPE_BYTES32_ID, TYPE_U64_ID};
    use tabula_testing::exec::{compiled_program_from_artifact, compiled_program_from_definition};
    use tabula_testing::fixtures::artifacts::precompile_requirement_artifact;
    use tabula_testing::fixtures::schema::single_u64_column_schema;

    use super::RuntimeBuilder;
    use crate::error::RuntimeError;
    use crate::host::HostEnvironment;
    use crate::testing::fixtures::{
        compiled_program_with_property_query, compiled_program_with_unsupported_property_query,
    };
    use crate::testing::schemes::{
        EmptySchemeFactory, UnsupportedLayoutSchemeFactory, UnsupportedPropertySchemeFactory,
        custom_scheme_profile, custom_smt_scheme_profile, set_artifact_column_scheme,
        unsupported_layout_scheme_profile,
    };
    use tabula_core::{ColId, SchemeId, TableId, TxTypeId};

    #[derive(Clone)]
    struct DummyPrecompileBackendFactory {
        descriptor: PrecompileDescriptor,
    }

    impl DummyPrecompileBackendFactory {
        fn new(descriptor: PrecompileDescriptor) -> Self {
            Self { descriptor }
        }
    }

    fn dummy_descriptor(id: PrecompileId) -> PrecompileDescriptor {
        PrecompileDescriptor::new(
            id,
            1,
            PrecompileSignature::new(
                vec![PrecompileValueProfile {
                    type_id: TYPE_U64_ID,
                    encoding_profile_id: ENCODING_U64_ID,
                }],
                vec![],
            ),
            [0x33; 32],
        )
    }

    fn wide_descriptor(id: PrecompileId) -> PrecompileDescriptor {
        PrecompileDescriptor::new(
            id,
            1,
            PrecompileSignature::new(
                vec![],
                vec![PrecompileValueProfile {
                    type_id: TYPE_BYTES32_ID,
                    encoding_profile_id: ENCODING_BYTES32_ID,
                }],
            ),
            [0x44; 32],
        )
    }

    struct DummyPrecompileProofSystem {
        descriptor: PrecompileDescriptor,
    }

    impl PrecompileProofSystem for DummyPrecompileProofSystem {
        fn name(&self) -> &str {
            "dummy_precompile"
        }

        fn descriptor(&self) -> PrecompileDescriptor {
            self.descriptor.clone()
        }

        fn airs(&self) -> Vec<Box<dyn AnyRap>> {
            vec![]
        }

        fn dyn_chips(&self) -> Vec<Box<dyn tabula_stark::trace::DynChip>> {
            vec![]
        }
    }

    struct DummyPrecompileProofPreparer {
        descriptor: PrecompileDescriptor,
    }

    impl PrecompileProofPreparer for DummyPrecompileProofPreparer {
        fn name(&self) -> &str {
            "dummy_precompile"
        }

        fn descriptor(&self) -> &PrecompileDescriptor {
            &self.descriptor
        }

        fn prepare_precompile(
            &self,
            _context: PrecompileProofContext,
        ) -> Result<PreparedPrecompileProof, ExtError> {
            Ok(PreparedPrecompileProof {
                store: tabula_stark::trace::WitnessStore::new(),
            })
        }
    }

    struct DummyPrecompileHandler {
        descriptor: PrecompileDescriptor,
    }

    impl tabula_ext::precompile::PrecompileHandler for DummyPrecompileHandler {
        fn id(&self) -> PrecompileId {
            self.descriptor.precompile_id
        }

        fn signature(&self) -> &PrecompileSignature {
            &self.descriptor.signature
        }

        fn execute(
            &self,
            _inputs: &[tabula_types::TypedValue],
        ) -> Result<Vec<tabula_types::TypedValue>, tabula_core::error::TabulaError> {
            Ok(vec![])
        }
    }

    impl PrecompileBackendFactory for DummyPrecompileBackendFactory {
        fn name(&self) -> &str {
            "dummy_precompile"
        }

        fn descriptor(&self) -> &PrecompileDescriptor {
            &self.descriptor
        }

        fn build_system(
            &self,
            resolved: &ResolvedPrecompile,
        ) -> Result<Arc<dyn PrecompileProofSystem>, ExtError> {
            Ok(Arc::new(DummyPrecompileProofSystem {
                descriptor: resolved.descriptor.clone(),
            }))
        }

        fn build_preparer(
            &self,
            resolved: &ResolvedPrecompile,
        ) -> Result<Arc<dyn PrecompileProofPreparer>, ExtError> {
            Ok(Arc::new(DummyPrecompileProofPreparer {
                descriptor: resolved.descriptor.clone(),
            }))
        }

        fn build_handler(
            &self,
            resolved: &ResolvedPrecompile,
        ) -> Result<Arc<dyn tabula_ext::precompile::PrecompileHandler>, ExtError> {
            Ok(Arc::new(DummyPrecompileHandler {
                descriptor: resolved.descriptor.clone(),
            }))
        }
    }

    fn compiled_single_column_noop_program() -> tabula_compiler::SealedProgram {
        let schema = single_u64_column_schema(TableId(1), ColId(0), "accounts", "balance");
        let tx = TxTypeDef {
            id: TxTypeId(1),
            name: "noop".to_string(),
            param_schema: vec![],
            body: vec![],
        };
        compiled_program_from_definition(vec![schema], vec![tx])
    }

    #[test]
    fn custom_scheme_factory_resolves_proof_preparer() {
        let mut artifact = compiled_single_column_noop_program().into_artifact();
        set_artifact_column_scheme(&mut artifact, 0, custom_scheme_profile(SchemeId(0x1000)));
        let compiled = compiled_program_from_artifact(&artifact);

        let host_environment = HostEnvironment::standard()
            .with_column_backend_bundle(ColumnBackendFactoryBundle::new(EmptySchemeFactory))
            .expect("register custom backend bundle");
        let runtime = RuntimeBuilder::new(compiled)
            .with_host_environment(host_environment)
            .build()
            .expect("runtime");

        assert_eq!(runtime.proof_program().runtime_columns().len(), 1);
        assert_eq!(runtime.proof_program().proof_plan().column_slots().len(), 1);
        let backend = runtime
            .proof_program()
            .column_backends()
            .get(&(TableId(1), ColId(0)))
            .expect("column backend");
        assert_eq!(backend.verifier_contract.scheme_id, SchemeId(0x1000));
    }

    #[test]
    fn runtime_rejects_explicit_proof_scheme_for_unsupported_layout() {
        let compiled = compiled_single_column_noop_program();
        let mut artifact = compiled.into_artifact();
        set_artifact_column_scheme(
            &mut artifact,
            0,
            unsupported_layout_scheme_profile(SchemeId(0x1000)),
        );
        let compiled = compiled_program_from_artifact(&artifact);
        let host_environment = HostEnvironment::standard()
            .with_column_backend_bundle(ColumnBackendFactoryBundle::new(
                UnsupportedLayoutSchemeFactory,
            ))
            .expect("register custom backend bundle");

        let err = RuntimeBuilder::new(compiled)
            .with_host_environment(host_environment)
            .build()
            .expect_err("unsupported layout should fail");

        match err {
            RuntimeError::ValidationFailed { detail } => {
                assert!(detail.contains("unsupported proof scheme layout"));
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn built_in_runtime_registers_ordered_proof_slots() {
        let compiled = compiled_single_column_noop_program();

        let runtime = RuntimeBuilder::new(compiled).build().expect("runtime");

        assert_eq!(runtime.proof_program().runtime_columns().len(), 1);
        assert_eq!(runtime.proof_program().proof_plan().column_slots().len(), 1);
    }

    #[test]
    fn runtime_rejects_missing_registered_scheme_factory() {
        let compiled = compiled_single_column_noop_program();
        let mut artifact = compiled.into_artifact();
        set_artifact_column_scheme(&mut artifact, 0, custom_scheme_profile(SchemeId(0x1000)));
        let compiled = compiled_program_from_artifact(&artifact);

        let err = RuntimeBuilder::new(compiled)
            .build()
            .expect_err("missing scheme");
        match err {
            RuntimeError::ValidationFailed { detail } => {
                assert!(
                    detail.contains("no canonical backend"),
                    "unexpected detail: {detail}"
                );
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn runtime_rejects_unsupported_property_requirement_via_factory() {
        let compiled = compiled_program_with_unsupported_property_query();
        let mut artifact = compiled.into_artifact();
        set_artifact_column_scheme(
            &mut artifact,
            0,
            custom_smt_scheme_profile(SchemeId(0x1001)),
        );
        let compiled = compiled_program_from_artifact(&artifact);
        let host_environment = HostEnvironment::standard()
            .with_column_backend_bundle(ColumnBackendFactoryBundle::new(
                UnsupportedPropertySchemeFactory,
            ))
            .expect("register custom backend bundle");

        let err = RuntimeBuilder::new(compiled)
            .with_host_environment(host_environment)
            .build()
            .expect_err("unsupported property requirement should fail");

        match err {
            RuntimeError::ValidationFailed { detail } => {
                assert!(detail.contains("unsupported property"));
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn host_environment_rejects_duplicate_scheme_factory_ids() {
        let result = HostEnvironment::standard()
            .with_column_backend_bundle(ColumnBackendFactoryBundle::new(EmptySchemeFactory))
            .expect("first")
            .with_column_backend_bundle(ColumnBackendFactoryBundle::new(EmptySchemeFactory));

        match result {
            Err(RuntimeError::ValidationFailed { detail }) => {
                assert!(detail.contains("duplicate scheme backend registration"));
            }
            Err(other) => panic!("unexpected error: {other}"),
            Ok(_) => panic!("duplicate scheme id should fail"),
        }
    }

    #[test]
    fn runtime_materializes_matching_ordered_proof_slots() {
        let compiled = compiled_program_with_property_query();
        let runtime = RuntimeBuilder::new(compiled).build().expect("runtime");

        assert_eq!(
            runtime.proof_program().runtime_columns().len(),
            runtime.proof_program().proof_plan().column_slots().len()
        );

        for slot in runtime.proof_program().proof_plan().column_slots() {
            let backend = runtime
                .proof_program()
                .column_backends()
                .get(&(slot.table, slot.col))
                .expect("column backend");
            assert_eq!(
                slot.proof_backend.scheme_id(),
                backend.verifier_contract.scheme_id
            );
        }
    }

    #[test]
    fn runtime_supports_custom_only_host_environment() {
        let compiled = compiled_single_column_noop_program();
        let mut artifact = compiled.into_artifact();
        set_artifact_column_scheme(&mut artifact, 0, custom_scheme_profile(SchemeId(0x1000)));
        let compiled = compiled_program_from_artifact(&artifact);
        let host_environment = HostEnvironment::empty()
            .with_runtime_registries(crate::host::RuntimeRegistries::standard())
            .with_column_backend_bundle(ColumnBackendFactoryBundle::new(EmptySchemeFactory))
            .expect("register custom backend bundle");

        let runtime = RuntimeBuilder::new(compiled)
            .with_host_environment(host_environment)
            .build()
            .expect("custom-only runtime");

        assert_eq!(runtime.proof_program().runtime_columns().len(), 1);
        assert_eq!(runtime.proof_program().proof_plan().column_slots().len(), 1);
    }

    #[test]
    fn runtime_custom_only_mode_requires_installed_scheme_backend() {
        let compiled = compiled_single_column_noop_program();
        let mut artifact = compiled.into_artifact();
        set_artifact_column_scheme(&mut artifact, 0, custom_scheme_profile(SchemeId(0x1000)));
        let compiled = compiled_program_from_artifact(&artifact);
        let host_environment = HostEnvironment::empty()
            .with_runtime_registries(crate::host::RuntimeRegistries::standard());

        let err = RuntimeBuilder::new(compiled)
            .with_host_environment(host_environment)
            .build()
            .expect_err("missing backend bundle should fail");

        match err {
            RuntimeError::ValidationFailed { detail } => {
                assert!(
                    detail.contains("no canonical backend"),
                    "unexpected detail: {detail}"
                );
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn runtime_rejects_missing_required_precompile_backend() {
        let compiled = compiled_program_from_artifact(&precompile_requirement_artifact());

        let err = RuntimeBuilder::new(compiled)
            .build()
            .expect_err("missing precompile backend should fail");

        match err {
            RuntimeError::ValidationFailed { detail } => {
                assert!(detail.contains("precompile backend"));
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn runtime_rejects_precompile_backend_descriptor_mismatch() {
        let compiled = compiled_program_from_artifact(&precompile_requirement_artifact());
        let mut wrong_descriptor = dummy_descriptor(PrecompileId(0x0001));
        wrong_descriptor.precompile_version = 9;
        wrong_descriptor.semantic_hash = [0x99; 32];
        let host_environment = HostEnvironment::standard()
            .with_precompile_backend_bundle(PrecompileBackendFactoryBundle::new(
                DummyPrecompileBackendFactory::new(wrong_descriptor),
            ))
            .expect("register dummy precompile backend");

        let err = RuntimeBuilder::new(compiled)
            .with_host_environment(host_environment)
            .build()
            .expect_err("descriptor mismatch should fail");

        match err {
            RuntimeError::ValidationFailed { detail } => {
                assert!(
                    detail.contains("expects descriptor") || detail.contains("resolved descriptor"),
                    "unexpected detail: {detail}",
                );
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[cfg(feature = "prove")]
    #[test]
    fn compiler_catalogs_reject_precompile_io_wider_than_execution_width() {
        let err = CompilerCatalogs::standard()
            .with_precompile_descriptor(wide_descriptor(PrecompileId(0x0001)))
            .expect_err("wide execution precompile I/O should fail at catalog registration");

        match err {
            tabula_compiler::CompilerCatalogError::InvalidPrecompileDescriptor { detail } => {
                assert!(detail.contains("generic execution lane only supports width 3"));
            }
            other => panic!("unexpected error: {other}"),
        }
    }
}
