//! Unified runtime: owns machine, schemas, and precompile registry.
//!
//! [`TabulaRuntime`](crate::TabulaRuntime) is the primary entry point for applications that need
//! both execution and proving. The machine is built once at setup time
//! and reused across batches.

use tabula_artifact::{State, Statement, TransactionBatch, normalize_state};
use tabula_commitment::PoseidonHasher;
use tabula_executor::precompile::PrecompileRegistry;
use tabula_executor::property::PropertyQueryRegistry;
use tabula_ext::root::RootBackendBundle;
use tabula_ir::Program;
use tabula_machine::{TabulaMachine, TabulaProof};
use tabula_types::{EncodingRuntimeRegistry, TypeRuntimeRegistry};

use crate::bootstrap::RuntimeBuilder;
use crate::error::RuntimeError;
use crate::execute::{ExecutionEnvelope, ExecutionResources, SnapshotStateView, execute_pipeline};
use crate::policy::{
    validate_execution_state_surface, validate_proof_state_surface, validate_prove_input_prestate,
};
use crate::program::RuntimeProgram;
use crate::proving::{
    self, PreparedProofRequest, ProofSummary, ProveInput, ProveResult, VerifiedResult,
};
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
    root_backend_bundle: RootBackendBundle,
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
        root_backend_bundle: RootBackendBundle,
        machine: TabulaMachine,
        precompiles: PrecompileRegistry,
        property_queries: PropertyQueryRegistry,
    ) -> Self {
        Self {
            runtime_program,
            root_backend_bundle,
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

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn root_backend_bundle(&self) -> &RootBackendBundle {
        &self.root_backend_bundle
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
        self.prepare_proof_request(&normalized_state, input)
            .map(|request| request.statement)
    }

    /// Generate a STARK proof from an executed batch.
    ///
    /// Pipeline: column states -> witness -> traces -> prove.
    #[tracing::instrument(skip_all, name = "prove")]
    pub fn prove(&self, input: &ProveInput<'_>) -> Result<ProveResult, RuntimeError> {
        let normalized_state = self.validate_prove_input_state(input)?;
        let PreparedProofRequest {
            statement,
            machine_input,
        } = self.prepare_proof_request(&normalized_state, input)?;

        let proof = {
            let _span = tracing::info_span!("stark_prove").entered();
            self.machine
                .prove(machine_input)
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

    fn prepare_proof_request(
        &self,
        normalized_state: &State,
        input: &ProveInput<'_>,
    ) -> Result<PreparedProofRequest, RuntimeError> {
        proving::prepare_proof_request(
            self.proof_program(),
            self.type_runtimes(),
            &self.root_backend_bundle,
            normalized_state,
            input.batch,
            &input.executed.state_after,
            input.executed.execution_journal(),
        )
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
            .field("root_backend_bundle", &self.root_backend_bundle.name())
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
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use tabula_artifact::{State, StateEntry};
    use tabula_ext::root::{
        PreparedRootWitness, RootBackend, RootBackendBundle, RootWitnessContext,
        RootWitnessPreparer, SmtRootWitnessPreparer,
    };
    use tabula_testing::assertions::assert_statement_matches_artifact;
    use tabula_testing::fixtures::compiled::compiled_hash_only_case;
    use tabula_testing::fixtures::examples::transfer_example_compiled_case;
    use tabula_types::u64_portable;

    use super::*;

    #[derive(Clone, Copy, Debug)]
    struct DelegatingRootProofBackend;

    impl tabula_machine::RootProofBackend for DelegatingRootProofBackend {
        fn name(&self) -> &str {
            "delegating_root_proof"
        }

        fn supported_root_binding_families(&self) -> &'static [tabula_core::RootProfileId] {
            tabula_machine::SmtRootProofBackend.supported_root_binding_families()
        }

        fn airs(&self) -> Vec<Box<dyn tabula_machine::backend::AnyRap>> {
            tabula_machine::SmtRootProofBackend.airs()
        }

        fn dyn_chips(&self) -> Vec<Box<dyn tabula_stark::trace::DynChip>> {
            tabula_machine::SmtRootProofBackend.dyn_chips()
        }
    }

    #[derive(Debug)]
    struct CountingRootWitnessPreparer {
        calls: Arc<AtomicUsize>,
    }

    impl RootWitnessPreparer for CountingRootWitnessPreparer {
        fn name(&self) -> &str {
            "counting_root"
        }

        fn prepare_root_witness(
            &self,
            context: RootWitnessContext<'_>,
        ) -> tabula_ext::ExtResult<PreparedRootWitness> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            SmtRootWitnessPreparer.prepare_root_witness(context)
        }
    }

    #[derive(Debug)]
    struct FailingRootWitnessPreparer;

    impl RootWitnessPreparer for FailingRootWitnessPreparer {
        fn name(&self) -> &str {
            "failing_root"
        }

        fn prepare_root_witness(
            &self,
            _context: RootWitnessContext<'_>,
        ) -> tabula_ext::ExtResult<PreparedRootWitness> {
            Err(tabula_ext::ExtError::validation("intentional root failure"))
        }
    }

    #[derive(Clone, Debug)]
    struct CountingRootBackend {
        calls: Arc<AtomicUsize>,
    }

    impl RootBackend for CountingRootBackend {
        fn name(&self) -> &str {
            "counting_root_backend"
        }

        fn proof_backend(&self) -> Arc<dyn tabula_machine::RootProofBackend> {
            Arc::new(DelegatingRootProofBackend)
        }

        fn witness_preparer(&self) -> Arc<dyn RootWitnessPreparer> {
            Arc::new(CountingRootWitnessPreparer {
                calls: Arc::clone(&self.calls),
            })
        }
    }

    #[derive(Clone, Copy, Debug)]
    struct FailingRootBackend;

    impl RootBackend for FailingRootBackend {
        fn name(&self) -> &str {
            "failing_root_backend"
        }

        fn proof_backend(&self) -> Arc<dyn tabula_machine::RootProofBackend> {
            Arc::new(DelegatingRootProofBackend)
        }

        fn witness_preparer(&self) -> Arc<dyn RootWitnessPreparer> {
            Arc::new(FailingRootWitnessPreparer)
        }
    }

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
        assert!(
            !runtime
                .proof_program()
                .proof_plan()
                .column_slots()
                .is_empty()
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
    fn runtime_prove_and_verify_hash_only_program_uses_builtin_ir_hash_backend() {
        let case = compiled_hash_only_case();
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
            .expect("prove and verify hash-only program");

        assert!(verified.verified);
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
    fn machine_backend_proves_from_prepared_input() {
        let case = transfer_example_compiled_case();
        let runtime = TabulaRuntime::builder(case.compiled_program)
            .build()
            .expect("runtime");
        let executed = runtime
            .execute(&case.state, &case.batch)
            .expect("execution succeeds");
        let input = ProveInput {
            state: &case.state,
            batch: &case.batch,
            executed: &executed,
        };
        let normalized_state = runtime
            .validate_prove_input_state(&input)
            .expect("normalized prove state");
        let prepared = runtime
            .prepare_proof_request(&normalized_state, &input)
            .expect("prepared proof request");

        let proof = runtime
            .machine()
            .prove(prepared.machine_input)
            .expect("machine prove");

        runtime.machine().verify(&proof).expect("machine verify");
    }

    #[test]
    fn build_execution_statement_and_prove_share_prepared_request_path() {
        let case = transfer_example_compiled_case();
        let runtime = TabulaRuntime::builder(case.compiled_program)
            .build()
            .expect("runtime");
        let executed = runtime
            .execute(&case.state, &case.batch)
            .expect("execution succeeds");
        let input = ProveInput {
            state: &case.state,
            batch: &case.batch,
            executed: &executed,
        };
        let normalized_state = runtime
            .validate_prove_input_state(&input)
            .expect("normalized prove state");
        let prepared = runtime
            .prepare_proof_request(&normalized_state, &input)
            .expect("prepared proof request");

        let statement = runtime
            .build_execution_statement(&input)
            .expect("execution statement");
        let prove_result = runtime.prove(&input).expect("prove result");

        assert_eq!(prepared.statement, statement);
        assert_eq!(prove_result.statement, statement);
        assert_eq!(
            prepared.machine_input.statement_digest,
            statement.statement_hash_bytes()
        );
    }

    #[test]
    fn runtime_prove_invokes_custom_root_witness_preparer() {
        let case = transfer_example_compiled_case();
        let calls = Arc::new(AtomicUsize::new(0));
        let runtime = TabulaRuntime::builder(case.compiled_program)
            .with_root_backend_bundle(RootBackendBundle::new(CountingRootBackend {
                calls: Arc::clone(&calls),
            }))
            .build()
            .expect("runtime");
        let executed = runtime
            .execute(&case.state, &case.batch)
            .expect("execution succeeds");

        let proved = runtime
            .prove_and_verify(&ProveInput {
                state: &case.state,
                batch: &case.batch,
                executed: &executed,
            })
            .expect("prove and verify");

        assert!(proved.verified);
        assert!(calls.load(Ordering::SeqCst) > 0);
    }

    #[test]
    fn runtime_prove_fails_when_root_witness_preparer_fails() {
        let case = transfer_example_compiled_case();
        let runtime = TabulaRuntime::builder(case.compiled_program)
            .with_root_backend_bundle(RootBackendBundle::new(FailingRootBackend))
            .build()
            .expect("runtime");
        let executed = runtime
            .execute(&case.state, &case.batch)
            .expect("execution succeeds");

        let err = runtime
            .prove(&ProveInput {
                state: &case.state,
                batch: &case.batch,
                executed: &executed,
            })
            .err()
            .expect("failing root preparer must fail proving");

        match err {
            RuntimeError::WitnessGeneration { detail } => {
                assert!(detail.contains("failing_root"));
            }
            other => panic!("unexpected error: {other}"),
        }
    }
}
