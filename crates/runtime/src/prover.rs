//! Prepared prover handle for one registered native program.
//!
//! [`PreparedProver`] is the canonical way to get a prove-capable
//! runtime handle. It owns the prepared-once state (`VerifierState`,
//! machine, chip-kit registry, root backend bundle) and exposes
//! [`PreparedProver::prove`] for per-batch proving. The handle is
//! `Send + Sync` and cheap to share via `Arc`.

use std::sync::Arc;

use tabula_compiler::RegisteredProgram;
use tabula_contract::ProgramBinding;
use tabula_core::Digest;
use tabula_ext::root::RootBackendBundle;
use tabula_machine::TabulaMachine;
use tabula_types::{EncodingRuntimeRegistry, TypeRuntimeRegistry};
use tabula_witness::stark::ChipKitRegistry;

use crate::engine::{
    ProveInput, ProveResult, VerifiedResult, prepare_proof_request_on_prepared_state,
};
use crate::error::{ProveError, RuntimeError, VerifyError};
use crate::options::PreparedOptions;
use crate::prepared_state::{
    PreparedRuntimeState, build_chip_kit_registry, build_prepared_runtime,
};
use crate::verifier::VerifierState;

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
        &self.verifier_state.context.binding
    }

    /// Borrow the transcript-bound static relation-table root.
    pub fn static_table_root(&self) -> Digest {
        self.verifier_state.context.static_table_root
    }

    /// The STARK machine backing this prover.
    pub fn machine(&self) -> &TabulaMachine {
        &self.verifier_state.machine
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
    pub fn prove(&self, input: &ProveInput<'_>) -> Result<ProveResult, ProveError> {
        prepare_proof_request_on_prepared_state(
            &self.runtime_program,
            &self.root_backend_bundle,
            &self.kit_registry,
            &self.verifier_state.machine,
            input,
        )
        .map_err(route_to_prove)
    }

    /// Generate and verify a proof in one call.
    pub fn prove_and_verify(
        &self,
        verifier: &crate::PreparedVerifier,
        input: &ProveInput<'_>,
    ) -> Result<VerifiedResult, ProveError> {
        let prove_result = self.prove(input)?;
        verifier
            .verify(&prove_result.proof, &prove_result.public_statement)
            .map_err(|e| match e {
                VerifyError::Verification(source) => ProveError::PostVerify(source),
                other => ProveError::WitnessGeneration {
                    detail: other.to_string(),
                },
            })?;
        Ok(VerifiedResult {
            proof: prove_result.proof,
            envelope: prove_result.envelope,
            public_statement: prove_result.public_statement,
            verified: true,
            summary: prove_result.summary,
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
    let program = Arc::try_unwrap(registered).unwrap_or_else(|shared| (*shared).clone());
    program
        .validate_sealed_artifact()
        .map_err(|e| ProveError::WitnessGeneration {
            detail: e.to_string(),
        })?;
    let prepared = build_prepared_runtime(
        &program,
        opts.host_environment(),
        opts.machine_stark_config(),
        opts.root_backend().0.clone(),
    )
    .map_err(route_to_prove)?;
    let kit_registry = build_chip_kit_registry(&prepared.runtime_program);
    let verifier_state = VerifierState {
        context: prepared.runtime_program.artifact_context.clone(),
        relation_policy: prepared.runtime_program.relation_policy,
        machine: prepared.machine,
    };
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
/// All non-`Prove` variants are pre-prove setup or decode steps; they map to
/// `ProveError::WitnessGeneration` with a preserved detail string.
fn route_to_prove(error: RuntimeError) -> ProveError {
    match error {
        RuntimeError::Prove(inner) => inner,
        other => ProveError::WitnessGeneration {
            detail: other.to_string(),
        },
    }
}

// Load-bearing Send+Sync+'static: PreparedProver must be cheap to share via Arc.
const _: fn() = || {
    fn assert_send_sync_static<T: Send + Sync + 'static>() {}
    assert_send_sync_static::<PreparedProver>();
};

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
        let prover = prepare_prover(Arc::new(registered.clone()), &opts)
            .expect("build PreparedProver");

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

        let entry_id = executor
            .entry_id_by_symbol("enroll")
            .expect("enroll entry");
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
