//! Unified runtime: owns machine, schemas, and precompile registry.
//!
//! [`TabulaRuntime`](crate::TabulaRuntime) is the primary entry point for applications that need
//! both execution and proving. The machine is built once at setup time
//! and reused across batches.

use tabula_artifact::{State, Statement, TransactionBatch, normalize_state};
use tabula_commitment::PoseidonHasher;
use tabula_executor::precompile::PrecompileRegistry;
use tabula_executor::property::PropertyQueryRegistry;
use tabula_ir::Program;
use tabula_machine::{MachineProofInput, TabulaMachine, TabulaProof};
use tabula_types::{EncodingRuntimeRegistry, TypeRuntimeRegistry};

use crate::builder::RuntimeBuilder;
use crate::error::RuntimeError;
use crate::execute::{ExecutedBatch, ExecutionResources, execute_pipeline};
use crate::program::ResolvedProgram;
use crate::program::SnapshotStateView;
use crate::proving::{self, ProofSummary, ProveInput, ProveResult, VerifiedResult};
use crate::setup::materialize::ColumnProofRecipe;
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
/// - **ResolvedProgram** — runtime materialization of the compiler artifact
/// - **TabulaMachine** — STARK prover/verifier (built once from schemas)
/// - **PrecompileRegistry** — executor-side precompile handlers
/// - **PropertyQueryRegistry** — executor-side property query handlers
pub struct TabulaRuntime {
    resolved_program: ResolvedProgram,
    proof_recipes: Vec<ColumnProofRecipe>,
    precompile_recipes: Vec<crate::proving::PrecompileProofRecipe>,
    machine: TabulaMachine,
    precompiles: PrecompileRegistry,
    property_queries: PropertyQueryRegistry,
}

impl TabulaRuntime {
    /// Create a builder for customized runtime construction.
    pub fn builder(compiled_program: tabula_compiler::SealedProgram) -> RuntimeBuilder {
        RuntimeBuilder::new(compiled_program)
    }

    /// Construct from pre-built parts (used by [`RuntimeBuilder`]).
    pub(crate) fn from_parts(
        resolved_program: ResolvedProgram,
        proof_recipes: Vec<ColumnProofRecipe>,
        precompile_recipes: Vec<crate::proving::PrecompileProofRecipe>,
        machine: TabulaMachine,
        precompiles: PrecompileRegistry,
        property_queries: PropertyQueryRegistry,
    ) -> Self {
        Self {
            resolved_program,
            proof_recipes,
            precompile_recipes,
            machine,
            precompiles,
            property_queries,
        }
    }

    /// The resolved program backing this runtime.
    pub fn resolved_program(&self) -> &ResolvedProgram {
        &self.resolved_program
    }

    /// The IR program executed by this runtime.
    pub fn program(&self) -> &Program {
        self.resolved_program.program()
    }

    /// Runtime type behavior registry.
    pub fn type_runtimes(&self) -> &TypeRuntimeRegistry {
        self.resolved_program.type_runtimes()
    }

    /// Runtime encoding behavior registry.
    pub fn encoding_runtimes(&self) -> &EncodingRuntimeRegistry {
        self.resolved_program.encoding_runtimes()
    }

    /// The precompile registry (for executor integration).
    pub fn precompiles(&self) -> &PrecompileRegistry {
        &self.precompiles
    }

    /// The property query registry (for executor PropertyRead resolution).
    pub fn property_queries(&self) -> &PropertyQueryRegistry {
        &self.property_queries
    }

    /// The STARK machine backing this runtime.
    pub fn machine(&self) -> &TabulaMachine {
        &self.machine
    }

    #[cfg(test)]
    pub(crate) fn proof_recipes(&self) -> &[ColumnProofRecipe] {
        &self.proof_recipes
    }

    #[cfg(test)]
    pub(crate) fn precompile_recipes(&self) -> &[crate::proving::PrecompileProofRecipe] {
        &self.precompile_recipes
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
        state: &State,
        batch: &TransactionBatch,
    ) -> Result<ExecutedBatch, RuntimeError> {
        let hasher = PoseidonHasher::new();
        let normalized = normalize_state(state).map_err(RuntimeError::InvalidState)?;
        let committed =
            SnapshotStateView::from_state(&normalized, self.resolved_program.type_runtimes());

        execute_pipeline(
            self.resolved_program.program(),
            &normalized,
            batch,
            &hasher,
            self.resolved_program.type_runtimes(),
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
    ) -> Result<Statement, RuntimeError> {
        let prepared = proving::prepare_proof_batch(
            &self.resolved_program,
            &self.proof_recipes,
            &self.precompile_recipes,
            input.state,
            input.batch,
            input.executed,
        )?;

        proving::build_execution_statement(
            &self.resolved_program,
            input.state,
            input.batch,
            &input.executed.state_after,
            &prepared.air_statement,
        )
    }

    /// Generate a STARK proof from an executed batch.
    ///
    /// Pipeline: column states -> witness -> traces -> prove.
    #[tracing::instrument(skip_all, name = "prove")]
    pub fn prove(&self, input: &ProveInput<'_>) -> Result<ProveResult, RuntimeError> {
        let mut prepared = proving::prepare_proof_batch(
            &self.resolved_program,
            &self.proof_recipes,
            &self.precompile_recipes,
            input.state,
            input.batch,
            input.executed,
        )?;
        let statement = proving::build_execution_statement(
            &self.resolved_program,
            input.state,
            input.batch,
            &input.executed.state_after,
            &prepared.air_statement,
        )?;

        let proof = {
            let _span = tracing::info_span!("stark_prove").entered();
            let traces = proving::build_traces(&self.machine, &mut prepared)?;
            self.machine
                .prover()
                .prove(MachineProofInput {
                    traces,
                    statement: prepared.air_statement,
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
    pub fn verify(&self, proof: &TabulaProof, statement: &Statement) -> Result<(), RuntimeError> {
        verify_with_binding(
            self.resolved_program.binding(),
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
        state: &State,
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
            .field("resolved_program", &self.resolved_program)
            .field("proof_recipes", &self.proof_recipes.len())
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
        let artifact = case.compiled_program.as_artifact();
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
        let mut prepared = proving::prepare_proof_batch(
            runtime.resolved_program(),
            runtime.proof_recipes(),
            runtime.precompile_recipes(),
            &case.state,
            &case.batch,
            &executed,
        )
        .expect("prepared proof batch");
        let statement = proving::build_execution_statement(
            runtime.resolved_program(),
            &case.state,
            &case.batch,
            &executed.state_after,
            &prepared.air_statement,
        )
        .expect("execution statement");
        let traces = proving::build_traces(runtime.machine(), &mut prepared).expect("proof traces");

        let proof = runtime
            .machine()
            .prover()
            .prove(MachineProofInput {
                traces,
                statement: prepared.air_statement,
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
