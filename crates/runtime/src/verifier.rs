//! Verifier-only runtime surface.
//!
//! A [`Verifier`] is built against one program binding plus the host-installed
//! capabilities needed to materialize the sealed artifact's proof surface. It
//! does not own execution-only registries such as property query handlers.

use std::collections::BTreeSet;
use std::sync::Arc;

use tabula_artifact::{Artifact, Statement};
use tabula_compiler::register_artifact;
use tabula_machine::{RootProofBackend, SmtRootProofBackend, TabulaMachine, TabulaProof};

use crate::bootstrap::machine::{
    attach_builtin_execution_backends, attach_execution_backend, build_machine_builder,
    supported_root_binding_families,
};
use crate::bootstrap::materialize::{
    materialize_column_backends, materialize_precompile_verifier_systems,
};
use crate::bootstrap::validation::{
    validate_compiler_owned_profiles, validate_precompile_requirements, validate_statement_binding,
};
use crate::error::RuntimeError;
use crate::host::HostEnvironment;
use crate::program::{Binding, binding_from_artifact};

/// Verify a proof against an expected program binding and low-level machine verifier.
pub(crate) fn verify_with_binding(
    binding: &Binding,
    machine: &TabulaMachine,
    proof: &TabulaProof,
    statement: &Statement,
) -> Result<(), RuntimeError> {
    validate_statement_binding(
        statement,
        &proof.statement_digest,
        binding.program_hash(),
        binding.metadata_hash(),
    )?;

    machine.verify(proof).map_err(RuntimeError::Verification)
}

/// Verifier built once per program binding.
pub struct Verifier {
    binding: Binding,
    machine: TabulaMachine,
}

/// Fluent builder for [`Verifier`].
pub struct VerifierBuilder {
    artifact: Artifact,
    host_environment: HostEnvironment,
    machine_stark_config: tabula_machine::TabulaStarkConfig,
    root_proof_backend: Arc<dyn RootProofBackend>,
}

impl Verifier {
    /// Create a builder for verifier-only preparation from a sealed artifact.
    pub fn builder(artifact: Artifact) -> VerifierBuilder {
        VerifierBuilder::new(artifact)
    }

    /// The canonical binding this verifier is pinned to.
    pub fn binding(&self) -> &Binding {
        &self.binding
    }

    /// The STARK machine backing verification.
    pub fn machine(&self) -> &TabulaMachine {
        &self.machine
    }

    /// Verify a proof against this verifier's sealed artifact and expected statement.
    pub fn verify(&self, proof: &TabulaProof, statement: &Statement) -> Result<(), RuntimeError> {
        verify_with_binding(&self.binding, &self.machine, proof, statement)
    }
}

impl VerifierBuilder {
    fn new(artifact: Artifact) -> Self {
        Self {
            artifact,
            host_environment: HostEnvironment::standard(),
            machine_stark_config: tabula_machine::default_config(),
            root_proof_backend: Arc::new(SmtRootProofBackend),
        }
    }

    /// Replace the host-installed runtime capabilities used for verification.
    pub fn with_host_environment(mut self, host_environment: HostEnvironment) -> Self {
        self.host_environment = host_environment;
        self
    }

    /// Override the STARK configuration used by the machine verifier.
    pub fn with_machine_stark_config(
        mut self,
        machine_stark_config: tabula_machine::TabulaStarkConfig,
    ) -> Self {
        self.machine_stark_config = machine_stark_config;
        self
    }

    /// Override the proof-side root backend.
    pub fn with_root_proof_backend(
        mut self,
        root_proof_backend: impl RootProofBackend + 'static,
    ) -> Self {
        self.root_proof_backend = Arc::new(root_proof_backend);
        self
    }

    /// Override the proof-side root backend using a shared backend object.
    pub fn with_root_proof_backend_arc(
        mut self,
        root_proof_backend: Arc<dyn RootProofBackend>,
    ) -> Self {
        self.root_proof_backend = root_proof_backend;
        self
    }

    /// Build the verifier, validating artifact semantics and machine-side requirements.
    pub fn build(self) -> Result<Verifier, RuntimeError> {
        let binding = binding_from_artifact(&self.artifact)?;
        let compiled_program =
            register_artifact(&self.artifact).map_err(RuntimeError::CompilerValidation)?;

        self.validate(&compiled_program)?;

        let resolved_columns = materialize_column_backends(
            &compiled_program,
            self.host_environment.schemes().factories(),
            self.host_environment.runtime_registries().type_runtimes(),
            self.host_environment
                .runtime_registries()
                .encoding_runtimes(),
            supported_root_binding_families(&self.root_proof_backend),
        )?;
        let precompile_systems = materialize_precompile_verifier_systems(
            &self.artifact.precompile_manifest,
            self.host_environment.precompiles().factories(),
        )?;
        let mut machine_builder = build_machine_builder(
            &self.machine_stark_config,
            Arc::clone(&self.root_proof_backend),
        );
        machine_builder = attach_builtin_execution_backends(
            machine_builder,
            compiled_program.program(),
            !self.artifact.precompile_manifest.is_empty(),
        );
        for system in precompile_systems {
            machine_builder = attach_execution_backend(machine_builder, system.system);
        }
        let machine = machine_builder
            .with_columns(
                resolved_columns
                    .column_backends
                    .into_values()
                    .map(|backend| backend.proof_column),
            )
            .build()
            .map_err(RuntimeError::MachineSetup)?;

        Ok(Verifier { binding, machine })
    }

    fn validate(
        &self,
        sealed_program: &tabula_compiler::SealedProgram,
    ) -> Result<(), RuntimeError> {
        validate_compiler_owned_profiles(sealed_program)?;
        let installed = self
            .host_environment
            .precompiles()
            .factories()
            .keys()
            .copied()
            .collect::<BTreeSet<_>>();
        validate_precompile_requirements(sealed_program, &installed, "precompile backend")?;
        Ok(())
    }
}

impl std::fmt::Debug for Verifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Verifier")
            .field("binding", &self.binding)
            .field("machine", &self.machine)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::RuntimeError;
    #[cfg(feature = "verify")]
    use tabula_core::{ColId, SchemeId, TableId, TxTypeId};
    #[cfg(feature = "prove")]
    use tabula_ext::root::{
        PreparedRootWitness, RootBackend, RootBackendBundle, RootWitnessContext,
        RootWitnessPreparer,
    };
    #[cfg(feature = "prove")]
    use tabula_testing::assertions::assert_statement_matches_artifact;
    #[cfg(feature = "verify")]
    use tabula_testing::exec::compiled_program_from_definition;
    #[cfg(feature = "verify")]
    use tabula_testing::fixtures::artifacts::precompile_requirement_artifact;
    #[cfg(feature = "prove")]
    use tabula_testing::fixtures::compiled::compiled_hash_only_case;
    #[cfg(feature = "verify")]
    use tabula_testing::fixtures::schema::single_u64_column_schema;

    #[cfg(feature = "verify")]
    use tabula_ir::TxTypeDef;
    #[cfg(feature = "verify")]
    use tabula_machine::{RootProofBackend, SmtRootProofBackend};
    #[cfg(feature = "prove")]
    use tabula_testing::fixtures::examples::{
        transfer_example_artifact_case, transfer_example_compiled_case,
    };
    #[cfg(feature = "prove")]
    use tabula_testing::runtime::prove_compiled_case;

    use super::Verifier;
    #[cfg(feature = "verify")]
    use crate::host::{HostEnvironment, RuntimeRegistries};
    #[cfg(feature = "verify")]
    use crate::testing::schemes::{
        EmptySchemeFactory, custom_scheme_profile, set_artifact_column_scheme,
    };
    #[cfg(feature = "verify")]
    use tabula_ext::ColumnBackendFactoryBundle;

    #[cfg(feature = "verify")]
    #[derive(Clone, Copy, Debug)]
    struct DelegatingRootProofBackend;

    #[cfg(feature = "verify")]
    impl RootProofBackend for DelegatingRootProofBackend {
        fn name(&self) -> &str {
            "delegating_root_proof"
        }

        fn supported_root_binding_families(&self) -> &'static [tabula_core::RootProfileId] {
            SmtRootProofBackend.supported_root_binding_families()
        }

        fn airs(&self) -> Vec<Box<dyn tabula_machine::backend::AnyRap>> {
            SmtRootProofBackend.airs()
        }

        fn dyn_chips(&self) -> Vec<Box<dyn tabula_stark::trace::DynChip>> {
            SmtRootProofBackend.dyn_chips()
        }
    }

    #[cfg(feature = "prove")]
    #[derive(Debug)]
    struct FailingRootWitnessPreparer;

    #[cfg(feature = "prove")]
    impl RootWitnessPreparer for FailingRootWitnessPreparer {
        fn name(&self) -> &str {
            "failing_root"
        }

        fn prepare_root_witness(
            &self,
            _context: RootWitnessContext<'_>,
        ) -> Result<PreparedRootWitness, tabula_ext::ExtError> {
            Err(tabula_ext::ExtError::validation("should not be called"))
        }
    }

    #[cfg(feature = "prove")]
    #[derive(Clone, Copy, Debug)]
    struct FailingRootBackend;

    #[cfg(feature = "prove")]
    impl RootBackend for FailingRootBackend {
        fn name(&self) -> &str {
            "failing_root_backend"
        }

        fn proof_backend(&self) -> Arc<dyn RootProofBackend> {
            Arc::new(DelegatingRootProofBackend)
        }

        fn witness_preparer(&self) -> Arc<dyn RootWitnessPreparer> {
            Arc::new(FailingRootWitnessPreparer)
        }
    }

    #[cfg(feature = "verify")]
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

    #[cfg(feature = "verify")]
    #[test]
    fn program_verifier_rejects_missing_required_precompile() {
        let err = Verifier::builder(precompile_requirement_artifact())
            .build()
            .expect_err("missing verifier precompile should fail");

        match err {
            RuntimeError::ValidationFailed { detail } => {
                assert!(detail.contains("precompile backend"));
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[cfg(feature = "verify")]
    #[test]
    fn program_verifier_supports_custom_only_host_environment() {
        let compiled = compiled_single_column_noop_program();
        let mut artifact = compiled.into_artifact();
        set_artifact_column_scheme(&mut artifact, 0, custom_scheme_profile(SchemeId(0x1000)));
        let host_environment = HostEnvironment::empty()
            .with_runtime_registries(RuntimeRegistries::standard())
            .with_column_backend_bundle(ColumnBackendFactoryBundle::new(EmptySchemeFactory))
            .expect("register custom backend bundle");
        let verifier = Verifier::builder(artifact)
            .with_host_environment(host_environment)
            .build()
            .expect("custom-only verifier");

        assert_eq!(verifier.binding().metadata_hash().len(), 64);
    }

    #[cfg(feature = "verify")]
    #[test]
    fn program_verifier_allows_custom_root_proof_backend() {
        let verifier = Verifier::builder(compiled_single_column_noop_program().into_artifact())
            .with_root_proof_backend(DelegatingRootProofBackend)
            .build()
            .expect("verifier should accept custom proof-side root backends");

        assert_eq!(verifier.binding().metadata_hash().len(), 64);
    }

    #[cfg(feature = "prove")]
    #[test]
    fn program_verifier_accepts_runtime_proof() {
        let case = transfer_example_artifact_case();
        let proved = prove_compiled_case(&transfer_example_compiled_case());

        let verifier = Verifier::builder(case.artifact.clone())
            .build()
            .expect("program verifier");

        assert_statement_matches_artifact(&proved.statement, &case.artifact);
        verifier
            .verify(&proved.proof, &proved.statement)
            .expect("verification succeeds");
    }

    #[cfg(feature = "prove")]
    #[test]
    fn program_verifier_accepts_runtime_proof_for_hash_only_program() {
        let case = compiled_hash_only_case();
        let artifact = case.compiled_program.as_artifact();
        let proved = prove_compiled_case(&case);

        let verifier = Verifier::builder(artifact.clone())
            .build()
            .expect("program verifier");

        assert_statement_matches_artifact(&proved.statement, &artifact);
        verifier
            .verify(&proved.proof, &proved.statement)
            .expect("verification succeeds");
    }

    #[cfg(feature = "prove")]
    #[test]
    fn program_verifier_rejects_statement_program_hash_mismatch() {
        let case = transfer_example_artifact_case();
        let proved = prove_compiled_case(&transfer_example_compiled_case());

        let verifier = Verifier::builder(case.artifact.clone())
            .build()
            .expect("program verifier");
        let mut statement = proved.statement.clone();
        statement.program_hash = "00".repeat(32);

        let err = verifier
            .verify(&proved.proof, &statement)
            .expect_err("mismatched statement binding must fail");

        assert!(
            err.to_string().contains("program hash"),
            "expected program hash mismatch, got: {err}",
        );
    }

    #[cfg(feature = "prove")]
    #[test]
    fn program_verifier_ignores_root_witness_preparer_bundle() {
        let case = transfer_example_artifact_case();
        let proved = prove_compiled_case(&transfer_example_compiled_case());
        let bundle = RootBackendBundle::new(FailingRootBackend);

        let verifier = Verifier::builder(case.artifact.clone())
            .with_root_proof_backend_arc(bundle.proof_backend())
            .build()
            .expect("verifier build should ignore witness preparers");

        verifier
            .verify(&proved.proof, &proved.statement)
            .expect("verification succeeds without witness preparation");
    }
}
