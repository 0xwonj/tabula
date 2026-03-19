//! Fluent builder for [`TabulaRuntime`](crate::TabulaRuntime) construction.
//!
//! The runtime owns scheme registry resolution. Built-in and custom schemes
//! are both installed as [`ColumnSchemeFactory`](crate::ColumnSchemeFactory)
//! values, then materialized into runtime/proof column views during `build()`.

use std::collections::BTreeSet;

use tabula_compiler::CompiledProgram;
use tabula_core::TableId;
use tabula_ir::PrecompileId;
use tabula_machine::{ChipExtension, RootProof, TabulaStarkConfig};

use crate::assembly::build_base::BuildBase;
use crate::assembly::materialize::resolve_column_views_with_factories;
use crate::assembly::registries::{build_precompile_registry, build_property_query_registry};
use crate::assembly::validation::{
    validate_compiler_owned_proof_plan, validate_precompile_requirements,
};
use crate::capabilities::PrecompileRegistration;
use crate::columns::ColumnSchemeFactory;
use crate::error::RuntimeError;
use crate::program::RuntimeProgram;
use crate::runtime::TabulaRuntime;

/// Fluent builder for [`TabulaRuntime`](crate::TabulaRuntime).
pub struct RuntimeBuilder {
    compiled_program: CompiledProgram,
    base: BuildBase,
    precompile_registrations: Vec<PrecompileRegistration>,
}

impl RuntimeBuilder {
    /// Create a builder with a compiler-produced program artifact.
    pub(crate) fn new(compiled_program: CompiledProgram) -> Self {
        Self {
            compiled_program,
            base: BuildBase::new(),
            precompile_registrations: Vec::new(),
        }
    }

    /// Register a precompile capability as one logical unit.
    pub fn with_precompile(
        mut self,
        registration: PrecompileRegistration,
    ) -> Result<Self, RuntimeError> {
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

    /// Register a machine-only chip extension.
    pub fn with_extension(mut self, ext: impl ChipExtension + 'static) -> Self {
        self.base = self.base.with_extension(ext);
        self
    }

    /// Register a custom or replacement scheme factory.
    pub fn with_scheme(
        mut self,
        factory: impl ColumnSchemeFactory + 'static,
    ) -> Result<Self, RuntimeError> {
        self.base = self.base.with_scheme(factory)?;
        Ok(self)
    }

    /// Override the root proof scheme.
    pub fn with_root_proof(mut self, root: impl RootProof + 'static) -> Self {
        self.base = self.base.with_root_proof(root);
        self
    }

    /// Override the STARK configuration.
    pub fn with_config(mut self, config: TabulaStarkConfig) -> Self {
        self.base = self.base.with_config(config);
        self
    }

    /// Build the runtime, materializing per-column schemes before machine setup.
    pub fn build(self) -> Result<TabulaRuntime, RuntimeError> {
        self.validate()?;

        let resolved_columns = resolve_column_views_with_factories(
            &self.compiled_program,
            self.base.scheme_factories(),
            self.base.root_profile_id(),
        )?;
        let proof_columns = resolved_columns.proof_columns.clone();
        let runtime_program =
            RuntimeProgram::from_compiled_program(&self.compiled_program, resolved_columns)?;

        let (machine_builder, _scheme_factories) = self.base.into_parts();
        let mut machine_builder = machine_builder.with_columns(proof_columns);

        let mut precompile_handlers = Vec::with_capacity(self.precompile_registrations.len());
        for registration in self.precompile_registrations {
            let (_id, handler, verifier) = registration.into_parts();
            precompile_handlers.push(handler);
            machine_builder = machine_builder.with_extension_boxed(verifier);
        }

        let machine = machine_builder
            .build()
            .map_err(RuntimeError::MachineSetup)?;
        let precompiles = build_precompile_registry(precompile_handlers)?;
        let property_queries = build_property_query_registry(
            runtime_program.runtime_columns(),
            runtime_program.column_plans(),
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

    fn registered_precompile_ids(&self) -> BTreeSet<PrecompileId> {
        self.precompile_registrations
            .iter()
            .map(PrecompileRegistration::id)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tabula_artifact::SchemeDescriptor;
    use tabula_compiler::register_program;
    use tabula_core::error::TabulaError;
    use tabula_core::{
        ColId, ColumnLayoutKind, RootProfileId, SchemeId, TableId, TableSchema, TxTypeId, ValueType,
    };
    use tabula_executor::precompile::PrecompileHandler;
    use tabula_ir::{AggregateKind, Instruction, PrecompileId, PropertyQuery, TxTypeDef};
    use tabula_machine::prelude::{ChipIdAllocator, DynChip};
    use tabula_machine::{ChipExtension, ColumnChipSet, ProofColumn, SetupError};

    use super::RuntimeBuilder;
    use crate::error::RuntimeError;
    use crate::{
        ColumnPlan, ColumnProofInput, ColumnSchemeFactory, ColumnTransitionBackend,
        ColumnTransitionInput, ColumnViews, RuntimeColumn,
    };

    fn custom_descriptor(scheme_id: SchemeId) -> SchemeDescriptor {
        SchemeDescriptor {
            scheme_id,
            scheme_version: 1,
            layout_kind: ColumnLayoutKind::SSMC_V1,
            params_hash: [scheme_id.raw() as u8; 32],
            root_profile_id: RootProfileId::SMT_V1,
            supported_property_query_kinds: vec![],
        }
    }

    fn unsupported_layout_descriptor(scheme_id: SchemeId) -> SchemeDescriptor {
        SchemeDescriptor {
            scheme_id,
            scheme_version: 1,
            layout_kind: ColumnLayoutKind(0x9000),
            params_hash: [scheme_id.raw() as u8; 32],
            root_profile_id: RootProfileId::SMT_V1,
            supported_property_query_kinds: vec![],
        }
    }

    fn compiled_program_with_property_query() -> tabula_compiler::CompiledProgram {
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
            name: "scan".to_string(),
            param_schema: vec![],
            body: vec![Instruction::PropertyRead {
                dst_val: 0,
                dst_key: 1,
                dst_is_null: 2,
                table: TableId(1),
                col: ColId(0),
                query: PropertyQuery::Successor {
                    key: tabula_core::RowKey(0),
                },
            }],
        };

        register_program(&[schema], &[tx]).expect("register program")
    }

    fn compiled_program_with_unsupported_property_query() -> tabula_compiler::CompiledProgram {
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
            name: "scan".to_string(),
            param_schema: vec![],
            body: vec![Instruction::PropertyRead {
                dst_val: 0,
                dst_key: 1,
                dst_is_null: 2,
                table: TableId(1),
                col: ColId(0),
                query: PropertyQuery::Aggregate {
                    kind: AggregateKind::Count,
                },
            }],
        };

        register_program(&[schema], &[tx]).expect("register program")
    }

    struct EmptyRuntimeColumn;

    impl RuntimeColumn for EmptyRuntimeColumn {
        fn name(&self) -> &str {
            "empty"
        }
    }

    struct EmptyProofColumn {
        plan: ColumnPlan,
    }

    impl ProofColumn for EmptyProofColumn {
        fn name(&self) -> &str {
            "empty"
        }

        fn table_id(&self) -> TableId {
            self.plan.table_id
        }

        fn col_id(&self) -> ColId {
            self.plan.col_id
        }

        fn scheme_id(&self) -> SchemeId {
            self.plan.scheme_id
        }

        fn create_chips(&self, _alloc: &mut ChipIdAllocator) -> Result<ColumnChipSet, SetupError> {
            Ok(ColumnChipSet {
                airs: vec![],
                dyn_chips: vec![],
                bus_consumers: vec![],
            })
        }
    }

    struct EmptyTransitionBackend {
        plan: ColumnPlan,
    }

    impl EmptyTransitionBackend {
        fn new(plan: ColumnPlan) -> Result<Self, SetupError> {
            if plan.scheme_descriptor.layout_kind != ColumnLayoutKind::SSMC_V1 {
                return Err(SetupError::SetupFailed(format!(
                    "unsupported transition layout {}",
                    plan.scheme_descriptor.layout_kind.0,
                )));
            }
            Ok(Self { plan })
        }
    }

    impl ColumnTransitionBackend for EmptyTransitionBackend {
        fn name(&self) -> &str {
            "empty"
        }

        fn table_id(&self) -> TableId {
            self.plan.table_id
        }

        fn col_id(&self) -> ColId {
            self.plan.col_id
        }

        fn scheme_id(&self) -> SchemeId {
            self.plan.scheme_id
        }

        fn build_proof_input(
            &self,
            _input: ColumnTransitionInput,
            _property_reads: &[tabula_witness::trace::builtin::PropertyReadRecord],
        ) -> Result<ColumnProofInput, TabulaError> {
            Err(TabulaError::ProofError {
                phase: "runtime_builder_test",
                detail: "empty transition backend should not be used in prove flow".to_string(),
            })
        }
    }

    struct EmptySchemeFactory;

    impl ColumnSchemeFactory for EmptySchemeFactory {
        fn descriptor(&self) -> SchemeDescriptor {
            custom_descriptor(SchemeId(0x1000))
        }

        fn name(&self) -> &str {
            "empty"
        }

        fn build_column(&self, plan: ColumnPlan) -> Result<ColumnViews, SetupError> {
            Ok(ColumnViews::new(
                Arc::new(EmptyRuntimeColumn),
                Arc::new(EmptyProofColumn { plan: plan.clone() }),
                Arc::new(EmptyTransitionBackend::new(plan)?),
            ))
        }
    }

    struct UnsupportedLayoutSchemeFactory;

    impl ColumnSchemeFactory for UnsupportedLayoutSchemeFactory {
        fn descriptor(&self) -> SchemeDescriptor {
            unsupported_layout_descriptor(SchemeId(0x1000))
        }

        fn name(&self) -> &str {
            "unsupported_layout"
        }

        fn build_column(&self, plan: ColumnPlan) -> Result<ColumnViews, SetupError> {
            let transition = Arc::new(EmptyTransitionBackend::new(plan.clone())?);
            Ok(ColumnViews::new(
                Arc::new(EmptyRuntimeColumn),
                Arc::new(EmptyProofColumn { plan: plan.clone() }),
                transition,
            ))
        }
    }

    #[derive(Clone)]
    struct UnsupportedPropertyRuntimeColumn;

    impl RuntimeColumn for UnsupportedPropertyRuntimeColumn {
        fn name(&self) -> &str {
            "unsupported"
        }
    }

    struct UnsupportedPropertyProofColumn {
        plan: ColumnPlan,
    }

    impl ProofColumn for UnsupportedPropertyProofColumn {
        fn name(&self) -> &str {
            "unsupported"
        }

        fn table_id(&self) -> TableId {
            self.plan.table_id
        }

        fn col_id(&self) -> ColId {
            self.plan.col_id
        }

        fn scheme_id(&self) -> SchemeId {
            self.plan.scheme_id
        }

        fn create_chips(&self, _alloc: &mut ChipIdAllocator) -> Result<ColumnChipSet, SetupError> {
            Ok(ColumnChipSet {
                airs: vec![],
                dyn_chips: vec![],
                bus_consumers: vec![],
            })
        }
    }

    struct UnsupportedPropertyTransitionBackend {
        plan: ColumnPlan,
    }

    impl ColumnTransitionBackend for UnsupportedPropertyTransitionBackend {
        fn name(&self) -> &str {
            "unsupported"
        }

        fn table_id(&self) -> TableId {
            self.plan.table_id
        }

        fn col_id(&self) -> ColId {
            self.plan.col_id
        }

        fn scheme_id(&self) -> SchemeId {
            self.plan.scheme_id
        }

        fn build_proof_input(
            &self,
            _input: ColumnTransitionInput,
            _property_reads: &[tabula_witness::trace::builtin::PropertyReadRecord],
        ) -> Result<ColumnProofInput, TabulaError> {
            Err(TabulaError::ProofError {
                phase: "runtime_builder_test",
                detail: "unsupported transition backend should not be used in prove flow"
                    .to_string(),
            })
        }
    }

    struct UnsupportedPropertySchemeFactory;

    impl ColumnSchemeFactory for UnsupportedPropertySchemeFactory {
        fn descriptor(&self) -> SchemeDescriptor {
            custom_descriptor(SchemeId(0x1001))
        }

        fn name(&self) -> &str {
            "unsupported"
        }

        fn build_column(&self, plan: ColumnPlan) -> Result<ColumnViews, SetupError> {
            if !plan.required_property_query_kinds.is_empty() {
                return Err(SetupError::SetupFailed(
                    "unsupported property query".to_string(),
                ));
            }
            Ok(ColumnViews::new(
                Arc::new(UnsupportedPropertyRuntimeColumn),
                Arc::new(UnsupportedPropertyProofColumn { plan: plan.clone() }),
                Arc::new(UnsupportedPropertyTransitionBackend { plan }),
            ))
        }
    }

    struct DummyVerifierExtension;

    impl ChipExtension for DummyVerifierExtension {
        fn name(&self) -> &str {
            "dummy_verifier"
        }

        fn airs(&self) -> Vec<Box<dyn tabula_machine::AnyRap>> {
            vec![]
        }

        fn dyn_chips(&self) -> Vec<Box<dyn DynChip>> {
            vec![]
        }
    }

    #[test]
    fn custom_scheme_factory_resolves_transition_backend() {
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
        compiled.as_program_artifact();
        let plan = compiled.column_proof_plan()[0].clone();
        let mut artifact = compiled.into_program_artifact();
        artifact.column_proof_plan[0].scheme_id = SchemeId(0x1000);
        artifact.column_proof_plan[0].scheme_descriptor = custom_descriptor(SchemeId(0x1000));
        let compiled = tabula_compiler::register_program_artifact(&artifact).expect("compiled");

        let runtime = RuntimeBuilder::new(compiled)
            .with_scheme(EmptySchemeFactory)
            .expect("register custom scheme")
            .build()
            .expect("runtime");

        assert_eq!(runtime.runtime_program().runtime_columns().len(), 1);
        assert_eq!(runtime.runtime_program().transition_backends().len(), 1);
        let plan = runtime
            .runtime_program()
            .column_plans()
            .get(&(plan.table_id, plan.col_id))
            .expect("column plan");
        assert_eq!(plan.scheme_id, SchemeId(0x1000));
    }

    #[test]
    fn runtime_rejects_explicit_state_backend_for_unsupported_layout() {
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
        let mut artifact = compiled.into_program_artifact();
        artifact.column_proof_plan[0].scheme_id = SchemeId(0x1000);
        artifact.column_proof_plan[0].scheme_descriptor =
            unsupported_layout_descriptor(SchemeId(0x1000));
        let compiled = tabula_compiler::register_program_artifact(&artifact).expect("compiled");

        let err = RuntimeBuilder::new(compiled)
            .with_scheme(UnsupportedLayoutSchemeFactory)
            .expect("register custom scheme")
            .build()
            .expect_err("unsupported layout should fail");

        match err {
            RuntimeError::MachineSetup(SetupError::SetupFailed(detail)) => {
                assert!(detail.contains("unsupported transition layout"));
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn built_in_runtime_registers_transition_backends() {
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

        assert_eq!(runtime.runtime_program().runtime_columns().len(), 1);
        assert_eq!(runtime.runtime_program().transition_backends().len(), 1);
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
        let mut artifact = compiled.into_program_artifact();
        artifact.column_proof_plan[0].scheme_id = SchemeId(0x1000);
        artifact.column_proof_plan[0].scheme_descriptor = custom_descriptor(SchemeId(0x1000));
        let compiled = tabula_compiler::register_program_artifact(&artifact).expect("compiled");

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
        let mut artifact = compiled.into_program_artifact();
        artifact.column_proof_plan[0].scheme_id = SchemeId(0x1001);
        artifact.column_proof_plan[0].scheme_descriptor = custom_descriptor(SchemeId(0x1001));
        let compiled = tabula_compiler::register_program_artifact(&artifact).expect("compiled");

        let err = RuntimeBuilder::new(compiled)
            .with_scheme(UnsupportedPropertySchemeFactory)
            .expect("register custom scheme")
            .build()
            .expect_err("unsupported property requirement should fail");

        match err {
            RuntimeError::MachineSetup(SetupError::SetupFailed(detail)) => {
                assert!(detail.contains("unsupported property"));
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn runtime_rejects_duplicate_scheme_factory_ids() {
        let compiled = compiled_program_with_property_query();
        let result = RuntimeBuilder::new(compiled)
            .with_scheme(EmptySchemeFactory)
            .expect("first")
            .with_scheme(EmptySchemeFactory);

        match result {
            Err(err) => match err {
                RuntimeError::ValidationFailed { detail } => {
                    assert!(detail.contains("duplicate scheme factory"));
                }
                other => panic!("unexpected error: {other}"),
            },
            Ok(_) => panic!("duplicate scheme id should fail"),
        }
    }

    #[test]
    fn runtime_materializes_matching_transition_backends() {
        let compiled = compiled_program_with_property_query();
        let runtime = RuntimeBuilder::new(compiled).build().expect("runtime");

        assert_eq!(
            runtime.runtime_program().runtime_columns().len(),
            runtime.runtime_program().transition_backends().len()
        );
        assert_eq!(
            runtime.runtime_program().runtime_columns().len(),
            runtime.runtime_program().transition_backends().len()
        );

        for (&(table_id, col_id), backend) in runtime.runtime_program().transition_backends() {
            assert_eq!(backend.table_id(), table_id);
            assert_eq!(backend.col_id(), col_id);
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

        let err = crate::PrecompileRegistration::new(
            PrecompileId(0x0001),
            WrongIdHandler,
            DummyVerifierExtension,
        )
        .expect_err("id mismatch should fail");

        match err {
            RuntimeError::ValidationFailed { detail } => {
                assert!(detail.contains("handler reports"));
            }
            other => panic!("unexpected error: {other}"),
        }
    }
}
