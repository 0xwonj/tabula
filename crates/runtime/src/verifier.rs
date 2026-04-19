//! Native proof verification surface and shared verification core.

use std::sync::Arc;

use tabula_chips::event_transcript::EVENT_TRANSCRIPT_CHIP_ID;
use tabula_chips::public_context_transcript::PUBLIC_CONTEXT_TRANSCRIPT_CHIP_ID;
use tabula_chips::relation_table::RELATION_TABLE_CHIP_ID;
use tabula_chips::tx_batch_transcript::TX_BATCH_TRANSCRIPT_CHIP_ID;
use tabula_commitment::NativeDigest;
use tabula_contract::SealedArtifact;
use tabula_contract::{
    ArtifactContext, BoundStatement, ProgramBinding, PublicStatement, SealedRelationPolicy,
};
use tabula_core::Digest;
#[cfg(feature = "prove")]
use tabula_ext::root::RootBackendBundle;
#[cfg(not(feature = "prove"))]
use tabula_ext::root::{RootProofBackend, SmtRootProofBackend};
use tabula_machine::{BackendVerifier, TabulaMachine, TabulaProof, TabulaStarkConfig};

use crate::bootstrap::program::{build_registered_program_machine, resolve_sealed_artifact_setup};
use crate::error::{RuntimeError, SetupError, VerifyError};
use crate::host::HostEnvironment;
use crate::options::PreparedOptions;

/// Prepared verifier state derived from the sealed artifact and machine setup.
///
/// Public so downstream consumers (SDK, tests, future prover) can name
/// the prepared-once state without going through a builder.
#[non_exhaustive]
pub struct VerifierState {
    /// Artifact-bound transcript context sealed at prepare time.
    pub context: ArtifactContext,
    /// Relation-policy decision derived from program analysis.
    pub relation_policy: SealedRelationPolicy,
    /// STARK machine backing verification.
    pub machine: TabulaMachine,
}

/// Verifier built once per registered native program.
///
/// Cheap to share via Arc; [`PreparedVerifier::verify`] takes
/// `&self` so callers can drive it from multiple threads.
#[non_exhaustive]
pub struct PreparedVerifier {
    prepared: VerifierState,
}

/// Fluent builder for [`PreparedVerifier`].
pub struct PreparedVerifierBuilder {
    sealed_artifact: Arc<SealedArtifact>,
    host_environment: HostEnvironment,
    machine_stark_config: TabulaStarkConfig,
    #[cfg(feature = "prove")]
    root_backend_bundle: RootBackendBundle,
    #[cfg(not(feature = "prove"))]
    root_proof_backend: Arc<dyn RootProofBackend>,
}

impl PreparedVerifier {
    /// Create a builder for a sealed artifact.
    pub fn builder(
        sealed_artifact: Arc<SealedArtifact>,
    ) -> Result<PreparedVerifierBuilder, RuntimeError> {
        PreparedVerifierBuilder::new(sealed_artifact)
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
    ) -> Result<BoundStatement, VerifyError> {
        let bound = BoundStatement::new(
            self.prepared.context.clone(),
            expected_public_statement.clone(),
        );
        let expected_binding_digest =
            bound
                .binding_digest()
                .map_err(|error| VerifyError::StatementBuild {
                    detail: error.to_string(),
                })?;
        if proof.binding_digest != expected_binding_digest {
            return Err(VerifyError::Validation {
                detail: "proof binding digest does not match the artifact-bound public statement"
                    .to_string(),
            });
        }
        verify_proved_public_statement_digests(
            proof,
            &self.prepared.machine,
            expected_public_statement,
        )
        .map_err(route_to_verify)?;
        match relation_table_root_from_proof(proof, &self.prepared.machine)
            .map_err(route_to_verify)?
        {
            Some(root) if self.prepared.relation_policy.requires_artifact_root() => {
                if root != self.prepared.context.static_table_root {
                    return Err(VerifyError::Validation {
                        detail: "relation table chip root does not match the verifier artifact"
                            .to_string(),
                    });
                }
            }
            None if self.prepared.relation_policy.requires_artifact_root() => {
                return Err(VerifyError::Validation {
                    detail: "relation table chip opening is missing from the execution proof"
                        .to_string(),
                });
            }
            _ => {}
        }
        BackendVerifier::new(&self.prepared.machine)
            .verify_proof(proof)
            .map_err(VerifyError::Verification)?;
        Ok(bound)
    }
}

impl PreparedVerifierBuilder {
    fn new(sealed_artifact: Arc<SealedArtifact>) -> Result<Self, RuntimeError> {
        sealed_artifact
            .validate()
            .map_err(|e| SetupError::Validation {
                detail: format!("sealed artifact validation failed: {e}"),
            })?;
        Ok(Self {
            sealed_artifact,
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
    ///
    /// Note: `validate_core_first_program` (capability-call rejection) is NOT
    /// run on the verifier path. The check requires `ir::Program`, which the
    /// verifier does not hold. The engine path (`build_prepared_runtime`)
    /// continues to run it; the binding-digest check already gates non-matching
    /// programs on the verifier side.
    pub fn build(self) -> Result<PreparedVerifier, RuntimeError> {
        #[cfg(feature = "prove")]
        let proof_backend = self.root_backend_bundle.proof_backend();
        #[cfg(not(feature = "prove"))]
        let proof_backend = Arc::clone(&self.root_proof_backend);
        #[cfg(feature = "prove")]
        let accepted_root_binding_families =
            self.root_backend_bundle.supported_root_binding_families();
        #[cfg(not(feature = "prove"))]
        let accepted_root_binding_families = proof_backend.supported_root_binding_families();
        let program_setup = resolve_sealed_artifact_setup(
            &self.sealed_artifact,
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

/// Build a [`PreparedVerifier`] from a sealed artifact and an option
/// bundle.
///
/// The verifier path is IR-free: it takes `Arc<SealedArtifact>` and does
/// not require a `RegisteredProgram`. The prover and executor paths stay
/// on `Arc<RegisteredProgram>` because they execute IR.
pub fn prepare_verifier(
    sealed: Arc<SealedArtifact>,
    opts: &PreparedOptions,
) -> Result<PreparedVerifier, VerifyError> {
    let builder = PreparedVerifier::builder(sealed)
        .map_err(route_to_verify)?
        .with_host_environment(opts.host_environment().clone())
        .with_machine_stark_config(opts.machine_stark_config().clone());
    #[cfg(feature = "prove")]
    let builder = builder.with_root_backend_bundle(opts.root_backend().0.clone());
    #[cfg(not(feature = "prove"))]
    let builder = builder.with_root_proof_backend_arc(Arc::clone(&opts.root_backend().0));
    builder.build().map_err(route_to_verify)
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
        .ok_or_else(|| VerifyError::Validation {
            detail: format!(
                "execution machine metadata is missing {label} chip {}",
                chip_id.0
            ),
        })?;
    if values.len() != expected_arity {
        return Err(VerifyError::Validation {
            detail: format!(
                "{label} chip exposed {0} public values; machine metadata requires {expected_arity}",
                values.len()
            ),
        }
        .into());
    }
    let digest_arity = NativeDigest::ZERO.0.len();
    if expected_arity != digest_arity {
        return Err(VerifyError::Validation {
            detail: format!(
                "{label} chip metadata declares {expected_arity} public values, but runtime digest checks require {digest_arity}"
            ),
        }
        .into());
    }
    let public_values: [p3_koala_bear::KoalaBear; 8] =
        values
            .try_into()
            .map_err(|_| VerifyError::Validation {
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
        return Err(VerifyError::Validation {
            detail: "public-context transcript chip opening is missing from the execution proof"
                .to_string(),
        }
        .into());
    };
    if public_context_digest != public_statement.public_context_digest.to_bytes() {
        return Err(VerifyError::Validation {
            detail:
                "public-context transcript chip digest does not match the proved public statement"
                    .to_string(),
        }
        .into());
    }

    let Some(applied_tx_digest) = execution_chip_digest_from_proof(
        proof,
        machine,
        TX_BATCH_TRANSCRIPT_CHIP_ID,
        "tx-batch transcript",
    )?
    else {
        return Err(VerifyError::Validation {
            detail: "tx-batch transcript chip opening is missing from the execution proof"
                .to_string(),
        }
        .into());
    };
    if applied_tx_digest != public_statement.applied_tx_digest.to_bytes() {
        return Err(VerifyError::Validation {
            detail: "tx-batch transcript chip digest does not match the proved public statement"
                .to_string(),
        }
        .into());
    }

    let Some(event_digest) = execution_chip_digest_from_proof(
        proof,
        machine,
        EVENT_TRANSCRIPT_CHIP_ID,
        "event transcript",
    )?
    else {
        return Err(VerifyError::Validation {
            detail: "event transcript chip opening is missing from the execution proof".to_string(),
        }
        .into());
    };
    if event_digest != public_statement.event_digest.to_bytes() {
        return Err(VerifyError::Validation {
            detail: "event transcript chip digest does not match the proved public statement"
                .to_string(),
        }
        .into());
    }

    Ok(())
}

/// Narrow a [`RuntimeError`] to [`VerifyError`] for the verifier surface.
///
/// Internal helpers (`execution_chip_digest_from_proof`,
/// `relation_table_root_from_proof`, `verify_proved_public_statement_digests`)
/// and the builder chain return `RuntimeError` wrapping `VerifyError` or
/// `SetupError`. All `Verify` variants map directly; setup and other phases
/// observed on the verifier surface are pre-verification steps and map to
/// `VerifyError::Validation` with a preserved detail string.
fn route_to_verify(error: RuntimeError) -> VerifyError {
    match error {
        RuntimeError::Verify(inner) => inner,
        other => VerifyError::Validation {
            detail: other.to_string(),
        },
    }
}

// Static guarantee that PreparedVerifier is cheap to share across threads.
// The SDK's cache and any future concurrent driver relies on this.
const _: fn() = || {
    fn assert_send_sync_static<T: Send + Sync + 'static>() {}
    assert_send_sync_static::<PreparedVerifier>();
    assert_send_sync_static::<VerifierState>();
};
