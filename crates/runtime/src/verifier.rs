//! Verifier-only runtime surface.
//!
//! A [`Verifier`] is built against one program binding plus any
//! required proving-side extensions. It does not own execution-only resources
//! such as precompile handlers or property resolvers.

use std::collections::BTreeMap;
use std::sync::Arc;

use tabula_artifact::{Artifact, PrecompileDescriptor, Statement};
use tabula_chips::precompile_transcript::PrecompileTranscriptChip;
use tabula_compiler::register_artifact;
use tabula_ext::{PrecompileBundle, SchemeBundle};
use tabula_ir::PrecompileId;
use tabula_machine::backend::AnyRap;
use tabula_machine::backend::extension::ExecutionTierExtension;
use tabula_machine::{TabulaMachine, TabulaProof};

use crate::error::RuntimeError;
use crate::precompile_proofs::{PrecompileProofFactory, PrecompileProofSystem};
use crate::program::{Binding, binding_from_artifact};
use crate::setup::builder_state::{MachineConfigBase, ProofRegistryBase};
use crate::setup::materialize::{
    materialize_precompile_proofs_with_factories, materialize_proof_slots_with_factories,
};
use crate::setup::planning::derive_column_plans;
use crate::setup::validation::{
    validate_compiler_owned_proof_plan, validate_precompile_requirements,
    validate_statement_binding,
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
///
/// Collects verifier-side extensions and machine configuration, then prepares
/// the proving backend for repeated verification against one sealed program.
pub struct VerifierBuilder {
    artifact: Artifact,
    machine_base: MachineConfigBase,
    proof_registry: ProofRegistryBase,
    precompile_factories: BTreeMap<PrecompileId, Arc<dyn PrecompileProofFactory>>,
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
            machine_base: MachineConfigBase::new(),
            proof_registry: ProofRegistryBase::seeded(),
            precompile_factories: BTreeMap::new(),
        }
    }

    /// Register a verifier-side precompile extension.
    pub fn with_precompile(mut self, bundle: PrecompileBundle) -> Result<Self, RuntimeError> {
        let id = bundle.id();
        let proof_factory = bundle.into_proof_factory();
        if self
            .precompile_factories
            .insert(id, proof_factory)
            .is_some()
        {
            return Err(RuntimeError::ValidationFailed {
                detail: format!(
                    "duplicate verifier precompile registration for id 0x{:04x}",
                    id.0
                ),
            });
        }
        Ok(self)
    }

    /// Register one canonical custom scheme bundle.
    pub fn with_scheme_bundle(mut self, bundle: SchemeBundle) -> Result<Self, RuntimeError> {
        let scheme_id = bundle.scheme_id();
        let proof_factory = bundle.into_proof_factory();
        if self.proof_registry.contains(scheme_id) {
            return Err(RuntimeError::ValidationFailed {
                detail: format!("duplicate proof scheme registration for id {}", scheme_id.0),
            });
        }
        self.proof_registry.insert_arc(proof_factory)?;
        Ok(self)
    }

    /// Clear all preloaded standard proof schemes.
    pub fn without_default_schemes(mut self) -> Self {
        self.proof_registry = ProofRegistryBase::empty();
        self
    }

    /// Override the root proof scheme (default: two-level SMT).
    pub fn with_root_proof(mut self, root: impl tabula_machine::RootProof + 'static) -> Self {
        self.machine_base = self.machine_base.with_root_proof(root);
        self
    }

    /// Override the STARK configuration.
    pub fn with_config(mut self, config: tabula_machine::TabulaStarkConfig) -> Self {
        self.machine_base = self.machine_base.with_config(config);
        self
    }

    /// Build the verifier, validating artifact semantics and machine-side requirements.
    pub fn build(self) -> Result<Verifier, RuntimeError> {
        let binding = binding_from_artifact(&self.artifact)?;
        let compiled_program =
            register_artifact(&self.artifact).map_err(RuntimeError::CompilerValidation)?;

        self.validate(&compiled_program)?;

        let column_plans = derive_column_plans(&compiled_program)?;
        let columns = materialize_proof_slots_with_factories(
            &column_plans,
            self.proof_registry.factories(),
            self.machine_base.root_profile_id(),
        )?;
        let precompile_systems = materialize_precompile_proofs_with_factories(
            &self.artifact.precompile_manifest,
            &self.precompile_factories,
        )?;
        let mut machine_builder = self.machine_base.into_machine_builder();
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
            .with_columns(columns.into_iter().map(|slot| slot.proof_column))
            .build()
            .map_err(RuntimeError::MachineSetup)?;

        Ok(Verifier { binding, machine })
    }

    fn validate(
        &self,
        sealed_program: &tabula_compiler::SealedProgram,
    ) -> Result<(), RuntimeError> {
        validate_compiler_owned_proof_plan(sealed_program)?;
        let registered = self
            .precompile_factories
            .values()
            .map(|factory| {
                let descriptor = factory.descriptor();
                (descriptor.precompile_id, descriptor)
            })
            .collect::<BTreeMap<PrecompileId, PrecompileDescriptor>>();
        validate_precompile_requirements(sealed_program, &registered, "proof factory")?;
        Ok(())
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
    use tabula_compiler::register_program;
    #[cfg(feature = "verify")]
    use tabula_core::{ColId, SchemeId, TableId, TableSchema, TxTypeId, ValueType};
    #[cfg(feature = "verify")]
    use tabula_ext::SchemeBundle;
    #[cfg(feature = "prove")]
    use tabula_testing::assertions::assert_statement_matches_artifact;
    #[cfg(feature = "verify")]
    use tabula_testing::fixtures::artifacts::precompile_requirement_artifact;

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
    use crate::testing::prove::{
        EmptySchemeFactory, custom_descriptor, set_artifact_column_scheme,
    };

    #[cfg(feature = "verify")]
    #[test]
    fn program_verifier_rejects_missing_required_precompile() {
        let err = Verifier::builder(precompile_requirement_artifact())
            .build()
            .expect_err("missing verifier precompile should fail");

        match err {
            RuntimeError::ValidationFailed { detail } => {
                assert!(detail.contains("precompile"));
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[cfg(feature = "verify")]
    #[test]
    fn program_verifier_supports_custom_only_proof_registry() {
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
        let verifier = Verifier::builder(artifact)
            .without_default_schemes()
            .with_scheme_bundle(
                SchemeBundle::new(EmptySchemeFactory, EmptySchemeFactory)
                    .expect("empty scheme bundle"),
            )
            .expect("register custom proof bundle")
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
