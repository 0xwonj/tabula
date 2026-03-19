//! Verifier-only runtime surface.
//!
//! A [`ProgramVerifier`] is built against one program binding plus any
//! required proving-side extensions. It does not own execution-only resources
//! such as precompile handlers or property resolvers.

use std::collections::BTreeMap;

use tabula_artifact::{ExecutionStatement, ProgramArtifact};
use tabula_compiler::register_program_artifact;
use tabula_ir::PrecompileId;
use tabula_machine::{ChipExtension, TabulaMachine, TabulaProof};

use crate::assembly::build_base::BuildBase;
use crate::assembly::materialize::resolve_proof_columns_with_factories;
use crate::assembly::validation::{
    validate_compiler_owned_proof_plan, validate_precompile_requirements,
    validate_statement_binding,
};
use crate::columns::ColumnSchemeFactory;
use crate::error::RuntimeError;
use crate::program::ProgramBinding;

/// Verify a proof against an expected program binding and low-level machine verifier.
pub(crate) fn verify_with_binding(
    binding: &ProgramBinding,
    machine: &TabulaMachine,
    proof: &TabulaProof,
    statement: &ExecutionStatement,
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
pub struct ProgramVerifier {
    binding: ProgramBinding,
    machine: TabulaMachine,
}

/// Fluent builder for [`ProgramVerifier`].
///
/// Collects verifier-side extensions and machine configuration, then prepares
/// the proving backend for repeated verification against one sealed program.
pub struct ProgramVerifierBuilder {
    program_artifact: ProgramArtifact,
    base: BuildBase,
    precompile_verifiers: BTreeMap<PrecompileId, Box<dyn ChipExtension>>,
}

impl ProgramVerifier {
    /// Create a builder for verifier-only preparation from a sealed program artifact.
    pub fn builder(program_artifact: ProgramArtifact) -> ProgramVerifierBuilder {
        ProgramVerifierBuilder::new(program_artifact)
    }

    /// The canonical binding this verifier is pinned to.
    pub fn binding(&self) -> &ProgramBinding {
        &self.binding
    }

    /// The STARK machine backing verification.
    pub fn machine(&self) -> &TabulaMachine {
        &self.machine
    }

    /// Verify a proof against this verifier's sealed program artifact and expected statement.
    pub fn verify(
        &self,
        proof: &TabulaProof,
        statement: &ExecutionStatement,
    ) -> Result<(), RuntimeError> {
        verify_with_binding(&self.binding, &self.machine, proof, statement)
    }
}

impl ProgramVerifierBuilder {
    fn new(program_artifact: ProgramArtifact) -> Self {
        Self {
            program_artifact,
            base: BuildBase::new(),
            precompile_verifiers: BTreeMap::new(),
        }
    }

    /// Register a verifier-side precompile extension.
    pub fn with_precompile(
        mut self,
        id: PrecompileId,
        verifier: impl ChipExtension + 'static,
    ) -> Result<Self, RuntimeError> {
        if self
            .precompile_verifiers
            .insert(id, Box::new(verifier))
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

    /// Override the root proof scheme (default: two-level SMT).
    pub fn with_root_proof(mut self, root: impl tabula_machine::RootProof + 'static) -> Self {
        self.base = self.base.with_root_proof(root);
        self
    }

    /// Override the STARK configuration.
    pub fn with_config(mut self, config: tabula_machine::TabulaStarkConfig) -> Self {
        self.base = self.base.with_config(config);
        self
    }

    /// Build the verifier, validating artifact semantics and machine-side requirements.
    pub fn build(self) -> Result<ProgramVerifier, RuntimeError> {
        let binding = ProgramBinding::from_program_artifact(&self.program_artifact)?;
        let compiled_program = register_program_artifact(&self.program_artifact)
            .map_err(RuntimeError::CompilerValidation)?;

        self.validate(&compiled_program)?;

        let columns = resolve_proof_columns_with_factories(
            &compiled_program,
            self.base.scheme_factories(),
            self.base.root_profile_id(),
        )?;
        let (mut machine_builder, _scheme_factories) = self.base.into_parts();
        for verifier in self.precompile_verifiers.into_values() {
            machine_builder = machine_builder.with_extension_boxed(verifier);
        }
        let machine = machine_builder
            .with_columns(columns)
            .build()
            .map_err(RuntimeError::MachineSetup)?;

        Ok(ProgramVerifier { binding, machine })
    }

    fn validate(
        &self,
        compiled_program: &tabula_compiler::CompiledProgram,
    ) -> Result<(), RuntimeError> {
        validate_compiler_owned_proof_plan(compiled_program)?;
        let registered_ids = self.precompile_verifiers.keys().copied().collect();
        validate_precompile_requirements(compiled_program, &registered_ids, "verifier extension")?;
        Ok(())
    }
}

impl std::fmt::Debug for ProgramVerifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProgramVerifier")
            .field("binding", &self.binding)
            .field("machine", &self.machine)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use tabula_compiler::register_program;
    use tabula_core::{ColId, TableId, TableSchema, TxTypeId, ValueType};
    use tabula_ir::{Instruction, PrecompileId, TxTypeDef};

    use crate::RuntimeError;

    #[cfg(feature = "prove")]
    use crate::{ProveInput, TabulaRuntime};
    #[cfg(feature = "prove")]
    use tabula_compiler::{register_program_artifact, transfer_example_bundle};

    use super::ProgramVerifier;

    #[cfg(feature = "verify")]
    fn precompile_program_artifact() -> tabula_artifact::ProgramArtifact {
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
            body: vec![Instruction::Precompile {
                id: PrecompileId(0x0001),
                dst_slots: vec![0],
                inputs: vec![],
            }],
        };

        register_program(&[schema], &[tx])
            .expect("register program")
            .into_program_artifact()
    }

    #[cfg(feature = "verify")]
    #[test]
    fn program_verifier_rejects_missing_required_precompile() {
        let err = ProgramVerifier::builder(precompile_program_artifact())
            .build()
            .expect_err("missing verifier precompile should fail");

        match err {
            RuntimeError::ValidationFailed { detail } => {
                assert!(detail.contains("precompile"));
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[cfg(feature = "prove")]
    #[test]
    fn program_verifier_accepts_runtime_proof() {
        let bundle = transfer_example_bundle().expect("example bundle");
        let compiled = register_program_artifact(&bundle.program).expect("compiled program");
        let runtime = TabulaRuntime::builder(compiled).build().expect("runtime");
        let executed = runtime
            .execute(&bundle.state, &bundle.batch)
            .expect("execution succeeds");
        let proved = runtime
            .prove(&ProveInput {
                state: &bundle.state,
                batch: &bundle.batch,
                executed: &executed,
            })
            .expect("proof succeeds");

        let verifier = ProgramVerifier::builder(bundle.program.clone())
            .build()
            .expect("program verifier");

        verifier
            .verify(&proved.proof, &proved.statement)
            .expect("verification succeeds");
    }

    #[cfg(feature = "prove")]
    #[test]
    fn program_verifier_rejects_statement_program_hash_mismatch() {
        let bundle = transfer_example_bundle().expect("example bundle");
        let compiled = register_program_artifact(&bundle.program).expect("compiled program");
        let runtime = TabulaRuntime::builder(compiled).build().expect("runtime");
        let executed = runtime
            .execute(&bundle.state, &bundle.batch)
            .expect("execution succeeds");
        let proved = runtime
            .prove(&ProveInput {
                state: &bundle.state,
                batch: &bundle.batch,
                executed: &executed,
            })
            .expect("proof succeeds");

        let verifier = ProgramVerifier::builder(bundle.program.clone())
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
