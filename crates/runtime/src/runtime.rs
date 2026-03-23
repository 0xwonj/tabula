//! Unified runtime: owns machine, schemas, and precompile registry.
//!
//! [`TabulaRuntime`](crate::TabulaRuntime) is the primary entry point for applications that need
//! both execution and proving. The machine is built once at setup time
//! and reused across batches.

use tabula_artifact::{State, Statement, TransactionBatch, normalize_state};
use tabula_commitment::PoseidonHasher;
use tabula_core::InMemoryStaticTables;
use tabula_executor::precompile::PrecompileRegistry;
use tabula_executor::property::PropertyQueryRegistry;
use tabula_ir::Program;
use tabula_machine::{MachineProofInput, TabulaMachine, TabulaProof};
use tabula_types::{EncodingRuntimeRegistry, TypeRuntimeRegistry};

use crate::bootstrap::RuntimeBuilder;
use crate::error::RuntimeError;
use crate::execute::{ExecutionEnvelope, ExecutionResources, SnapshotStateView, execute_pipeline};
use crate::policy::{
    validate_execution_state_surface, validate_proof_state_surface, validate_prove_input_prestate,
};
use crate::program::RuntimeProgram;
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
/// - **RuntimeProgram** — split execution/proof runtime contract
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
    pub fn builder(compiled_program: tabula_compiler::SealedProgram) -> RuntimeBuilder {
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

    /// The split resolved runtime contract backing this runtime.
    pub fn runtime_program(&self) -> &RuntimeProgram {
        &self.runtime_program
    }

    /// Canonical resolved execution contract consumed by the executor.
    pub fn execution_program(&self) -> &tabula_executor::ResolvedExecutionProgram {
        self.runtime_program.execution()
    }

    /// Canonical resolved proof contract consumed by runtime proving.
    pub fn proof_program(&self) -> &crate::program::ResolvedProofProgram {
        self.runtime_program.proof()
    }

    /// The IR program executed by this runtime.
    pub fn program(&self) -> &Program {
        self.proof_program().program()
    }

    /// Runtime type behavior registry.
    pub fn type_runtimes(&self) -> &TypeRuntimeRegistry {
        self.proof_program().type_runtimes()
    }

    /// Runtime encoding behavior registry.
    pub fn encoding_runtimes(&self) -> &EncodingRuntimeRegistry {
        self.proof_program().encoding_runtimes()
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

    /// Execute a batch using the runtime's owned resources.
    ///
    /// Unlike the free function [`run_batch()`](crate::run_batch), this method:
    /// - Uses `PoseidonHasher` (consistent with the proving path)
    /// - Passes registered precompiles and property query handlers to the executor
    ///
    /// Returns an [`ExecutionEnvelope`] ready for [`prove()`](Self::prove).
    #[tracing::instrument(skip_all, name = "execute")]
    pub fn execute(
        &self,
        state: &State,
        batch: &TransactionBatch,
    ) -> Result<ExecutionEnvelope, RuntimeError> {
        let hasher = PoseidonHasher::new();
        let normalized = normalize_state(state).map_err(RuntimeError::InvalidState)?;
        validate_execution_state_surface(self.execution_program(), &normalized)?;
        let committed = SnapshotStateView::from_state(&normalized, self.type_runtimes());

        execute_pipeline(
            self.execution_program(),
            &normalized,
            batch,
            &hasher,
            self.type_runtimes(),
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
        let normalized_state = self.validate_prove_input_state(input)?;
        let batch = proving::convert_batch(input.batch, self.type_runtimes())?;
        let static_tables = InMemoryStaticTables::new();
        let journal = proving::build_proof_journal(proving::JournalInput {
            resolved_program: self.proof_program(),
            state: &normalized_state,
            batch: &batch,
            execution_journal: input.executed.execution_journal(),
            static_tables: &static_tables,
        })?;
        let artifacts = proving::prepare_proof_artifacts(self.proof_program(), journal)?;

        proving::build_execution_statement(
            self.proof_program(),
            &normalized_state,
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
        let normalized_state = self.validate_prove_input_state(input)?;
        let batch = proving::convert_batch(input.batch, self.type_runtimes())?;
        let static_tables = InMemoryStaticTables::new();
        let journal = proving::build_proof_journal(proving::JournalInput {
            resolved_program: self.proof_program(),
            state: &normalized_state,
            batch: &batch,
            execution_journal: input.executed.execution_journal(),
            static_tables: &static_tables,
        })?;
        let mut artifacts = proving::prepare_proof_artifacts(self.proof_program(), journal)?;
        let statement = proving::build_execution_statement(
            self.proof_program(),
            &normalized_state,
            input.batch,
            &input.executed.state_after,
            &artifacts.air_statement,
        )?;

        let proof = {
            let _span = tracing::info_span!("stark_prove").entered();
            let traces = proving::build_traces(&self.machine, &mut artifacts)?;
            self.machine
                .prover()
                .prove(MachineProofInput {
                    traces,
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
    pub fn verify(&self, proof: &TabulaProof, statement: &Statement) -> Result<(), RuntimeError> {
        verify_with_binding(
            self.proof_program().binding(),
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

    fn validate_prove_input_state(&self, input: &ProveInput<'_>) -> Result<State, RuntimeError> {
        let normalized = normalize_state(input.state).map_err(RuntimeError::InvalidState)?;
        validate_proof_state_surface(self.proof_program(), &normalized)?;
        validate_prove_input_prestate(&normalized, &input.executed.state_before)?;
        Ok(normalized)
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
    use tabula_artifact::{State, StateEntry};
    use tabula_machine::MachineProofInput;
    use tabula_testing::assertions::assert_statement_matches_artifact;
    use tabula_testing::fixtures::examples::transfer_example_compiled_case;
    use tabula_types::u64_portable;

    use super::*;
    use crate::proving;

    fn state_with_extra_surface_cell(mut state: State) -> State {
        state.cells.push(StateEntry {
            table: 99,
            row: 0,
            col: 0,
            value: Some(u64_portable(9)),
        });
        state
    }

    fn state_with_modified_declared_value(mut state: State) -> State {
        let first = state
            .cells
            .first_mut()
            .expect("state has at least one cell");
        first.value = Some(u64_portable(999));
        state
    }

    #[test]
    fn runtime_exposes_split_resolved_contracts() {
        let case = transfer_example_compiled_case();
        let runtime = TabulaRuntime::builder(case.compiled_program)
            .build()
            .expect("runtime");
        let tx_type = runtime
            .program()
            .all_types()
            .first()
            .expect("at least one tx type")
            .id;
        let first_column = runtime
            .proof_program()
            .proof_plan()
            .column_slots()
            .first()
            .expect("at least one proof column");

        assert!(runtime.execution_program().tx_definition(tx_type).is_ok());
        assert_eq!(
            runtime.proof_program().proof_plan().column_slots().len(),
            runtime.machine().setup().proof_setups().columns.len()
        );
        assert!(
            runtime
                .runtime_program()
                .execution()
                .column_layout(first_column.table, first_column.col,)
                .is_ok()
        );
    }

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
    fn runtime_execute_rejects_state_outside_declared_surface() {
        let case = transfer_example_compiled_case();
        let runtime = TabulaRuntime::builder(case.compiled_program)
            .build()
            .expect("runtime");
        let invalid_state = state_with_extra_surface_cell(case.state.clone());

        let err = runtime
            .execute(&invalid_state, &case.batch)
            .expect_err("state outside execution surface must fail");

        match err {
            RuntimeError::ValidationFailed { detail } => {
                assert!(detail.contains("outside the declared program state surface"));
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn build_execution_statement_rejects_state_outside_declared_surface() {
        let case = transfer_example_compiled_case();
        let runtime = TabulaRuntime::builder(case.compiled_program)
            .build()
            .expect("runtime");
        let executed = runtime
            .execute(&case.state, &case.batch)
            .expect("execution succeeds");
        let invalid_state = state_with_extra_surface_cell(case.state.clone());

        let err = runtime
            .build_execution_statement(&ProveInput {
                state: &invalid_state,
                batch: &case.batch,
                executed: &executed,
            })
            .expect_err("proof input state outside declared surface must fail");

        match err {
            RuntimeError::ValidationFailed { detail } => {
                assert!(detail.contains("outside the declared program state surface"));
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn prove_rejects_state_mismatching_executed_pre_state() {
        let case = transfer_example_compiled_case();
        let runtime = TabulaRuntime::builder(case.compiled_program)
            .build()
            .expect("runtime");
        let executed = runtime
            .execute(&case.state, &case.batch)
            .expect("execution succeeds");
        let mismatched_state = state_with_modified_declared_value(case.state.clone());

        let err = runtime
            .prove(&ProveInput {
                state: &mismatched_state,
                batch: &case.batch,
                executed: &executed,
            })
            .err()
            .expect("mismatched prove input state must fail");

        match err {
            RuntimeError::ValidationFailed { detail } => {
                assert!(detail.contains("does not match the executed batch pre-state"));
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn prove_and_verify_rejects_state_mismatching_executed_pre_state() {
        let case = transfer_example_compiled_case();
        let runtime = TabulaRuntime::builder(case.compiled_program)
            .build()
            .expect("runtime");
        let executed = runtime
            .execute(&case.state, &case.batch)
            .expect("execution succeeds");
        let mismatched_state = state_with_modified_declared_value(case.state.clone());

        let err = runtime
            .prove_and_verify(&ProveInput {
                state: &mismatched_state,
                batch: &case.batch,
                executed: &executed,
            })
            .err()
            .expect("mismatched prove input state must fail");

        match err {
            RuntimeError::ValidationFailed { detail } => {
                assert!(detail.contains("does not match the executed batch pre-state"));
            }
            other => panic!("unexpected error: {other}"),
        }
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
        let batch =
            proving::convert_batch(&case.batch, runtime.type_runtimes()).expect("convert batch");
        let static_tables = InMemoryStaticTables::new();
        let journal = proving::build_proof_journal(proving::JournalInput {
            resolved_program: runtime.proof_program(),
            state: &case.state,
            batch: &batch,
            execution_journal: executed.execution_journal(),
            static_tables: &static_tables,
        })
        .expect("prepared batch journal");
        let mut prepared = proving::prepare_proof_artifacts(runtime.proof_program(), journal)
            .expect("prepared proof artifacts");
        let statement = proving::build_execution_statement(
            runtime.proof_program(),
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
