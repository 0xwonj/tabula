//! Native proof verification surface and shared verification core.

#[cfg(not(feature = "prove"))]
use std::sync::Arc;

use tabula_chips::event_transcript::EVENT_TRANSCRIPT_CHIP_ID;
use tabula_chips::public_context_transcript::PUBLIC_CONTEXT_TRANSCRIPT_CHIP_ID;
use tabula_chips::relation_table::RELATION_TABLE_CHIP_ID;
use tabula_chips::tx_batch_transcript::TX_BATCH_TRANSCRIPT_CHIP_ID;
use tabula_commitment::NativeDigest;
use tabula_compiler::RegisteredProgram;
use tabula_contract::{ArtifactContext, BoundStatement, ProgramBinding, PublicStatement};
use tabula_core::Digest;
#[cfg(feature = "prove")]
use tabula_ext::root::RootBackendBundle;
#[cfg(not(feature = "prove"))]
use tabula_ext::root::{RootProofBackend, SmtRootProofBackend};
use tabula_machine::{BackendVerifier, TabulaMachine, TabulaProof, TabulaStarkConfig};

use crate::bootstrap::program::{
    RelationPolicy, build_registered_program_machine, resolve_program_setup,
    validate_core_first_program,
};
use crate::error::RuntimeError;
use crate::host::HostEnvironment;

/// Prepared verifier state derived from the sealed artifact and machine setup.
///
/// Public so downstream consumers (SDK, tests, future prover) can name
/// the prepared-once state without going through a builder.
pub struct VerifierState {
    /// Artifact-bound transcript context sealed at prepare time.
    pub context: ArtifactContext,
    /// Relation-policy decision derived from program analysis.
    pub relation_policy: RelationPolicy,
    /// STARK machine backing verification.
    pub machine: TabulaMachine,
}

/// Verifier built once per registered native program.
///
/// Cheap to share via Arc; [`PreparedVerifier::verify`] takes
/// `&self` so callers can drive it from multiple threads.
pub struct PreparedVerifier {
    prepared: VerifierState,
}

/// Fluent builder for [`PreparedVerifier`].
pub struct PreparedVerifierBuilder {
    registered_program: RegisteredProgram,
    host_environment: HostEnvironment,
    machine_stark_config: TabulaStarkConfig,
    #[cfg(feature = "prove")]
    root_backend_bundle: RootBackendBundle,
    #[cfg(not(feature = "prove"))]
    root_proof_backend: Arc<dyn RootProofBackend>,
}

impl PreparedVerifier {
    /// Create a builder for one registered native program.
    pub fn builder(
        registered_program: RegisteredProgram,
    ) -> Result<PreparedVerifierBuilder, RuntimeError> {
        PreparedVerifierBuilder::new(registered_program)
    }

    /// Borrow the prepared verify-side state.
    pub fn state(&self) -> &VerifierState {
        &self.prepared
    }

    /// Borrow the transcript-bound program binding.
    pub fn binding(&self) -> &ProgramBinding {
        &self.prepared.context.binding
    }

    /// The STARK machine backing this verifier.
    pub fn machine(&self) -> &TabulaMachine {
        &self.prepared.machine
    }

    /// Verify one native proof against an externally supplied expected public
    /// statement and return the artifact-bound statement on success.
    pub fn verify(
        &self,
        proof: &TabulaProof,
        expected_public_statement: &PublicStatement,
    ) -> Result<BoundStatement, RuntimeError> {
        let bound = BoundStatement::new(
            self.prepared.context.clone(),
            expected_public_statement.clone(),
        );
        let expected_binding_digest =
            bound
                .binding_digest()
                .map_err(|error| RuntimeError::StatementBuild {
                    detail: error.to_string(),
                })?;
        if proof.binding_digest != expected_binding_digest {
            return Err(RuntimeError::ValidationFailed {
                detail: "proof binding digest does not match the artifact-bound public statement"
                    .to_string(),
            });
        }
        verify_proved_public_statement_digests(
            proof,
            &self.prepared.machine,
            expected_public_statement,
        )?;
        match relation_table_root_from_proof(proof, &self.prepared.machine)? {
            Some(root) if self.prepared.relation_policy.requires_artifact_root() => {
                if root != self.prepared.context.static_table_root {
                    return Err(RuntimeError::ValidationFailed {
                        detail: "relation table chip root does not match the verifier artifact"
                            .to_string(),
                    });
                }
            }
            None if self.prepared.relation_policy.requires_artifact_root() => {
                return Err(RuntimeError::ValidationFailed {
                    detail: "relation table chip opening is missing from the execution proof"
                        .to_string(),
                });
            }
            _ => {}
        }
        BackendVerifier::new(&self.prepared.machine)
            .verify_proof(proof)
            .map_err(RuntimeError::Verification)?;
        Ok(bound)
    }
}

impl PreparedVerifierBuilder {
    fn new(registered_program: RegisteredProgram) -> Result<Self, RuntimeError> {
        registered_program
            .validate_sealed_artifact()
            .map_err(RuntimeError::CompilerValidation)?;
        Ok(Self {
            registered_program,
            host_environment: HostEnvironment::standard()?,
            machine_stark_config: tabula_machine::default_config(),
            #[cfg(feature = "prove")]
            root_backend_bundle: RootBackendBundle::standard(),
            #[cfg(not(feature = "prove"))]
            root_proof_backend: Arc::new(SmtRootProofBackend),
        })
    }

    /// Replace the host-owned runtime registries and scheme factories.
    pub fn with_host_environment(mut self, host_environment: HostEnvironment) -> Self {
        self.host_environment = host_environment;
        self
    }

    /// Override the machine STARK configuration.
    pub fn with_machine_stark_config(mut self, machine_stark_config: TabulaStarkConfig) -> Self {
        self.machine_stark_config = machine_stark_config;
        self
    }

    /// Override the root proof backend bundle.
    #[cfg(feature = "prove")]
    pub fn with_root_backend_bundle(mut self, root_backend_bundle: RootBackendBundle) -> Self {
        self.root_backend_bundle = root_backend_bundle;
        self
    }

    /// Override the proof-side root backend.
    #[cfg(not(feature = "prove"))]
    pub fn with_root_proof_backend(
        mut self,
        root_proof_backend: impl RootProofBackend + 'static,
    ) -> Self {
        self.root_proof_backend = Arc::new(root_proof_backend);
        self
    }

    /// Override the proof-side root backend using a shared backend object.
    #[cfg(not(feature = "prove"))]
    pub fn with_root_proof_backend_arc(
        mut self,
        root_proof_backend: Arc<dyn RootProofBackend>,
    ) -> Self {
        self.root_proof_backend = root_proof_backend;
        self
    }

    /// Build the prepared verifier.
    pub fn build(self) -> Result<PreparedVerifier, RuntimeError> {
        validate_core_first_program(self.registered_program.program())?;
        #[cfg(feature = "prove")]
        let proof_backend = self.root_backend_bundle.proof_backend();
        #[cfg(not(feature = "prove"))]
        let proof_backend = Arc::clone(&self.root_proof_backend);
        #[cfg(feature = "prove")]
        let accepted_root_binding_families =
            self.root_backend_bundle.supported_root_binding_families();
        #[cfg(not(feature = "prove"))]
        let accepted_root_binding_families = proof_backend.supported_root_binding_families();
        let program_setup = resolve_program_setup(
            &self.registered_program,
            self.host_environment.schemes().factories(),
            self.host_environment.runtime_registries().type_runtimes(),
            self.host_environment
                .runtime_registries()
                .encoding_runtimes(),
            accepted_root_binding_families,
        )?;
        let machine = build_registered_program_machine(
            &program_setup,
            &self.machine_stark_config,
            proof_backend,
        )?;
        Ok(PreparedVerifier {
            prepared: VerifierState {
                context: program_setup.artifact_context,
                relation_policy: program_setup.relation_policy,
                machine,
            },
        })
    }
}

/// Convenience constructor: `prepare_verifier(reg)` is sugar over
/// `PreparedVerifier::builder(reg)?.build()` using the standard host
/// environment, machine config, and root backend.
pub fn prepare_verifier(
    registered_program: RegisteredProgram,
) -> Result<PreparedVerifier, RuntimeError> {
    PreparedVerifier::builder(registered_program)?.build()
}

pub(crate) fn relation_table_root_from_proof(
    proof: &TabulaProof,
    machine: &TabulaMachine,
) -> Result<Option<Digest>, RuntimeError> {
    execution_chip_digest_from_proof(proof, machine, RELATION_TABLE_CHIP_ID, "relation table")
}

fn execution_chip_digest_from_proof(
    proof: &TabulaProof,
    machine: &TabulaMachine,
    chip_id: tabula_stark::chips::ChipId,
    label: &str,
) -> Result<Option<Digest>, RuntimeError> {
    let Some(values) = proof.execution_chip_public_values(chip_id) else {
        return Ok(None);
    };
    let expected_arity = machine
        .execution_chip_public_value_arity(chip_id)
        .ok_or_else(|| RuntimeError::ValidationFailed {
            detail: format!(
                "execution machine metadata is missing {label} chip {}",
                chip_id.0
            ),
        })?;
    if values.len() != expected_arity {
        return Err(RuntimeError::ValidationFailed {
            detail: format!(
                "{label} chip exposed {0} public values; machine metadata requires {expected_arity}",
                values.len()
            ),
        });
    }
    let digest_arity = NativeDigest::ZERO.0.len();
    if expected_arity != digest_arity {
        return Err(RuntimeError::ValidationFailed {
            detail: format!(
                "{label} chip metadata declares {expected_arity} public values, but runtime digest checks require {digest_arity}"
            ),
        });
    }
    let public_values: [p3_koala_bear::KoalaBear; 8] =
        values
            .try_into()
            .map_err(|_| RuntimeError::ValidationFailed {
                detail: format!(
                    "{label} chip exposed {} public values after metadata validation; expected {}",
                    values.len(),
                    digest_arity
                ),
            })?;
    Ok(Some(NativeDigest(public_values).to_bytes()))
}

fn verify_proved_public_statement_digests(
    proof: &TabulaProof,
    machine: &TabulaMachine,
    public_statement: &PublicStatement,
) -> Result<(), RuntimeError> {
    let Some(public_context_digest) = execution_chip_digest_from_proof(
        proof,
        machine,
        PUBLIC_CONTEXT_TRANSCRIPT_CHIP_ID,
        "public-context transcript",
    )?
    else {
        return Err(RuntimeError::ValidationFailed {
            detail: "public-context transcript chip opening is missing from the execution proof"
                .to_string(),
        });
    };
    if public_context_digest != public_statement.public_context_digest.to_bytes() {
        return Err(RuntimeError::ValidationFailed {
            detail:
                "public-context transcript chip digest does not match the proved public statement"
                    .to_string(),
        });
    }

    let Some(applied_tx_digest) = execution_chip_digest_from_proof(
        proof,
        machine,
        TX_BATCH_TRANSCRIPT_CHIP_ID,
        "tx-batch transcript",
    )?
    else {
        return Err(RuntimeError::ValidationFailed {
            detail: "tx-batch transcript chip opening is missing from the execution proof"
                .to_string(),
        });
    };
    if applied_tx_digest != public_statement.applied_tx_digest.to_bytes() {
        return Err(RuntimeError::ValidationFailed {
            detail: "tx-batch transcript chip digest does not match the proved public statement"
                .to_string(),
        });
    }

    let Some(event_digest) = execution_chip_digest_from_proof(
        proof,
        machine,
        EVENT_TRANSCRIPT_CHIP_ID,
        "event transcript",
    )?
    else {
        return Err(RuntimeError::ValidationFailed {
            detail: "event transcript chip opening is missing from the execution proof".to_string(),
        });
    };
    if event_digest != public_statement.event_digest.to_bytes() {
        return Err(RuntimeError::ValidationFailed {
            detail: "event transcript chip digest does not match the proved public statement"
                .to_string(),
        });
    }

    Ok(())
}

// Static guarantee that PreparedVerifier is cheap to share across threads.
// The SDK's cache and any future concurrent driver relies on this.
const _: fn() = || {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<PreparedVerifier>();
    assert_send_sync::<VerifierState>();
};
