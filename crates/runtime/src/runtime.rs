//! Unified runtime: owns machine, schemas, and precompile registry.
//!
//! [`TabulaRuntime`](crate::TabulaRuntime) is the primary entry point for applications that need
//! both execution and proving. The machine is built once at setup time
//! and reused across batches.

use tabula_artifact::{ExecutionStatement, StateSnapshot, TransactionBatch, normalize_state};
use tabula_commitment::PoseidonHasher;
use tabula_executor::precompile::PrecompileRegistry;
use tabula_executor::property::PropertyQueryRegistry;
use tabula_ir::Program;
use tabula_machine::{MachineProofInput, TabulaMachine, TabulaProof};

use crate::builder::RuntimeBuilder;
use crate::error::RuntimeError;
use crate::execute::{ExecutedBatch, ExecutionResources, execute_pipeline};
use crate::program::RuntimeProgram;
use crate::program::StateSnapshotCommittedState;
use crate::proving::{self, ProofSummary, ProveInput, ProveResult, VerifiedResult};
use crate::verifier::verify_with_binding;

/// Unified runtime owning execution and proving infrastructure.
///
/// Built once via [`RuntimeBuilder`], then reused for every batch:
///
/// ```ignore
/// let runtime = TabulaRuntime::builder(compiled_program).build()?;
///
/// // Per-batch: execute, then prove
/// let executed = runtime.execute(&state_file, &batch_file)?;
/// let result = runtime.prove_and_verify(&ProveInput {
///     state: &state_file,
///     batch: &batch_file,
///     executed: &executed,
/// })?;
/// assert!(result.verified);
/// ```
///
/// # What it owns
///
/// - **RuntimeProgram** — runtime materialization of the compiler artifact
/// - **TabulaMachine** — STARK prover/verifier (built once from schemas)
/// - **PrecompileRegistry** — executor-side precompile handlers
/// - **PropertyQueryRegistry** — executor-side property query handlers
pub struct TabulaRuntime {
    runtime_program: RuntimeProgram,
    machine: TabulaMachine,
    precompiles: PrecompileRegistry,
    property_queries: PropertyQueryRegistry,
}

impl TabulaRuntime {
    /// Create a builder for customized runtime construction.
    pub fn builder(compiled_program: tabula_compiler::CompiledProgram) -> RuntimeBuilder {
        RuntimeBuilder::new(compiled_program)
    }

    /// Construct from pre-built parts (used by [`RuntimeBuilder`]).
    pub(crate) fn from_parts(
        runtime_program: RuntimeProgram,
        machine: TabulaMachine,
        precompiles: PrecompileRegistry,
        property_queries: PropertyQueryRegistry,
    ) -> Self {
        Self {
            runtime_program,
            machine,
            precompiles,
            property_queries,
        }
    }

    /// The runtime program backing this runtime.
    pub fn runtime_program(&self) -> &RuntimeProgram {
        &self.runtime_program
    }

    /// The IR program executed by this runtime.
    pub fn program(&self) -> &Program {
        self.runtime_program.program()
    }

    /// The precompile registry (for executor integration).
    pub fn precompiles(&self) -> &PrecompileRegistry {
        &self.precompiles
    }

    /// The property query registry (for executor PropertyRead resolution).
    pub fn property_queries(&self) -> &PropertyQueryRegistry {
        &self.property_queries
    }

    /// The STARK machine (for advanced usage).
    pub fn machine(&self) -> &TabulaMachine {
        &self.machine
    }

    /// Execute a batch using the runtime's owned resources.
    ///
    /// Unlike the free function [`run_batch()`](crate::run_batch), this method:
    /// - Uses `PoseidonHasher` (consistent with the proving path)
    /// - Passes registered precompiles and property query handlers to the executor
    ///
    /// Returns an [`ExecutedBatch`] ready for [`prove()`](Self::prove).
    #[tracing::instrument(skip_all, name = "execute")]
    pub fn execute(
        &self,
        state: &StateSnapshot,
        batch: &TransactionBatch,
    ) -> Result<ExecutedBatch, RuntimeError> {
        let hasher = PoseidonHasher::new();
        let normalized = normalize_state(state).map_err(RuntimeError::InvalidState)?;
        let committed = StateSnapshotCommittedState::from_cells(&normalized.cells);

        execute_pipeline(
            self.runtime_program.program(),
            &normalized,
            batch,
            &hasher,
            ExecutionResources {
                precompiles: Some(&self.precompiles),
                committed_state: Some(&committed),
                property_queries: &self.property_queries,
            },
        )
    }

    /// Build the canonical execution statement for one executed batch.
    #[tracing::instrument(skip_all, name = "build_execution_statement")]
    pub fn build_execution_statement(
        &self,
        input: &ProveInput<'_>,
    ) -> Result<ExecutionStatement, RuntimeError> {
        let artifacts = proving::prepare_witness_artifacts(
            &self.runtime_program,
            input.state,
            input.batch,
            input.executed,
        )?;

        proving::build_execution_statement(
            &self.runtime_program,
            input.state,
            input.batch,
            &input.executed.state_after,
            &artifacts.air_statement,
        )
    }

    /// Generate a STARK proof from an executed batch.
    ///
    /// Pipeline: column states -> witness -> traces -> prove.
    #[tracing::instrument(skip_all, name = "prove")]
    pub fn prove(&self, input: &ProveInput<'_>) -> Result<ProveResult, RuntimeError> {
        let artifacts = proving::prepare_witness_artifacts(
            &self.runtime_program,
            input.state,
            input.batch,
            input.executed,
        )?;
        let statement = proving::build_execution_statement(
            &self.runtime_program,
            input.state,
            input.batch,
            &input.executed.state_after,
            &artifacts.air_statement,
        )?;

        let column_identities = artifacts.proof_input.column_identities();
        let traces =
            proving::build_traces(&self.machine, artifacts.proof_input, &artifacts.lowering)?;

        let proof = {
            let _span = tracing::info_span!("stark_prove").entered();
            self.machine
                .prover()
                .prove(MachineProofInput {
                    traces,
                    column_identities,
                    statement: artifacts.air_statement,
                    statement_digest: statement.statement_hash_bytes(),
                })
                .map_err(RuntimeError::Proving)?
        };

        let summary = ProofSummary::from_proof(&proof);
        tracing::info!(chip_count = summary.chip_count, "proof generated");

        Ok(ProveResult {
            proof,
            statement,
            summary,
        })
    }

    /// Verify a STARK proof against this runtime's machine and expected statement.
    #[tracing::instrument(skip_all, name = "verify")]
    pub fn verify(
        &self,
        proof: &TabulaProof,
        statement: &ExecutionStatement,
    ) -> Result<(), RuntimeError> {
        verify_with_binding(
            self.runtime_program.binding(),
            &self.machine,
            proof,
            statement,
        )
    }

    /// Generate and verify a STARK proof.
    ///
    /// Convenience method that calls [`prove()`](Self::prove) then
    /// [`verify()`](Self::verify).
    #[tracing::instrument(skip_all, name = "prove_and_verify")]
    pub fn prove_and_verify(&self, input: &ProveInput<'_>) -> Result<VerifiedResult, RuntimeError> {
        let prove_result = self.prove(input)?;

        {
            let _span = tracing::info_span!("stark_verify").entered();
            self.verify(&prove_result.proof, &prove_result.statement)?;
        }

        tracing::info!(verified = true, "verification complete");

        Ok(VerifiedResult {
            proof: prove_result.proof,
            statement: prove_result.statement,
            verified: true,
            summary: prove_result.summary,
        })
    }

    /// Execute a batch, generate a STARK proof, and verify it in one call.
    ///
    /// Convenience method chaining [`execute()`](Self::execute) →
    /// [`prove_and_verify()`](Self::prove_and_verify), with full tracing.
    #[tracing::instrument(skip_all, name = "execute_and_prove")]
    pub fn execute_and_prove(
        &self,
        state: &StateSnapshot,
        batch: &TransactionBatch,
    ) -> Result<VerifiedResult, RuntimeError> {
        let executed = self.execute(state, batch)?;
        self.prove_and_verify(&ProveInput {
            state,
            batch,
            executed: &executed,
        })
    }
}

impl std::fmt::Debug for TabulaRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TabulaRuntime")
            .field("runtime_program", &self.runtime_program)
            .field("machine", &self.machine)
            .field("precompiles_registered", &!self.precompiles.is_empty())
            .field(
                "property_queries_registered",
                &!self.property_queries.is_empty(),
            )
            .finish()
    }
}

// Compile-time verification that TabulaRuntime is Send + Sync.
// This enables `Arc<TabulaRuntime>` for multi-threaded batch processing.
#[allow(dead_code)]
const _: () = {
    fn assert_send_sync<T: Send + Sync>() {}
    fn _check() {
        assert_send_sync::<TabulaRuntime>();
    }
};

#[cfg(test)]
mod tests {
    use tabula_machine::MachineProofInput;
    use tabula_testing::assertions::assert_statement_matches_artifact;
    use tabula_testing::fixtures::examples::transfer_example_compiled_case;

    use super::*;
    use crate::proving;

    #[test]
    fn runtime_prove_and_verify_smoke() {
        let case = transfer_example_compiled_case();
        let artifact = case.compiled_program.as_program_artifact();
        let runtime = TabulaRuntime::builder(case.compiled_program)
            .build()
            .expect("runtime");
        let executed = runtime
            .execute(&case.state, &case.batch)
            .expect("execution succeeds");
        let verified = runtime
            .prove_and_verify(&ProveInput {
                state: &case.state,
                batch: &case.batch,
                executed: &executed,
            })
            .expect("prove and verify");

        assert!(verified.verified);
        assert!(!verified.proof.columns.is_empty());
        assert_statement_matches_artifact(&verified.statement, &artifact);
    }

    #[test]
    fn machine_backend_proves_from_prepared_traces() {
        let case = transfer_example_compiled_case();
        let runtime = TabulaRuntime::builder(case.compiled_program)
            .build()
            .expect("runtime");
        let executed = runtime
            .execute(&case.state, &case.batch)
            .expect("execution succeeds");
        let artifacts = proving::prepare_witness_artifacts(
            runtime.runtime_program(),
            &case.state,
            &case.batch,
            &executed,
        )
        .expect("witness artifacts");
        let statement = proving::build_execution_statement(
            runtime.runtime_program(),
            &case.state,
            &case.batch,
            &executed.state_after,
            &artifacts.air_statement,
        )
        .expect("execution statement");
        let column_identities = artifacts.proof_input.column_identities();
        let traces = proving::build_traces(
            runtime.machine(),
            artifacts.proof_input,
            &artifacts.lowering,
        )
        .expect("proof traces");

        let proof = runtime
            .machine()
            .prover()
            .prove(MachineProofInput {
                traces,
                column_identities,
                statement: artifacts.air_statement,
                statement_digest: statement.statement_hash_bytes(),
            })
            .expect("machine prove");

        runtime
            .machine()
            .verifier()
            .verify(&proof)
            .expect("machine verify");
    }
}
