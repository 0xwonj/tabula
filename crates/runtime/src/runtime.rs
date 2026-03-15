//! Unified runtime: owns machine, schemas, and precompile registry.
//!
//! [`TabulaRuntime`] is the primary entry point for applications that need
//! both execution and proving. The machine is built once at setup time
//! and reused across batches.

use std::collections::BTreeMap;

use tabula_artifact::{
    BatchFile, CompiledProgram, StateFile, merge_output_state_cells, normalize_state,
};
use tabula_commitment::PoseidonHasher;
use tabula_core::{
    InMemoryState, InMemoryStaticTables, NoopSigVerifier, SequentialNonce, TableId, TableSchema,
};
use tabula_executor::batch::{BatchEnv, execute_batch};
use tabula_executor::consistency::check_consistency_status;
use tabula_executor::precompile::PrecompileRegistry;
use tabula_executor::property::PropertyOpeningRegistry;
use tabula_ir::Program;
use tabula_machine::{TabulaMachine, TabulaProof};

use crate::builder::RuntimeBuilder;
use crate::committed_state::StateFileCommittedState;
use crate::error::RuntimeError;
use crate::execute::ExecutedBatch;
use crate::prove::{self, ProofSummary, ProveInput, ProveResult, VerifiedResult};

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
/// - **Program** and **schemas** — for witness generation
/// - **TabulaMachine** — STARK prover/verifier (built once from schemas)
/// - **PrecompileRegistry** — executor-side precompile handlers
/// - **PropertyOpeningRegistry** — executor-side property query resolvers
pub struct TabulaRuntime {
    compiled_program: CompiledProgram,
    schemas_by_id: BTreeMap<TableId, TableSchema>,
    machine: TabulaMachine,
    precompiles: PrecompileRegistry,
    property_openings: Option<PropertyOpeningRegistry>,
}

impl TabulaRuntime {
    /// Create a builder for customized runtime construction.
    pub fn builder(compiled_program: CompiledProgram) -> RuntimeBuilder {
        RuntimeBuilder::new(compiled_program)
    }

    /// Construct from pre-built parts (used by [`RuntimeBuilder`]).
    pub(crate) fn from_parts(
        compiled_program: CompiledProgram,
        schemas_by_id: BTreeMap<TableId, TableSchema>,
        machine: TabulaMachine,
        precompiles: PrecompileRegistry,
        property_openings: Option<PropertyOpeningRegistry>,
    ) -> Self {
        Self {
            compiled_program,
            schemas_by_id,
            machine,
            precompiles,
            property_openings,
        }
    }

    /// The compiled program artifact backing this runtime.
    pub fn compiled_program(&self) -> &CompiledProgram {
        &self.compiled_program
    }

    /// The IR program.
    pub fn program(&self) -> &Program {
        &self.compiled_program.program
    }

    /// Table schemas.
    pub fn schemas(&self) -> &[TableSchema] {
        &self.compiled_program.table_schemas
    }

    /// The STARK machine (for advanced usage).
    pub fn machine(&self) -> &TabulaMachine {
        &self.machine
    }

    /// The precompile registry (for executor integration).
    pub fn precompiles(&self) -> &PrecompileRegistry {
        &self.precompiles
    }

    /// The property opening registry (for executor PropertyRead resolution).
    pub fn property_openings(&self) -> Option<&PropertyOpeningRegistry> {
        self.property_openings.as_ref()
    }

    /// Execute a batch using the runtime's owned resources.
    ///
    /// Unlike the free function [`run_batch()`](crate::run_batch), this method:
    /// - Uses `PoseidonHasher` (consistent with the proving path)
    /// - Passes registered precompiles and property openings to the executor
    ///
    /// Returns an [`ExecutedBatch`] ready for [`prove()`](Self::prove).
    #[tracing::instrument(skip_all, name = "execute")]
    pub fn execute(
        &self,
        state: &StateFile,
        batch: &BatchFile,
    ) -> Result<ExecutedBatch, RuntimeError> {
        let normalized = normalize_state(state).map_err(RuntimeError::InvalidState)?;

        let mut state_store = InMemoryState::new();
        for cell in &normalized.cells {
            let (key, value) = cell.to_cell_pair().map_err(RuntimeError::InvalidState)?;
            state_store.set(key, value);
        }

        let batch_core = prove::convert_batch(batch)?;
        let hasher = PoseidonHasher::new();
        let st = InMemoryStaticTables::new();
        let committed = StateFileCommittedState::from_cells(&normalized.cells);
        let env = BatchEnv {
            hasher: &hasher,
            sig_verifier: &NoopSigVerifier,
            nonce_policy: &SequentialNonce,
            static_tables: &st,
            precompiles: Some(&self.precompiles),
            committed_state: Some(&committed),
            property_openings: self.property_openings.as_ref(),
        };

        let result = execute_batch(
            &batch_core,
            &self.compiled_program.program,
            &state_store,
            &env,
            &BTreeMap::new(),
        )
        .map_err(|e| RuntimeError::Execution {
            source: e,
            instruction_index: None,
            tx_index: None,
        })?;

        let all_events: Vec<_> = result.successful_events().cloned().collect();
        let consistency = check_consistency_status(&all_events, &result.read_set_old, &result.txs);

        let state_after = StateFile {
            cells: merge_output_state_cells(&normalized.cells, &result.write_set_final),
        };

        Ok(ExecutedBatch {
            state_before: normalized,
            state_after,
            txs: result.txs,
            read_set: result.read_set_old,
            write_set: result.write_set_final,
            consistency,
        })
    }

    /// Generate a STARK proof from an executed batch.
    ///
    /// Pipeline: column states -> witness -> traces -> prove.
    #[tracing::instrument(skip_all, name = "prove")]
    pub fn prove(&self, input: &ProveInput<'_>) -> Result<ProveResult, RuntimeError> {
        let old_column_states = prove::build_old_column_states(&self.schemas_by_id, input.state)?;
        let batch_result = prove::to_batch_result(input.executed);
        let batch = prove::convert_batch(input.batch)?;

        let witness =
            prove::generate_witness(&batch_result, &self.schemas_by_id, &old_column_states)?;
        let column_identities = prove::extract_column_identities(&witness);
        let statement = prove::extract_statement(&witness);

        let traces = prove::build_traces(
            &self.machine,
            &witness,
            &self.compiled_program.program,
            &batch,
            &batch_result,
            &self.schemas_by_id,
        )?;

        let proof = {
            let _span = tracing::info_span!("stark_prove").entered();
            self.machine
                .prove(traces, &column_identities, statement)
                .map_err(RuntimeError::Proving)?
        };

        let summary = ProofSummary::from_proof(&proof);
        tracing::info!(chip_count = summary.chip_count, "proof generated");

        Ok(ProveResult { proof, summary })
    }

    /// Verify a STARK proof against this runtime's prepared machine.
    #[tracing::instrument(skip_all, name = "verify")]
    pub fn verify(&self, proof: &TabulaProof) -> Result<(), RuntimeError> {
        self.machine
            .verify(proof)
            .map_err(RuntimeError::Verification)
    }

    /// Generate and verify a STARK proof.
    ///
    /// Convenience method that calls [`prove()`](Self::prove) then
    /// [`machine().verify()`](TabulaMachine::verify).
    #[tracing::instrument(skip_all, name = "prove_and_verify")]
    pub fn prove_and_verify(&self, input: &ProveInput<'_>) -> Result<VerifiedResult, RuntimeError> {
        let prove_result = self.prove(input)?;

        let verified = {
            let _span = tracing::info_span!("stark_verify").entered();
            self.verify(&prove_result.proof).is_ok()
        };

        tracing::info!(verified, "verification complete");

        Ok(VerifiedResult {
            proof: prove_result.proof,
            verified,
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
        state: &StateFile,
        batch: &BatchFile,
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
            .field("schemas", &self.compiled_program.table_schemas.len())
            .field("machine", &self.machine)
            .field("precompiles_registered", &!self.precompiles.is_empty())
            .field(
                "property_openings_registered",
                &self.property_openings.is_some(),
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
