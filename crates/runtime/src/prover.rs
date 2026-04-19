//! Prepared prover handle for one registered native program.
//!
//! [`PreparedProver`] is the canonical way to get a prove-capable
//! runtime handle. It owns the prepared-once state (`VerifierState`,
//! machine, chip-kit registry, root backend bundle) and exposes
//! [`PreparedProver::prove`] for per-batch proving. The handle is
//! `Send + Sync` and cheap to share via `Arc`.

use std::sync::Arc;

use tabula_compiler::RegisteredProgram;
use tabula_contract::{BoundStatement, ProgramBinding, ProofEnvelope, PublicStatement};
use tabula_core::Digest;
use tabula_executor as exec;
use tabula_ext::root::RootBackendBundle;
use tabula_ir as ir;
use tabula_machine::{BackendProver, TabulaMachine, TabulaProof};
use tabula_types::{EncodingRuntimeRegistry, TypeRuntimeRegistry};
use tabula_witness::stark::ChipKitRegistry;

use crate::error::{ProveError, RuntimeError, VerifyError};
use crate::options::PreparedOptions;
use crate::prepared_state::{
    PreparedRuntimeState, build_chip_kit_registry, build_prepared_runtime,
};
use crate::proof_summary::ProofSummary;
use crate::semantics as runtime_ir;
use crate::snapshot::CommittedStateSnapshot;
use crate::verifier::VerifierState;

/// Inputs for native proving.
pub struct ProveInput<'a> {
    /// Committed pre-state.
    pub snapshot: &'a CommittedStateSnapshot,
    /// Applied transactions.
    pub batch: &'a ir::EntryBatch,
    /// Public context values.
    pub context: &'a ir::ContextInput,
    /// Execution journal returned by execution.
    pub executed: &'a exec::ExecutionJournal,
}

/// Result of proof generation, optionally accompanied by a verified bound statement.
///
/// Returned by both [`PreparedProver::prove`] (where `bound_statement` is `None`) and
/// [`PreparedProver::prove_and_verify`] (where `bound_statement` is `Some`, carrying the
/// artifact-bound statement confirmed by the verifier). Collapsing the two previous result
/// types (`ProveResult` / `VerifiedResult`) here removes the duplication and makes the
/// post-verify bound statement accessible as a first-class value.
#[non_exhaustive]
pub struct ProofOutcome {
    proof: TabulaProof,
    envelope: ProofEnvelope,
    public_statement: PublicStatement,
    /// Artifact-bound statement confirmed by the verifier.
    ///
    /// Present when this outcome was produced by [`PreparedProver::prove_and_verify`];
    /// `None` when produced by [`PreparedProver::prove`].
    pub bound_statement: Option<BoundStatement>,
    summary: ProofSummary,
}

impl ProofOutcome {
    /// The generated STARK proof (decoded form).
    pub fn proof(&self) -> &TabulaProof {
        &self.proof
    }

    /// Wire-format envelope around the encoded proof bytes.
    pub fn envelope(&self) -> &ProofEnvelope {
        &self.envelope
    }

    /// The artifact-bound public statement that accompanies `proof`.
    pub fn public_statement(&self) -> &PublicStatement {
        &self.public_statement
    }

    /// Human-readable machine summary.
    pub fn summary(&self) -> &ProofSummary {
        &self.summary
    }

    /// Whether the proof was verified inline.
    ///
    /// `true` iff this outcome carries a `bound_statement` (i.e., was produced by
    /// [`PreparedProver::prove_and_verify`]).
    pub fn verified(&self) -> bool {
        self.bound_statement.is_some()
    }

    /// Consume and unpack all parts.
    ///
    /// Returns `(proof, envelope, public_statement, bound_statement, summary)`.
    pub fn into_parts(
        self,
    ) -> (
        TabulaProof,
        ProofEnvelope,
        PublicStatement,
        Option<BoundStatement>,
        ProofSummary,
    ) {
        (
            self.proof,
            self.envelope,
            self.public_statement,
            self.bound_statement,
            self.summary,
        )
    }
}

impl std::fmt::Debug for ProofOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProofOutcome")
            .field("verified", &self.bound_statement.is_some())
            .field("summary", &self.summary)
            .finish_non_exhaustive()
    }
}

/// Shared prove pipeline entry point used by [`PreparedProver`].
///
/// Prepares the machine input and public statement for one already-executed tx batch.
/// All per-batch mutable state (KitScratch, column artifacts) lives in locals inside
/// this call — calling it twice with the same input produces byte-identical output.
pub(crate) fn prepare_proof_request_on_prepared_state(
    state: &PreparedRuntimeState,
    root_backend_bundle: &RootBackendBundle,
    kit_registry: &ChipKitRegistry,
    machine: &TabulaMachine,
    input: &ProveInput<'_>,
) -> Result<ProofOutcome, RuntimeError> {
    let typed_context = crate::prelude::decode_context_input_on_state(state, input.context)?;
    let typed_txs = crate::prelude::decode_entry_batch_on_state(state, input.batch)?;
    let applied_tx_digest = runtime_ir::compute_applied_tx_digest(
        input.batch,
        &state.type_runtimes,
        &state.encoding_runtimes,
        &state.tuple_encoding_defaults,
    )
    .map_err(|error| VerifyError::StatementBuild {
        detail: error.to_string(),
    })?;
    let proof_artifacts = crate::proof_artifacts::prepare_proof_artifacts(
        state,
        root_backend_bundle,
        kit_registry,
        input.snapshot,
        &typed_txs,
        &typed_context,
        input.executed,
    )?;
    let public_statement = crate::statement::materialize_public_statement_on_state(
        state,
        &typed_context,
        runtime_ir::PublicStatementMaterialization {
            applied_tx_digest,
            old_state_root: proof_artifacts.public_statement.old_root.to_bytes(),
            new_state_root: proof_artifacts.public_statement.new_root.to_bytes(),
        },
        input.executed,
    )?;
    let binding_digest =
        BoundStatement::new(state.artifact_context.clone(), public_statement.clone())
            .binding_digest()
            .map_err(|error| VerifyError::StatementBuild {
                detail: error.to_string(),
            })?;
    let machine_input = proof_artifacts.into_prepared_machine_input(binding_digest);
    let (proof, envelope) = BackendProver::new(machine)
        .prove_envelope(machine_input)
        .map_err(ProveError::Proving)?;
    let summary = ProofSummary::from_proof(&proof);
    Ok(ProofOutcome {
        proof,
        envelope,
        public_statement,
        bound_statement: None,
        summary,
    })
}

/// Prepared prover handle for one registered native program.
///
/// Owns the prepared-once state and the chip-kit registry hoisted to
/// handle-build time. Per-prove mutable state (KitScratch, column
/// artifacts) is allocated fresh inside each [`PreparedProver::prove`]
/// call — calling `prove` twice on the same handle with the same input
/// must produce byte-identical output.
#[non_exhaustive]
pub struct PreparedProver {
    /// Prove-specific prepared state (semantic, state runtime, etc.).
    pub(crate) runtime_program: PreparedRuntimeState,
    /// Root proof-backend bundle shared across prove calls.
    pub(crate) root_backend_bundle: RootBackendBundle,
    /// Chip-kit registry built once at handle construction time.
    pub(crate) kit_registry: ChipKitRegistry,
    /// Verify-side state: context, relation policy, and STARK machine.
    verifier_state: VerifierState,
}

impl PreparedProver {
    /// Borrow the transcript-bound program binding.
    pub fn binding(&self) -> &ProgramBinding {
        &self.verifier_state.context().binding
    }

    /// Borrow the transcript-bound static relation-table root.
    pub fn static_table_root(&self) -> Digest {
        self.verifier_state.context().static_table_root
    }

    /// The STARK machine backing this prover.
    pub fn machine(&self) -> &TabulaMachine {
        self.verifier_state.machine()
    }

    /// Installed type runtimes.
    pub fn type_runtimes(&self) -> &TypeRuntimeRegistry {
        &self.runtime_program.type_runtimes
    }

    /// Installed encoding runtimes.
    pub fn encoding_runtimes(&self) -> &EncodingRuntimeRegistry {
        &self.runtime_program.encoding_runtimes
    }

    /// Borrow the prepared verify-side state (shared semantics with `PreparedVerifier`).
    pub fn state(&self) -> &VerifierState {
        &self.verifier_state
    }

    /// Generate a proof for one already-executed tx batch.
    ///
    /// `&self` is load-bearing: prepared state is shared-read, and
    /// all per-batch mutable state (KitScratch, column artifacts)
    /// lives in locals inside this call. Calling `prove` twice on
    /// the same handle with the same input must produce byte-identical
    /// output.
    ///
    /// The returned [`ProofOutcome`] has `bound_statement: None`; use
    /// [`PreparedProver::prove_and_verify`] to obtain the verified bound
    /// statement in one call.
    pub fn prove(&self, input: &ProveInput<'_>) -> Result<ProofOutcome, ProveError> {
        prepare_proof_request_on_prepared_state(
            &self.runtime_program,
            &self.root_backend_bundle,
            &self.kit_registry,
            self.verifier_state.machine(),
            input,
        )
        .map_err(route_to_prove)
    }

    /// Generate and verify a proof in one call.
    ///
    /// The returned [`ProofOutcome`] has `bound_statement: Some(…)` carrying
    /// the artifact-bound statement confirmed by `verifier`.
    pub fn prove_and_verify(
        &self,
        verifier: &crate::PreparedVerifier,
        input: &ProveInput<'_>,
    ) -> Result<ProofOutcome, ProveError> {
        let outcome = self.prove(input)?;
        let bound = verifier
            .verify(&outcome.proof, &outcome.public_statement)
            .map_err(ProveError::PostVerify)?;
        Ok(ProofOutcome {
            bound_statement: Some(bound),
            ..outcome
        })
    }
}

/// Build a [`PreparedProver`] from a shared registered program and an
/// option bundle.
///
/// This is the canonical way to construct a prover handle: the
/// host-environment, machine-config, and root-backend knobs travel
/// through [`PreparedOptions`] instead of a fluent builder chain.
pub fn prepare_prover(
    registered: Arc<RegisteredProgram>,
    opts: &PreparedOptions,
) -> Result<PreparedProver, ProveError> {
    let program = Arc::unwrap_or_clone(registered);
    program
        .validate_sealed_artifact()
        .map_err(|e| ProveError::Setup(crate::error::SetupError::CompilerValidation(e)))?;
    let prepared = build_prepared_runtime(
        &program,
        opts.host_environment(),
        opts.machine_stark_config(),
        opts.root_backend().0.clone(),
    )
    .map_err(route_to_prove)?;
    let kit_registry = build_chip_kit_registry(&prepared.runtime_program);
    let verifier_state = VerifierState::new(
        prepared.runtime_program.artifact_context.clone(),
        prepared.runtime_program.relation_policy,
        prepared.machine,
    );
    Ok(PreparedProver {
        runtime_program: prepared.runtime_program,
        root_backend_bundle: prepared.root_backend_bundle,
        kit_registry,
        verifier_state,
    })
}

/// Narrow a [`RuntimeError`] to [`ProveError`] for the prover surface.
///
/// `prepare_proof_request_on_prepared_state` and the builder chain produce
/// `RuntimeError::Prove(_)`, `RuntimeError::Verify(_)` (statement-build
/// during pre-prove decode), `RuntimeError::Execute(_)` (decode steps), and
/// `RuntimeError::Setup(_)` (machine / validation failures on handle build).
/// Each narrowed phase now rides through a typed `#[source]` chain so
/// `Display` composition is preserved without stringification.
fn route_to_prove(error: RuntimeError) -> ProveError {
    match error {
        RuntimeError::Prove(inner) => inner,
        RuntimeError::Setup(inner) => ProveError::Setup(inner),
        RuntimeError::Verify(inner) => ProveError::Verify(inner),
        RuntimeError::Execute(inner) => ProveError::Execute(inner),
    }
}

impl std::fmt::Debug for PreparedProver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedProver")
            .field("binding", &self.verifier_state.context().binding)
            .field(
                "static_table_root",
                &self.verifier_state.context().static_table_root,
            )
            .field("relation_policy", &self.runtime_program.relation_policy)
            .finish_non_exhaustive()
    }
}

// Load-bearing Send+Sync+'static: PreparedProver must be cheap to share via Arc.
const _: fn() = || {
    fn assert_send_sync_static<T: Send + Sync + 'static>() {}
    assert_send_sync_static::<PreparedProver>();
};

#[cfg(all(test, feature = "prove"))]
#[path = "prover_relation_tests.rs"]
mod relation_proof_tests;

#[cfg(test)]
mod tests {
    use super::*;

    use tabula_ir as ir;
    use tabula_testing::exec::{context_input, register_program_from_source, tx_batch};
    use tabula_types::u64_portable;

    /// A simple stateless program suitable for proving tests.
    fn simple_source() -> &'static str {
        r#"
program SimpleProve

context {
  caller: u64;
}

state {
  table accounts(key id: u64) {
    tier: u64 @ssmc;
  }
}

relation AllowedTier(tier: u64) = enum { 0, 1, 2 };

tx enroll(id: u64, tier: u64) {
  assert relation AllowedTier(tier);
  accounts[id].tier = tier;
  return;
}
"#
    }

    fn build_prover_and_input() -> (
        PreparedProver,
        crate::snapshot::CommittedStateSnapshot,
        ir::EntryBatch,
        ir::ContextInput,
        tabula_executor::ExecutionJournal,
    ) {
        let registered = register_program_from_source(simple_source());
        let opts = PreparedOptions::try_standard().expect("standard prepared options");
        let prover =
            prepare_prover(Arc::new(registered.clone()), &opts).expect("build PreparedProver");

        // Share the registered program with a PreparedExecutor to drive
        // execute_batch; prover and executor both resolve from the same
        // registered program so their prepared states are equivalent.
        let executor = crate::prepare_executor(Arc::new(registered.clone()), &opts)
            .expect("build PreparedExecutor");

        // Prepopulate state so @ssmc column reads succeed during proving.
        let snapshot = executor
            .materialize_logical_state([
                (
                    ir::TableId(0),
                    vec![u64_portable(0)],
                    ir::FieldId(0),
                    u64_portable(0),
                ),
                (
                    ir::TableId(0),
                    vec![u64_portable(1)],
                    ir::FieldId(0),
                    u64_portable(0),
                ),
            ])
            .expect("build initial snapshot");

        let entry_id = executor.entry_id_by_symbol("enroll").expect("enroll entry");
        let batch = tx_batch(vec![ir::EntryCall {
            entry_id,
            params: vec![u64_portable(0), u64_portable(1)],
        }]);
        let ctx = context_input([(ir::ContextFieldId(0), u64_portable(7))]);
        let executed = executor
            .execute_batch(&snapshot, &batch, &ctx)
            .expect("execute batch");
        (prover, snapshot, batch, ctx, executed)
    }

    #[test]
    fn prove_twice_on_same_handle_is_byte_identical() {
        let (prover, snapshot, batch, ctx, executed) = build_prover_and_input();
        let input = ProveInput {
            snapshot: &snapshot,
            batch: &batch,
            context: &ctx,
            executed: &executed,
        };

        let result1 = prover.prove(&input).expect("first prove");
        let result2 = prover.prove(&input).expect("second prove");

        // Compare via the canonical ProofEnvelope (the wire-format bytes).
        assert_eq!(
            result1.envelope, result2.envelope,
            "prove must be deterministic per-handle (envelope bytes differ)"
        );
        assert_eq!(
            result1.public_statement, result2.public_statement,
            "public statements must match"
        );
    }
}
