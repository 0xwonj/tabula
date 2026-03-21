//! Fluent builder for [`TabulaRuntime`](crate::TabulaRuntime) construction.
//!
//! The runtime owns scheme registry resolution. Stable runtime scheme factories,
//! precompile proof factories, and proof-extension factories are materialized
//! separately during `build()`.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use tabula_artifact::PrecompileDescriptor;
use tabula_chips::precompile_transcript::PrecompileTranscriptChip;
use tabula_compiler::SealedProgram;
use tabula_core::TableId;
use tabula_ext::{PrecompileBundle, SchemeBundle};
use tabula_ir::PrecompileId;
use tabula_machine::backend::AnyRap;
use tabula_machine::backend::extension::ExecutionTierExtension;
use tabula_machine::{RootProof, TabulaStarkConfig};

use crate::capabilities::PrecompileRegistration;
use crate::error::RuntimeError;
use crate::precompile_proofs::{PrecompileProofFactory, PrecompileProofSystem};
use crate::program::ResolvedProgram;
use crate::runtime::TabulaRuntime;
use crate::setup::builder_state::{MachineConfigBase, ProofRegistryBase, RuntimeRegistryBase};
use crate::setup::materialize::{
    ColumnProofRecipe, materialize_precompile_proofs_with_factories,
    materialize_proof_slots_with_factories, resolve_runtime_columns_with_factories,
};
use crate::setup::planning::derive_column_plans;
use crate::setup::registries::{build_precompile_registry, build_property_query_registry};
use crate::setup::validation::{
    validate_compiler_owned_proof_plan, validate_precompile_requirements,
};

/// Fluent builder for [`TabulaRuntime`](crate::TabulaRuntime).
pub struct RuntimeBuilder {
    compiled_program: SealedProgram,
    machine_base: MachineConfigBase,
    runtime_registry: RuntimeRegistryBase,
    proof_registry: ProofRegistryBase,
    precompile_registrations: Vec<PrecompileRegistration>,
}

impl RuntimeBuilder {
    /// Create a builder with a compiler-produced artifact.
    pub(crate) fn new(compiled_program: SealedProgram) -> Self {
        Self {
            compiled_program,
            machine_base: MachineConfigBase::new(),
            runtime_registry: RuntimeRegistryBase::seeded(),
            proof_registry: ProofRegistryBase::seeded(),
            precompile_registrations: Vec::new(),
        }
    }

    /// Register a precompile capability as one logical unit.
    pub fn with_precompile(mut self, bundle: PrecompileBundle) -> Result<Self, RuntimeError> {
        let registration = PrecompileRegistration::from_bundle(bundle)?;
        if self
            .precompile_registrations
            .iter()
            .any(|existing| existing.id() == registration.id())
        {
            return Err(RuntimeError::ValidationFailed {
                detail: format!(
                    "duplicate precompile registration for id 0x{:04x}",
                    registration.id().0
                ),
            });
        }
        self.precompile_registrations.push(registration);
        Ok(self)
    }

    /// Clear all preloaded standard schemes from both runtime and proof registries.
    pub fn without_default_schemes(mut self) -> Self {
        self.runtime_registry = RuntimeRegistryBase::empty();
        self.proof_registry = ProofRegistryBase::empty();
        self
    }

    /// Register matching runtime/proof factories as one logical custom scheme bundle.
    pub fn with_scheme_bundle(mut self, bundle: SchemeBundle) -> Result<Self, RuntimeError> {
        let (runtime_descriptor, runtime_factory, proof_factory) = bundle.into_parts();
        if self.runtime_registry.contains(runtime_descriptor.scheme_id)
            || self.proof_registry.contains(runtime_descriptor.scheme_id)
        {
            return Err(RuntimeError::ValidationFailed {
                detail: format!(
                    "duplicate scheme bundle registration for id {}",
                    runtime_descriptor.scheme_id.0
                ),
            });
        }

        self.runtime_registry.insert_arc(runtime_factory)?;
        self.proof_registry.insert_arc(proof_factory)?;
        Ok(self)
    }

    /// Override the root proof scheme.
    pub fn with_root_proof(mut self, root: impl RootProof + 'static) -> Self {
        self.machine_base = self.machine_base.with_root_proof(root);
        self
    }

    /// Override the STARK configuration.
    pub fn with_config(mut self, config: TabulaStarkConfig) -> Self {
        self.machine_base = self.machine_base.with_config(config);
        self
    }

    /// Build the runtime, materializing per-column schemes before machine setup.
    pub fn build(self) -> Result<TabulaRuntime, RuntimeError> {
        self.validate()?;

        let column_plans = derive_column_plans(&self.compiled_program)?;
        let resolved_runtime = resolve_runtime_columns_with_factories(
            &column_plans,
            self.runtime_registry.factories(),
            self.machine_base.root_profile_id(),
        )?;
        let materialized_proof_slots = materialize_proof_slots_with_factories(
            &column_plans,
            self.proof_registry.factories(),
            self.machine_base.root_profile_id(),
        )?;
        let precompile_slots = materialize_precompile_proofs_with_factories(
            self.compiled_program.precompile_manifest(),
            &self.precompile_proof_factories(),
        )?;
        let proof_columns: Vec<_> = materialized_proof_slots
            .iter()
            .map(|slot| std::sync::Arc::clone(&slot.proof_column))
            .collect();
        let resolved_program =
            ResolvedProgram::from_compiled_program(&self.compiled_program, resolved_runtime)?;

        let mut machine_builder = self
            .machine_base
            .into_machine_builder()
            .with_columns(proof_columns);
        if !self.compiled_program.precompile_manifest().is_empty() {
            machine_builder = machine_builder.with_backend_execution_extension_boxed(Box::new(
                InternalPrecompileTranscriptExtension,
            ));
        }

        let mut precompile_handlers = Vec::with_capacity(self.precompile_registrations.len());
        let mut precompile_recipes = Vec::with_capacity(precompile_slots.len());
        let mut precompile_systems_by_id = BTreeMap::new();
        for slot in precompile_slots {
            #[cfg(feature = "prove")]
            precompile_recipes.push(crate::proving::PrecompileProofRecipe {
                descriptor: slot.descriptor.clone(),
                preparer: slot.preparer,
            });
            precompile_systems_by_id.insert(slot.descriptor.precompile_id, slot.system);
        }
        for registration in self.precompile_registrations {
            let (descriptor, handler, _proof_factory) = registration.into_parts();
            precompile_handlers.push(handler);
            let system = precompile_systems_by_id
                .remove(&descriptor.precompile_id)
                .ok_or_else(|| RuntimeError::ValidationFailed {
                    detail: format!(
                        "missing materialized precompile proof system for id 0x{:04x}",
                        descriptor.precompile_id.0,
                    ),
                })?;
            machine_builder = machine_builder.with_backend_execution_extension_boxed(Box::new(
                InternalPrecompileExtension { system },
            ));
        }

        let machine = machine_builder
            .build()
            .map_err(RuntimeError::MachineSetup)?;
        let precompiles = build_precompile_registry(precompile_handlers)?;
        let property_queries = build_property_query_registry(
            resolved_program.runtime_columns(),
            resolved_program.column_plans(),
        )?;

        Ok(TabulaRuntime::from_parts(
            resolved_program,
            materialized_proof_slots
                .into_iter()
                .map(|slot| ColumnProofRecipe {
                    table: slot.plan.table_id,
                    col: slot.plan.col_id,
                    value_type: slot.plan.value_type,
                    preparer: slot.preparer,
                })
                .collect(),
            #[cfg(feature = "prove")]
            precompile_recipes,
            machine,
            precompiles,
            property_queries,
        ))
    }

    fn validate(&self) -> Result<(), RuntimeError> {
        self.validate_schemas()?;
        self.validate_compiler_owned_proof_plan()?;
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
            .map(|s| s.id)
            .collect();
        for id in self.compiled_program.program().referenced_table_ids() {
            if !schema_ids.contains(&id) {
                return Err(RuntimeError::ValidationFailed {
                    detail: format!("program references table {} not found in schemas", id.0,),
                });
            }
        }
        Ok(())
    }

    fn validate_compiler_owned_proof_plan(&self) -> Result<(), RuntimeError> {
        validate_compiler_owned_proof_plan(&self.compiled_program)
    }

    fn validate_precompile_references(&self) -> Result<(), RuntimeError> {
        let registered = self.registered_precompile_ids();
        validate_precompile_requirements(&self.compiled_program, &registered, "handler")
    }

    fn registered_precompile_ids(&self) -> BTreeMap<PrecompileId, PrecompileDescriptor> {
        self.precompile_registrations
            .iter()
            .map(|registration| (registration.id(), registration.descriptor().clone()))
            .collect()
    }

    fn precompile_proof_factories(
        &self,
    ) -> BTreeMap<PrecompileId, Arc<dyn PrecompileProofFactory>> {
        self.precompile_registrations
            .iter()
            .map(|registration| (registration.id(), registration.proof_factory()))
            .collect()
    }
}

struct InternalPrecompileExtension {
    system: Arc<dyn PrecompileProofSystem>,
}

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

impl ExecutionTierExtension for InternalPrecompileTranscriptExtension {
    fn name(&self) -> &str {
        "precompile_transcript"
    }

    fn airs(&self) -> Vec<Box<dyn AnyRap>> {
        vec![Box::new(PrecompileTranscriptChip)]
    }

    fn dyn_chips(&self) -> Vec<Box<dyn tabula_stark::trace::DynChip>> {
        vec![Box::new(PrecompileTranscriptChip)]
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tabula_artifact::PrecompileDescriptor;
    use tabula_compiler::register_program;
    use tabula_core::error::TabulaError;
    use tabula_core::{ColId, SchemeId, TableId, TableSchema, TxTypeId, ValueType};
    use tabula_executor::precompile::PrecompileHandler;
    use tabula_ext::backend::AnyRap;
    use tabula_ext::backend::precompile::{
        PrecompileProofContext, PrecompileProofFactory, PrecompileProofPreparer,
        PrecompileProofSystem, PreparedPrecompileProof, ResolvedPrecompile,
    };
    use tabula_ext::backend::scheme::{ColumnProofPreparer, ProofSchemeFactory};
    use tabula_ext::{ExtError, PrecompileBundle, ResolvedColumnPlan, SchemeBundle};
    use tabula_ir::{PrecompileId, TxTypeDef};
    use tabula_testing::exec::compiled_program_from_artifact;

    use super::RuntimeBuilder;
    use crate::error::RuntimeError;
    use crate::testing::fixtures::{
        compiled_program_with_property_query, compiled_program_with_unsupported_property_query,
    };
    use crate::testing::prove::{
        EmptyProofColumn, EmptyProofPreparer, EmptySchemeFactory, UnsupportedLayoutSchemeFactory,
        UnsupportedPropertySchemeFactory, custom_descriptor, set_artifact_column_scheme,
        unsupported_layout_descriptor,
    };

    #[derive(Clone)]
    struct DummyPrecompileProofFactory {
        descriptor: PrecompileDescriptor,
    }

    impl DummyPrecompileProofFactory {
        fn new(descriptor: PrecompileDescriptor) -> Self {
            Self { descriptor }
        }
    }

    struct DummyPrecompileProofSystem;

    impl PrecompileProofSystem for DummyPrecompileProofSystem {
        fn name(&self) -> &str {
            "dummy_precompile"
        }

        fn descriptor(&self) -> PrecompileDescriptor {
            PrecompileDescriptor::from_labels(
                PrecompileId(0x0001),
                1,
                "dummy.params",
                "dummy.semantic",
            )
        }

        fn airs(&self) -> Vec<Box<dyn AnyRap>> {
            vec![]
        }

        fn dyn_chips(&self) -> Vec<Box<dyn tabula_stark::trace::DynChip>> {
            vec![]
        }
    }

    struct DummyPrecompileProofPreparer {
        id: PrecompileId,
    }

    impl PrecompileProofPreparer for DummyPrecompileProofPreparer {
        fn name(&self) -> &str {
            "dummy_precompile"
        }

        fn precompile_id(&self) -> PrecompileId {
            self.id
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

    impl PrecompileProofFactory for DummyPrecompileProofFactory {
        fn descriptor(&self) -> PrecompileDescriptor {
            self.descriptor.clone()
        }

        fn name(&self) -> &str {
            "dummy_precompile"
        }

        fn build_system(
            &self,
            _resolved: &ResolvedPrecompile,
        ) -> Result<Arc<dyn PrecompileProofSystem>, ExtError> {
            Ok(Arc::new(DummyPrecompileProofSystem))
        }

        fn build_preparer(
            &self,
            resolved: &ResolvedPrecompile,
        ) -> Result<Arc<dyn PrecompileProofPreparer>, ExtError> {
            Ok(Arc::new(DummyPrecompileProofPreparer {
                id: resolved.descriptor.precompile_id,
            }))
        }
    }

    #[test]
    fn custom_scheme_factory_resolves_proof_preparer() {
        let schema = TableSchema {
            id: TableId(1),
            name: "accounts".to_string(),
            columns: vec![tabula_core::ColumnDef {
                id: ColId(0),
                name: "balance".to_string(),
                value_type: ValueType::U64,
            }],
        };
        let tx = TxTypeDef {
            id: TxTypeId(1),
            name: "noop".to_string(),
            param_schema: vec![],
            body: vec![],
        };
        let compiled = register_program(&[schema], &[tx]).expect("register program");
        compiled.as_artifact();
        let plan = compiled.column_proof_plan()[0].clone();
        let mut artifact = compiled.into_artifact();
        set_artifact_column_scheme(&mut artifact, 0, custom_descriptor(SchemeId(0x1000)));
        let compiled = compiled_program_from_artifact(&artifact);

        let runtime = RuntimeBuilder::new(compiled)
            .with_scheme_bundle(
                SchemeBundle::new(EmptySchemeFactory, EmptySchemeFactory)
                    .expect("empty scheme bundle"),
            )
            .expect("register custom scheme bundle")
            .build()
            .expect("runtime");

        assert_eq!(runtime.resolved_program().runtime_columns().len(), 1);
        assert_eq!(runtime.proof_recipes().len(), 1);
        let plan = runtime
            .resolved_program()
            .column_plans()
            .get(&(plan.table_id, plan.col_id))
            .expect("column plan");
        assert_eq!(plan.scheme_id, SchemeId(0x1000));
    }

    #[test]
    fn runtime_rejects_explicit_proof_scheme_for_unsupported_layout() {
        let schema = TableSchema {
            id: TableId(1),
            name: "accounts".to_string(),
            columns: vec![tabula_core::ColumnDef {
                id: ColId(0),
                name: "balance".to_string(),
                value_type: ValueType::U64,
            }],
        };
        let tx = TxTypeDef {
            id: TxTypeId(1),
            name: "noop".to_string(),
            param_schema: vec![],
            body: vec![],
        };
        let compiled = register_program(&[schema], &[tx]).expect("register program");
        let mut artifact = compiled.into_artifact();
        set_artifact_column_scheme(
            &mut artifact,
            0,
            unsupported_layout_descriptor(SchemeId(0x1000)),
        );
        let compiled = compiled_program_from_artifact(&artifact);

        let err = RuntimeBuilder::new(compiled)
            .with_scheme_bundle(
                SchemeBundle::new(
                    UnsupportedLayoutSchemeFactory,
                    UnsupportedLayoutSchemeFactory,
                )
                .expect("unsupported layout bundle"),
            )
            .expect("register custom scheme bundle")
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
    fn built_in_runtime_registers_ordered_proof_recipes() {
        let schema = TableSchema {
            id: TableId(1),
            name: "accounts".to_string(),
            columns: vec![tabula_core::ColumnDef {
                id: ColId(0),
                name: "balance".to_string(),
                value_type: ValueType::U64,
            }],
        };
        let tx = TxTypeDef {
            id: TxTypeId(1),
            name: "noop".to_string(),
            param_schema: vec![],
            body: vec![],
        };
        let compiled = register_program(&[schema], &[tx]).expect("register program");

        let runtime = RuntimeBuilder::new(compiled).build().expect("runtime");

        assert_eq!(runtime.resolved_program().runtime_columns().len(), 1);
        assert_eq!(runtime.proof_recipes().len(), 1);
    }

    #[test]
    fn runtime_rejects_missing_registered_scheme_factory() {
        let schema = TableSchema {
            id: TableId(1),
            name: "accounts".to_string(),
            columns: vec![tabula_core::ColumnDef {
                id: ColId(0),
                name: "balance".to_string(),
                value_type: ValueType::U64,
            }],
        };
        let tx = TxTypeDef {
            id: TxTypeId(1),
            name: "noop".to_string(),
            param_schema: vec![],
            body: vec![],
        };
        let compiled = register_program(&[schema], &[tx]).expect("register program");
        let mut artifact = compiled.into_artifact();
        set_artifact_column_scheme(&mut artifact, 0, custom_descriptor(SchemeId(0x1000)));
        let compiled = compiled_program_from_artifact(&artifact);

        let err = RuntimeBuilder::new(compiled)
            .build()
            .expect_err("missing scheme");
        match err {
            RuntimeError::ValidationFailed { detail } => {
                assert!(detail.contains("scheme factory"));
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn runtime_rejects_unsupported_property_requirement_via_factory() {
        let compiled = compiled_program_with_unsupported_property_query();
        let mut artifact = compiled.into_artifact();
        set_artifact_column_scheme(&mut artifact, 0, custom_descriptor(SchemeId(0x1001)));
        let compiled = compiled_program_from_artifact(&artifact);

        let err = RuntimeBuilder::new(compiled)
            .with_scheme_bundle(
                SchemeBundle::new(
                    UnsupportedPropertySchemeFactory,
                    UnsupportedPropertySchemeFactory,
                )
                .expect("unsupported property bundle"),
            )
            .expect("register custom scheme bundle")
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
    fn runtime_rejects_duplicate_scheme_factory_ids() {
        let compiled = compiled_program_with_property_query();
        let result = RuntimeBuilder::new(compiled)
            .with_scheme_bundle(
                SchemeBundle::new(EmptySchemeFactory, EmptySchemeFactory)
                    .expect("first scheme bundle"),
            )
            .expect("first")
            .with_scheme_bundle(
                SchemeBundle::new(EmptySchemeFactory, EmptySchemeFactory)
                    .expect("duplicate scheme bundle"),
            );

        match result {
            Err(err) => match err {
                RuntimeError::ValidationFailed { detail } => {
                    assert!(detail.contains("duplicate scheme bundle"));
                }
                other => panic!("unexpected error: {other}"),
            },
            Ok(_) => panic!("duplicate proof scheme id should fail"),
        }
    }

    #[test]
    fn runtime_materializes_matching_ordered_proof_recipes() {
        let compiled = compiled_program_with_property_query();
        let runtime = RuntimeBuilder::new(compiled).build().expect("runtime");

        assert_eq!(
            runtime.resolved_program().runtime_columns().len(),
            runtime.proof_recipes().len()
        );
        assert_eq!(
            runtime.resolved_program().runtime_columns().len(),
            runtime.proof_recipes().len()
        );

        for slot in runtime.proof_recipes() {
            let plan = runtime
                .resolved_program()
                .column_plans()
                .get(&(slot.table, slot.col))
                .expect("column plan");
            assert_eq!(slot.value_type, plan.value_type);
        }
    }

    #[test]
    fn scheme_bundle_rejects_mismatched_scheme_ids() {
        let _compiled = compiled_program_with_property_query();
        let result = SchemeBundle::new(EmptySchemeFactory, UnsupportedPropertySchemeFactory);

        match result {
            Err(ExtError::Validation { detail }) => {
                assert!(detail.contains("identical scheme ids"));
            }
            Err(other) => panic!("unexpected error: {other}"),
            Ok(_) => panic!("mismatched scheme ids should fail"),
        }
    }

    #[test]
    fn scheme_bundle_rejects_mismatched_descriptors() {
        #[derive(Clone)]
        struct DescriptorMismatchProofSchemeFactory;

        impl ProofSchemeFactory for DescriptorMismatchProofSchemeFactory {
            fn descriptor(&self) -> tabula_artifact::SchemeDescriptor {
                let mut descriptor = custom_descriptor(SchemeId(0x1000));
                descriptor.params_hash = [0x55; 32];
                descriptor
            }

            fn name(&self) -> &str {
                "descriptor_mismatch"
            }

            fn build_proof_column(
                &self,
                plan: &ResolvedColumnPlan,
            ) -> Result<std::sync::Arc<dyn tabula_ext::backend::ProofColumn>, ExtError>
            {
                Ok(std::sync::Arc::new(EmptyProofColumn { plan: plan.clone() }))
            }

            fn build_proof_preparer(
                &self,
                plan: &ResolvedColumnPlan,
            ) -> Result<std::sync::Arc<dyn ColumnProofPreparer>, ExtError> {
                Ok(std::sync::Arc::new(EmptyProofPreparer {
                    plan: plan.clone(),
                }))
            }
        }

        let _compiled = compiled_program_with_property_query();
        let result = SchemeBundle::new(EmptySchemeFactory, DescriptorMismatchProofSchemeFactory);

        match result {
            Err(ExtError::Validation { detail }) => {
                assert!(detail.contains("identical descriptors"));
            }
            Err(other) => panic!("unexpected error: {other}"),
            Ok(_) => panic!("mismatched descriptors should fail"),
        }
    }

    #[test]
    fn runtime_supports_custom_only_mode_with_scheme_bundle() {
        let schema = TableSchema {
            id: TableId(1),
            name: "accounts".to_string(),
            columns: vec![tabula_core::ColumnDef {
                id: ColId(0),
                name: "balance".to_string(),
                value_type: ValueType::U64,
            }],
        };
        let tx = TxTypeDef {
            id: TxTypeId(1),
            name: "noop".to_string(),
            param_schema: vec![],
            body: vec![],
        };
        let compiled = register_program(&[schema], &[tx]).expect("register program");
        let mut artifact = compiled.into_artifact();
        set_artifact_column_scheme(&mut artifact, 0, custom_descriptor(SchemeId(0x1000)));
        let compiled = compiled_program_from_artifact(&artifact);

        let runtime = RuntimeBuilder::new(compiled)
            .without_default_schemes()
            .with_scheme_bundle(
                SchemeBundle::new(EmptySchemeFactory, EmptySchemeFactory)
                    .expect("empty scheme bundle"),
            )
            .expect("register custom scheme bundle")
            .build()
            .expect("custom-only runtime");

        assert_eq!(runtime.resolved_program().runtime_columns().len(), 1);
        assert_eq!(runtime.proof_recipes().len(), 1);
    }

    #[test]
    fn runtime_custom_only_mode_requires_scheme_bundle() {
        let schema = TableSchema {
            id: TableId(1),
            name: "accounts".to_string(),
            columns: vec![tabula_core::ColumnDef {
                id: ColId(0),
                name: "balance".to_string(),
                value_type: ValueType::U64,
            }],
        };
        let tx = TxTypeDef {
            id: TxTypeId(1),
            name: "noop".to_string(),
            param_schema: vec![],
            body: vec![],
        };
        let compiled = register_program(&[schema], &[tx]).expect("register program");
        let mut artifact = compiled.into_artifact();
        set_artifact_column_scheme(&mut artifact, 0, custom_descriptor(SchemeId(0x1000)));
        let compiled = compiled_program_from_artifact(&artifact);

        let err = RuntimeBuilder::new(compiled)
            .without_default_schemes()
            .build()
            .expect_err("missing scheme bundle should fail");

        match err {
            RuntimeError::ValidationFailed { detail } => {
                assert!(detail.contains("no runtime scheme factory registered"));
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn precompile_registration_rejects_handler_id_mismatch() {
        struct WrongIdHandler;

        impl PrecompileHandler for WrongIdHandler {
            fn id(&self) -> PrecompileId {
                PrecompileId(0x0002)
            }

            fn execute(
                &self,
                _inputs: &[tabula_core::Value],
            ) -> Result<Vec<tabula_core::Value>, TabulaError> {
                Ok(vec![])
            }
        }

        let descriptor = PrecompileDescriptor::from_labels(
            PrecompileId(0x0001),
            1,
            "dummy.params",
            "dummy.semantic",
        );
        let err = PrecompileBundle::verification(
            descriptor.clone(),
            DummyPrecompileProofFactory::new(descriptor),
        )
        .expect("verification-only bundle")
        .with_handler(WrongIdHandler)
        .err()
        .expect("id mismatch should fail");

        match err {
            ExtError::Validation { detail } => {
                assert!(detail.contains("handler reports"));
            }
            other => panic!("unexpected error: {other}"),
        }
    }
}
