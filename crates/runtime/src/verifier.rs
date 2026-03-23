//! Verifier-only runtime surface.
//!
//! A [`Verifier`] is built against one program binding plus the host-installed
//! capabilities needed to materialize the sealed artifact's proof surface. It
//! does not own execution-only registries such as property query handlers.

use std::collections::BTreeSet;
use std::sync::Arc;

use tabula_artifact::{Artifact, Statement};
use tabula_chips::ir_hash::IrHashChip;
use tabula_chips::poseidon::PoseidonChip;
use tabula_chips::precompile_transcript::PrecompileTranscriptChip;
use tabula_compiler::register_artifact;
use tabula_ext::backend::precompile::PrecompileProofSystem;
use tabula_ir::Instruction;
use tabula_machine::backend::AnyRap;
use tabula_machine::backend::extension::ExecutionTierExtension;
use tabula_machine::{TabulaMachine, TabulaProof};

use crate::error::RuntimeError;
use crate::host::HostEnvironment;
use crate::machine_config::MachineConfig;
use crate::program::{Binding, binding_from_artifact};
use crate::setup::materialize::{
    materialize_column_backends, materialize_precompile_verifier_systems,
};
use crate::setup::validation::{
    validate_compiler_owned_profiles, validate_precompile_requirements, validate_statement_binding,
};

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

    machine
        .verifier()
        .verify(proof)
        .map_err(RuntimeError::Verification)
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
    machine_config: MachineConfig,
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
            machine_config: MachineConfig::standard(),
        }
    }

    /// Replace the host-installed runtime capabilities used for verification.
    pub fn with_host_environment(mut self, host_environment: HostEnvironment) -> Self {
        self.host_environment = host_environment;
        self
    }

    /// Replace the machine-side verification configuration.
    pub fn with_machine_config(mut self, machine_config: MachineConfig) -> Self {
        self.machine_config = machine_config;
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
            self.host_environment.type_runtimes().type_runtimes(),
            self.host_environment.type_runtimes().encoding_runtimes(),
            self.machine_config.supported_root_binding_families(),
        )?;
        let precompile_systems = materialize_precompile_verifier_systems(
            &self.artifact.precompile_manifest,
            self.host_environment.precompiles().factories(),
        )?;
        let mut machine_builder = self.machine_config.build_machine_builder();
        if program_uses_ir_hash(compiled_program.program()) {
            machine_builder = machine_builder
                .with_backend_execution_extension_boxed(Box::new(InternalIrHashExtension));
        }
        if !self.artifact.precompile_manifest.is_empty() {
            machine_builder = machine_builder.with_backend_execution_extension_boxed(Box::new(
                InternalPrecompileTranscriptExtension,
            ));
        }
        for system in precompile_systems {
            machine_builder = machine_builder.with_backend_execution_extension_boxed(Box::new(
                InternalPrecompileExtension {
                    system: system.system,
                },
            ));
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
    use crate::RuntimeError;
    #[cfg(feature = "verify")]
    use tabula_core::{ColId, SchemeId, TableId, TxTypeId};
    #[cfg(feature = "prove")]
    use tabula_testing::assertions::assert_statement_matches_artifact;
    #[cfg(feature = "verify")]
    use tabula_testing::exec::compiled_program_from_definition;
    #[cfg(feature = "verify")]
    use tabula_testing::fixtures::artifacts::precompile_requirement_artifact;
    #[cfg(feature = "verify")]
    use tabula_testing::fixtures::schema::single_u64_column_schema;

    #[cfg(feature = "verify")]
    use tabula_ir::TxTypeDef;
    #[cfg(feature = "prove")]
    use tabula_testing::fixtures::examples::{
        transfer_example_artifact_case, transfer_example_compiled_case,
    };
    #[cfg(feature = "prove")]
    use tabula_testing::runtime::prove_compiled_case;

    use super::Verifier;
    #[cfg(feature = "verify")]
    use crate::host::{HostEnvironment, HostTypeRuntimes};
    #[cfg(feature = "verify")]
    use crate::testing::schemes::{
        EmptySchemeFactory, custom_scheme_profile, set_artifact_column_scheme,
    };
    #[cfg(feature = "verify")]
    use tabula_ext::ColumnBackendFactoryBundle;

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
            .with_type_runtimes(HostTypeRuntimes::standard())
            .with_column_backend_bundle(ColumnBackendFactoryBundle::new(EmptySchemeFactory))
            .expect("register custom backend bundle");
        let verifier = Verifier::builder(artifact)
            .with_host_environment(host_environment)
            .build()
            .expect("custom-only verifier");

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
}
