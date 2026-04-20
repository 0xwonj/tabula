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
use tabula_machine::{BackendVerifier, TabulaMachine, TabulaProof};

use crate::bootstrap::program::{build_registered_program_machine, resolve_sealed_artifact_setup};
use crate::error::{RuntimeError, VerifyError};
use crate::options::PreparedOptions;

/// Prepared verifier state derived from the sealed artifact and machine setup.
///
/// Public so downstream consumers (SDK, tests, future prover) can name
/// the prepared-once state without going through a builder.
#[non_exhaustive]
pub struct PreparedVerifierState {
    context: ArtifactContext,
    relation_policy: SealedRelationPolicy,
    machine: TabulaMachine,
}

impl PreparedVerifierState {
    /// Construct a verifier state from its three parts (crate-internal).
    pub(crate) fn new(
        context: ArtifactContext,
        relation_policy: SealedRelationPolicy,
        machine: TabulaMachine,
    ) -> Self {
        Self {
            context,
            relation_policy,
            machine,
        }
    }

    /// Artifact-bound transcript context sealed at prepare time.
    pub fn context(&self) -> &ArtifactContext {
        &self.context
    }

    /// Relation-policy decision derived from program analysis.
    pub fn relation_policy(&self) -> SealedRelationPolicy {
        self.relation_policy
    }

    /// STARK machine backing verification.
    pub fn machine(&self) -> &TabulaMachine {
        &self.machine
    }
}

/// Verifier built once per registered native program.
///
/// Cheap to share via Arc; [`PreparedVerifier::verify`] takes
/// `&self` so callers can drive it from multiple threads.
#[non_exhaustive]
pub struct PreparedVerifier {
    state: PreparedVerifierState,
}

impl PreparedVerifier {
    /// Borrow the prepared verify-side state.
    pub fn state(&self) -> &PreparedVerifierState {
        &self.state
    }

    /// Borrow the transcript-bound program binding.
    pub fn binding(&self) -> &ProgramBinding {
        &self.state.context.binding
    }

    /// The STARK machine backing this verifier.
    pub fn machine(&self) -> &TabulaMachine {
        &self.state.machine
    }

    /// Verify one native proof against an externally supplied expected public
    /// statement and return the artifact-bound statement on success.
    pub fn verify(
        &self,
        proof: &TabulaProof,
        expected_public_statement: &PublicStatement,
    ) -> Result<BoundStatement, VerifyError> {
        let bound = BoundStatement::new(
            self.state.context.clone(),
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
            &self.state.machine,
            expected_public_statement,
        )
        .map_err(route_to_verify)?;
        match relation_table_root_from_proof(proof, &self.state.machine).map_err(route_to_verify)? {
            Some(root) if self.state.relation_policy.requires_artifact_root() => {
                if root != self.state.context.static_table_root {
                    return Err(VerifyError::Validation {
                        detail: "relation table chip root does not match the verifier artifact"
                            .to_string(),
                    });
                }
            }
            None if self.state.relation_policy.requires_artifact_root() => {
                return Err(VerifyError::Validation {
                    detail: "relation table chip opening is missing from the execution proof"
                        .to_string(),
                });
            }
            _ => {}
        }
        BackendVerifier::new(&self.state.machine)
            .verify_proof(proof, expected_binding_digest)
            .map_err(VerifyError::Verification)?;
        Ok(bound)
    }
}

/// Build a [`PreparedVerifier`] from a sealed artifact and an option
/// bundle.
///
/// The verifier path is IR-free: it takes `Arc<SealedArtifact>` and does
/// not require a [`tabula_compiler::RegisteredProgram`]. The prover and
/// executor paths stay on `Arc<RegisteredProgram>` because they execute IR.
///
/// Note: `validate_core_first_program` (capability-call rejection) is NOT
/// run on the verifier path. The check requires `ir::Program`, which the
/// verifier does not hold. The engine path (`build_prepared_runtime`)
/// continues to run it; the binding-digest check already gates non-matching
/// programs on the verifier side.
// The by-value Arc mirrors `prepare_prover` and `prepare_executor` so the
// three prepare-* constructors keep a consistent ownership shape. Clippy
// flags it as needless because the body only borrows, but changing the
// signature would desync the public API across handles.
#[allow(clippy::needless_pass_by_value)]
pub fn prepare_verifier(
    sealed: Arc<SealedArtifact>,
    opts: &PreparedOptions,
) -> Result<PreparedVerifier, VerifyError> {
    sealed.validate().map_err(|e| VerifyError::Validation {
        detail: format!("sealed artifact validation failed: {e}"),
    })?;
    #[cfg(feature = "prove")]
    let root_backend_bundle = opts.root_backend().0.clone();
    #[cfg(not(feature = "prove"))]
    let root_proof_backend = Arc::clone(&opts.root_backend().0);
    #[cfg(feature = "prove")]
    let proof_backend = root_backend_bundle.proof_backend();
    #[cfg(not(feature = "prove"))]
    let proof_backend = Arc::clone(&root_proof_backend);
    #[cfg(feature = "prove")]
    let accepted_root_binding_families = root_backend_bundle.supported_root_binding_families();
    #[cfg(not(feature = "prove"))]
    let accepted_root_binding_families = proof_backend.supported_root_binding_families();
    let host_environment = opts.host_environment();
    let program_setup = resolve_sealed_artifact_setup(
        &sealed,
        host_environment.schemes().factories(),
        host_environment.runtime_registries().type_runtimes(),
        host_environment.runtime_registries().encoding_runtimes(),
        accepted_root_binding_families,
    )
    .map_err(route_to_verify)?;
    let machine = build_registered_program_machine(
        &program_setup,
        opts.machine_stark_config(),
        proof_backend,
    )
    .map_err(route_to_verify)?;
    Ok(PreparedVerifier {
        state: PreparedVerifierState::new(
            program_setup.artifact_context,
            program_setup.relation_policy,
            machine,
        ),
    })
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
        values.try_into().map_err(|_| VerifyError::Validation {
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
/// `SetupError`. Setup failures ride through [`VerifyError::Setup`] with a
/// typed `#[source]` chain; other cross-phase errors fall back to
/// [`VerifyError::Validation`] with a forward-compat detail string.
fn route_to_verify(error: RuntimeError) -> VerifyError {
    match error {
        RuntimeError::Verify(inner) => inner,
        RuntimeError::Setup(inner) => VerifyError::Setup(inner),
        #[cfg(feature = "prove")]
        RuntimeError::Prove(inner) => VerifyError::Validation {
            detail: format!("unexpected prove-phase error on verifier surface: {inner}"),
        },
        RuntimeError::Execute(inner) => VerifyError::Validation {
            detail: format!("unexpected execute-phase error on verifier surface: {inner}"),
        },
    }
}

impl std::fmt::Debug for PreparedVerifierState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedVerifierState")
            .field("binding", &self.context.binding)
            .field("static_table_root", &self.context.static_table_root)
            .field("relation_policy", &self.relation_policy)
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for PreparedVerifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedVerifier")
            .field("state", &self.state)
            .finish_non_exhaustive()
    }
}

// Static guarantee that PreparedVerifier is cheap to share across threads.
// The SDK's cache and any future concurrent driver relies on this.
const _: fn() = || {
    fn assert_send_sync_static<T: Send + Sync + 'static>() {}
    assert_send_sync_static::<PreparedVerifier>();
    assert_send_sync_static::<PreparedVerifierState>();
};
