//! Native execution and proving runtime built on `tabula_ir`.

#[cfg(feature = "prove")]
use std::collections::BTreeMap;
#[cfg(feature = "prove")]
use std::collections::BTreeSet;
use std::sync::Arc;

#[cfg(feature = "prove")]
use tabula_chips::execution::MAX_SLOTS;
#[cfg(feature = "prove")]
use tabula_chips::execution::trace::InstructionRecord;
use tabula_commitment::PoseidonHasher;
use tabula_compiler::RegisteredProgram;
#[cfg(feature = "prove")]
use tabula_contract::BoundStatement;
#[cfg(feature = "prove")]
use tabula_contract::ProofEnvelope;
#[cfg(feature = "prove")]
use tabula_contract::PublicStatement;
#[cfg(feature = "prove")]
use tabula_contract::TupleEncodingDefaults;
use tabula_contract::{ArtifactContext, ProgramBinding, SealedRelationPolicy, StaticTableArtifact};
#[cfg(feature = "prove")]
use tabula_core::{ColId, TableId};
use tabula_core::{Digest, PortableValue};
use tabula_executor as exec;
#[cfg(feature = "prove")]
use tabula_ext::backend::column::{ColumnProofContext, PreparedColumnDelta, PreparedColumnProof};
#[cfg(feature = "prove")]
use tabula_ext::root::{RootBackendBundle, RootWitnessContext};
#[cfg(all(feature = "verify", not(feature = "prove")))]
use tabula_ext::root::{RootProofBackend, SmtRootProofBackend};
use tabula_ir as ir;
#[cfg(feature = "prove")]
use tabula_machine::TabulaProof;
#[cfg(feature = "prove")]
use tabula_machine::{
    BackendProver, ColumnSlotKey, PreparedColumnInput, PreparedMachineInput, PreparedTierInput,
};
use tabula_machine::{TabulaMachine, TabulaStarkConfig};
#[cfg(feature = "prove")]
use tabula_types::StateEffectKind;
use tabula_types::{
    ContextValues, EncodingRuntimeRegistry, TxCall, TypeRuntimeRegistry, TypedValue,
};
#[cfg(feature = "prove")]
use tabula_witness::stark::prepare_execution_store;
#[cfg(feature = "prove")]
use tabula_witness::stark::{
    ChipKitRegistry, ContextPreludeSlot, LowerSuccessfulTxInput, ParamPreludeSlot,
    lower_successful_tx, merge_lowering_outputs,
};
#[cfg(feature = "prove")]
use tabula_witness::{
    AccessEvent, ColumnWrite, CommittedEntry, InitCell, PropertyReadClaim, prepare_relation_proof,
};

use crate::bootstrap::program::{
    build_registered_program_machine, resolve_program_setup, validate_core_first_program,
};
#[cfg(feature = "verify")]
use crate::error::ExecuteError;
#[cfg(feature = "prove")]
use crate::error::ProveError;
#[cfg(feature = "verify")]
use crate::error::VerifyError;
use crate::error::{RuntimeError, SetupError};
use crate::host::HostEnvironment;
#[cfg(feature = "prove")]
use crate::proof_summary::ProofSummary;
use crate::semantics as runtime_ir;
use crate::snapshot::LogicalStateCell;
// TODO(SP-5): remove this re-export once the `runtime_root_exposes_only_the_final_native_surface`
// architecture test relaxes the literal-string pin on the `pub use engine::{...}` line in
// `crates/runtime/src/lib.rs`. Until then, the relocated type must be reachable through
// `engine::` to keep that pin satisfied.
pub use crate::snapshot::CommittedStateSnapshot;
use crate::state_runtime::ResolvedStateRuntime;

/// Inputs for native proving.
#[cfg(feature = "prove")]
pub struct ProveInput<'a> {
    /// Committed pre-state.
    pub snapshot: &'a CommittedStateSnapshot,
    /// Applied transactions.
    pub batch: &'a ir::EntryBatch,
    /// Public context values.
    pub context: &'a ir::ContextInput,
    /// Execution journal returned by [`TabulaRuntime::execute_batch`].
    pub executed: &'a exec::ExecutionJournal,
}

/// Result of native proof generation.
#[cfg(feature = "prove")]
pub struct ProveResult {
    /// Generated STARK proof (decoded form).
    pub proof: TabulaProof,
    /// Wire-format envelope around the encoded proof bytes, produced by the
    /// machine backend primitive.
    pub envelope: ProofEnvelope,
    /// The artifact-bound public statement that accompanies `proof`.
    pub public_statement: PublicStatement,
    /// Human-readable machine summary.
    pub summary: ProofSummary,
}

/// Result of prove + verify.
#[cfg(feature = "prove")]
pub struct VerifiedResult {
    /// Generated STARK proof (decoded form).
    pub proof: TabulaProof,
    /// Wire-format envelope around the encoded proof bytes, produced by the
    /// machine backend primitive.
    pub envelope: ProofEnvelope,
    /// The artifact-bound public statement that accompanies `proof`.
    pub public_statement: PublicStatement,
    /// Whether verification passed.
    pub verified: bool,
    /// Human-readable machine summary.
    pub summary: ProofSummary,
}

/// Runtime-owned execution result including exact inputs and post-state.
#[cfg(feature = "verify")]
#[derive(Debug, Clone)]
pub struct ExecutionReceipt {
    /// The committed pre-state used for execution.
    pub snapshot: CommittedStateSnapshot,
    /// The exact portable entry batch that was executed.
    pub batch: ir::EntryBatch,
    /// The exact portable context input used for execution.
    pub context: ir::ContextInput,
    /// The committed post-state after applying the journal's final writes.
    pub state_after: CommittedStateSnapshot,
    /// The underlying native execution journal.
    pub journal: exec::ExecutionJournal,
}

/// Per-column proof-backend slot carried through the prepared runtime state.
#[cfg(feature = "prove")]
#[derive(Clone)]
pub(crate) struct ColumnProofSlot {
    /// Table ID for this column slot.
    table: TableId,
    /// Column ID for this column slot.
    col: ColId,
    /// Proof backend for this column.
    proof_backend: Arc<dyn tabula_ext::backend::column::ColumnProofBackend>,
}

/// Prepared runtime state derived from a registered program.
///
/// Shared between `TabulaRuntime` (execute surface) and `PreparedProver`
/// (prove surface). Construction is feature-gated; the fields marked
/// `#[cfg(feature = "prove")]` are carried only on the prove build.
#[derive(Clone)]
pub(crate) struct PreparedRuntimeState {
    pub(crate) semantic: runtime_ir::RuntimeProgram,
    pub(crate) state: ResolvedStateRuntime,
    #[cfg(feature = "prove")]
    pub(crate) column_slots: Vec<ColumnProofSlot>,
    pub(crate) artifact_context: ArtifactContext,
    pub(crate) relation_policy: SealedRelationPolicy,
    #[cfg(feature = "prove")]
    pub(crate) uses_ir_hash: bool,
    pub(crate) static_table_artifact: StaticTableArtifact,
    #[cfg(feature = "prove")]
    pub(crate) tuple_encoding_defaults: TupleEncodingDefaults,
    pub(crate) type_runtimes: TypeRuntimeRegistry,
    pub(crate) encoding_runtimes: EncodingRuntimeRegistry,
}

/// Output of `build_prepared_runtime`: the prepared state plus machine and, on prove builds,
/// the root-backend bundle.
#[cfg(feature = "verify")]
pub(crate) struct PreparedRuntimeBuild {
    pub(crate) runtime_program: PreparedRuntimeState,
    pub(crate) machine: TabulaMachine,
    #[cfg(feature = "prove")]
    pub(crate) root_backend_bundle: RootBackendBundle,
}

/// Build the [`ChipKitRegistry`] derived from a prepared runtime state.
///
/// This runs once at handle-build time (shared between `TabulaRuntime`
/// and `PreparedProver`). Per-prove work must still allocate a fresh
/// `KitScratch` — see SP-4 §2.5 on the SP-3 boundary.
#[cfg(feature = "prove")]
pub(crate) fn build_chip_kit_registry(state: &PreparedRuntimeState) -> ChipKitRegistry {
    let mut kit_registry = ChipKitRegistry::new();
    for backend in
        crate::bootstrap::program::execution_backends_for(state.uses_ir_hash, state.relation_policy)
    {
        kit_registry.register_all(backend.witness_kits());
    }
    kit_registry
}

/// Shared prove pipeline entry point used by both [`TabulaRuntime`] and [`crate::PreparedProver`].
///
/// Prepares the machine input and public statement for one already-executed tx batch.
/// All per-batch mutable state (KitScratch, column artifacts) lives in locals inside
/// this call — calling it twice with the same input produces byte-identical output.
#[cfg(feature = "prove")]
pub(crate) fn prepare_proof_request_on_prepared_state(
    state: &PreparedRuntimeState,
    root_backend_bundle: &RootBackendBundle,
    kit_registry: &ChipKitRegistry,
    machine: &TabulaMachine,
    input: &ProveInput<'_>,
) -> Result<ProveResult, RuntimeError> {
    let typed_context = decode_context_input_on_state(state, input.context)?;
    let typed_txs = decode_entry_batch_on_state(state, input.batch)?;
    let applied_tx_digest = runtime_ir::compute_applied_tx_digest(
        input.batch,
        &state.type_runtimes,
        &state.encoding_runtimes,
        &state.tuple_encoding_defaults,
    )
    .map_err(|error| VerifyError::StatementBuild {
        detail: error.to_string(),
    })?;
    let proof_artifacts = prepare_proof_artifacts(
        state,
        root_backend_bundle,
        kit_registry,
        input.snapshot,
        &typed_txs,
        &typed_context,
        input.executed,
    )?;
    let public_statement = materialize_public_statement_on_state(
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
    let summary = crate::proof_summary::ProofSummary::from_proof(&proof);
    Ok(ProveResult {
        proof,
        envelope,
        public_statement,
        summary,
    })
}

fn decode_entry_batch_on_state(
    state: &PreparedRuntimeState,
    batch: &ir::EntryBatch,
) -> Result<Vec<TxCall>, RuntimeError> {
    batch
        .calls
        .iter()
        .map(|call| decode_entry_call_on_state(state, call))
        .collect()
}

fn decode_entry_call_on_state(
    state: &PreparedRuntimeState,
    call: &ir::EntryCall,
) -> Result<TxCall, RuntimeError> {
    let entry = state
        .semantic
        .execution()
        .entry_definition(call.entry_id)
        .map_err(|error| VerifyError::Validation {
            detail: error.to_string(),
        })?;
    if entry.kind != ir::EntryKind::Tx {
        return Err(VerifyError::Validation {
            detail: format!("entry {} is not a tx entry", call.entry_id.0),
        }
        .into());
    }
    let params = decode_params_on_state(state, &entry.params, &call.params)?;
    Ok(TxCall {
        entry_id: call.entry_id,
        params,
    })
}

fn decode_params_on_state(
    state: &PreparedRuntimeState,
    expected: &[ir::ParamDecl],
    params: &[PortableValue],
) -> Result<Vec<TypedValue>, RuntimeError> {
    if expected.len() != params.len() {
        return Err(VerifyError::Validation {
            detail: format!(
                "expected {} params but received {}",
                expected.len(),
                params.len()
            ),
        }
        .into());
    }
    expected
        .iter()
        .zip(params)
        .map(|(param, value)| {
            if value.type_id() != param.ty {
                return Err(VerifyError::Validation {
                    detail: format!(
                        "param {} expects type {} but received {}",
                        param.symbol,
                        param.ty.0,
                        value.type_id().0
                    ),
                }
                .into());
            }
            state.type_runtimes.decode_portable(value).map_err(|error| {
                RuntimeError::from(VerifyError::Validation {
                    detail: error.to_string(),
                })
            })
        })
        .collect()
}

fn decode_context_input_on_state(
    state: &PreparedRuntimeState,
    context: &ir::ContextInput,
) -> Result<ContextValues, RuntimeError> {
    let mut typed = ContextValues::new();
    for (field_id, value) in &context.fields {
        let field = state
            .semantic
            .execution()
            .context_field(*field_id)
            .map_err(|error| VerifyError::Validation {
                detail: error.to_string(),
            })?;
        if value.type_id() != field.ty {
            return Err(VerifyError::Validation {
                detail: format!(
                    "context field {} expects type {} but received {}",
                    field.symbol,
                    field.ty.0,
                    value.type_id().0
                ),
            }
            .into());
        }
        let decoded = state
            .type_runtimes
            .decode_portable(value)
            .map_err(|error| VerifyError::Validation {
                detail: error.to_string(),
            })?;
        typed.insert(*field_id, decoded);
    }
    Ok(typed)
}

#[cfg(feature = "prove")]
fn materialize_public_statement_on_state(
    state: &PreparedRuntimeState,
    context: &ContextValues,
    materialization: runtime_ir::PublicStatementMaterialization,
    execution_journal: &exec::ExecutionJournal,
) -> Result<PublicStatement, RuntimeError> {
    runtime_ir::materialize_public_statement(
        state.semantic.proof(),
        context,
        execution_journal,
        materialization,
        &state.type_runtimes,
        &state.encoding_runtimes,
        &state.tuple_encoding_defaults,
    )
    .map_err(|error| {
        RuntimeError::from(VerifyError::StatementBuild {
            detail: error.to_string(),
        })
    })
}

/// Shared factory that constructs the prepared runtime state consumed by both the execute
/// and prove surfaces.
#[cfg(feature = "verify")]
pub(crate) fn build_prepared_runtime(
    registered_program: &RegisteredProgram,
    host_environment: &HostEnvironment,
    machine_stark_config: &TabulaStarkConfig,
    #[cfg(feature = "prove")] root_backend_bundle: RootBackendBundle,
    #[cfg(not(feature = "prove"))] root_proof_backend: Arc<dyn RootProofBackend>,
) -> Result<PreparedRuntimeBuild, RuntimeError> {
    validate_core_first_program(registered_program.program())?;
    let type_runtimes = host_environment
        .runtime_registries()
        .type_runtimes()
        .clone();
    let encoding_runtimes = host_environment
        .runtime_registries()
        .encoding_runtimes()
        .clone();
    #[cfg(feature = "prove")]
    let proof_backend = root_backend_bundle.proof_backend();
    #[cfg(not(feature = "prove"))]
    let proof_backend = Arc::clone(&root_proof_backend);
    #[cfg(feature = "prove")]
    let accepted_root_binding_families = root_backend_bundle.supported_root_binding_families();
    #[cfg(not(feature = "prove"))]
    let accepted_root_binding_families = proof_backend.supported_root_binding_families();
    let program_setup = resolve_program_setup(
        registered_program,
        host_environment.schemes().factories(),
        &type_runtimes,
        &encoding_runtimes,
        accepted_root_binding_families,
    )?;
    #[cfg(feature = "prove")]
    let column_slots = program_setup
        .resolved_state
        .backends()
        .map(|backend| ColumnProofSlot {
            table: backend.table_id,
            col: backend.col_id,
            proof_backend: Arc::clone(&backend.proof_backend),
        })
        .collect::<Vec<_>>();

    let semantic = runtime_ir::RuntimeProgram::from_validated_program(
        registered_program.validated_program().clone(),
    )
    .map_err(|error| SetupError::Validation {
        detail: error.to_string(),
    })?;

    let machine =
        build_registered_program_machine(&program_setup, machine_stark_config, proof_backend)?;

    let runtime_program = PreparedRuntimeState {
        semantic,
        state: program_setup.resolved_state.clone(),
        #[cfg(feature = "prove")]
        column_slots,
        artifact_context: program_setup.artifact_context,
        relation_policy: program_setup.relation_policy,
        #[cfg(feature = "prove")]
        uses_ir_hash: program_setup.uses_ir_hash,
        static_table_artifact: registered_program.static_table_artifact().clone(),
        #[cfg(feature = "prove")]
        tuple_encoding_defaults: registered_program.tuple_encoding_defaults().clone(),
        type_runtimes,
        encoding_runtimes,
    };

    Ok(PreparedRuntimeBuild {
        runtime_program,
        machine,
        #[cfg(feature = "prove")]
        root_backend_bundle,
    })
}

/// Fluent builder for the native execution/proving runtime.
pub struct RuntimeBuilder {
    registered_program: RegisteredProgram,
    host_environment: HostEnvironment,
    machine_stark_config: TabulaStarkConfig,
    #[cfg(feature = "prove")]
    root_backend_bundle: RootBackendBundle,
    #[cfg(not(feature = "prove"))]
    root_proof_backend: Arc<dyn RootProofBackend>,
}

impl RuntimeBuilder {
    fn new(registered_program: RegisteredProgram) -> Result<Self, RuntimeError> {
        registered_program
            .validate_sealed_artifact()
            .map_err(SetupError::CompilerValidation)?;
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

    /// Build the native runtime.
    pub fn build(self) -> Result<TabulaRuntime, RuntimeError> {
        let prepared = build_prepared_runtime(
            &self.registered_program,
            &self.host_environment,
            &self.machine_stark_config,
            #[cfg(feature = "prove")]
            self.root_backend_bundle,
            #[cfg(not(feature = "prove"))]
            self.root_proof_backend,
        )?;
        Ok(TabulaRuntime {
            runtime_program: prepared.runtime_program,
            machine: prepared.machine,
        })
    }
}

/// Native execution and proving runtime.
///
/// Execute-only facade. Proving is exposed through [`crate::PreparedProver`].
pub struct TabulaRuntime {
    runtime_program: PreparedRuntimeState,
    machine: TabulaMachine,
}

impl TabulaRuntime {
    /// Create a builder for one registered native program.
    pub fn builder(registered_program: RegisteredProgram) -> Result<RuntimeBuilder, RuntimeError> {
        RuntimeBuilder::new(registered_program)
    }

    /// Borrow the semantic runtime contract.
    pub fn runtime_program(&self) -> &runtime_ir::RuntimeProgram {
        &self.runtime_program.semantic
    }

    /// Borrow the canonical execution contract.
    pub fn execution_program(&self) -> &exec::ResolvedExecutionProgram {
        self.runtime_program.semantic.execution()
    }

    /// Borrow the canonical semantic proof contract.
    pub fn proof_program(&self) -> &runtime_ir::ResolvedProofProgram {
        self.runtime_program.semantic.proof()
    }

    /// Borrow the transcript-bound program binding.
    pub fn binding(&self) -> &ProgramBinding {
        &self.runtime_program.artifact_context.binding
    }

    /// Borrow the transcript-bound static relation table root.
    pub fn static_table_root(&self) -> Digest {
        self.runtime_program.artifact_context.static_table_root
    }

    /// The machine backing native proving and verification.
    pub fn machine(&self) -> &TabulaMachine {
        &self.machine
    }

    /// Installed type runtimes.
    pub fn type_runtimes(&self) -> &TypeRuntimeRegistry {
        &self.runtime_program.type_runtimes
    }

    /// Installed encoding runtimes.
    pub fn encoding_runtimes(&self) -> &EncodingRuntimeRegistry {
        &self.runtime_program.encoding_runtimes
    }

    /// Create an empty committed state snapshot for this runtime's program.
    pub fn empty_state_snapshot(&self) -> CommittedStateSnapshot {
        CommittedStateSnapshot::empty()
    }

    /// Materialize one logical keyed state input into a committed snapshot.
    pub fn materialize_logical_state<I>(
        &self,
        cells: I,
    ) -> Result<CommittedStateSnapshot, RuntimeError>
    where
        I: IntoIterator<Item = (ir::TableId, Vec<PortableValue>, ir::FieldId, PortableValue)>,
    {
        CommittedStateSnapshot::from_cells(&self.runtime_program.state, self.type_runtimes(), cells)
    }

    /// Decode and validate one committed snapshot payload against this runtime's sealed state contract.
    pub fn decode_committed_snapshot<I>(
        &self,
        cells: I,
    ) -> Result<CommittedStateSnapshot, RuntimeError>
    where
        I: IntoIterator<Item = (ir::TableId, Vec<u8>, ir::FieldId, PortableValue)>,
    {
        CommittedStateSnapshot::from_committed_cells(
            &self.runtime_program.state,
            self.type_runtimes(),
            cells,
        )
    }

    /// Project one committed snapshot back into logical keyed cells.
    pub fn project_logical_state(
        &self,
        snapshot: &CommittedStateSnapshot,
    ) -> Result<Vec<LogicalStateCell>, RuntimeError> {
        snapshot.validate(&self.runtime_program.state, self.type_runtimes())?;
        snapshot
            .cells()
            .map(|(key, value)| {
                let logical_key = self
                    .runtime_program
                    .state
                    .key_codec(key.table)?
                    .decode_key(&key.key)
                    .map_err(|error| ExecuteError::Validation {
                        detail: error.to_string(),
                    })?
                    .into_iter()
                    .map(|value| {
                        self.type_runtimes().encode_typed(&value).map_err(|source| {
                            ExecuteError::Validation {
                                detail: source.to_string(),
                            }
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok((
                    ir::TableId(key.table.0),
                    logical_key,
                    ir::FieldId(key.col.0),
                    value.clone(),
                ))
            })
            .collect()
    }

    /// Execute a canonical tx batch.
    pub fn execute_batch(
        &self,
        snapshot: &CommittedStateSnapshot,
        batch: &ir::EntryBatch,
        context: &ir::ContextInput,
    ) -> Result<exec::ExecutionJournal, RuntimeError> {
        let txs = self.decode_entry_batch(batch)?;
        let context = self.decode_context_input(context)?;
        self.execute_batch_typed(snapshot, &txs, &context)
    }

    /// Execute a canonical tx batch and return a runtime-owned receipt.
    pub fn execute_batch_receipt(
        &self,
        snapshot: &CommittedStateSnapshot,
        batch: &ir::EntryBatch,
        context: &ir::ContextInput,
    ) -> Result<ExecutionReceipt, RuntimeError> {
        let journal = self.execute_batch(snapshot, batch, context)?;
        let state_after = materialize_post_state(snapshot, &journal, self.type_runtimes())?;
        Ok(ExecutionReceipt {
            snapshot: snapshot.clone(),
            batch: batch.clone(),
            context: context.clone(),
            state_after,
            journal,
        })
    }

    fn execute_batch_typed(
        &self,
        snapshot: &CommittedStateSnapshot,
        txs: &[TxCall],
        context: &ContextValues,
    ) -> Result<exec::ExecutionJournal, RuntimeError> {
        snapshot.validate(&self.runtime_program.state, self.type_runtimes())?;
        exec::execute_batch(
            self.execution_program(),
            txs,
            context,
            snapshot,
            &exec::ExecContext {
                hasher: &PoseidonHasher::new(),
                type_runtimes: self.type_runtimes(),
                capability_executor: None,
                state_runtime: &self.runtime_program.state,
            },
        )
        .map_err(|source| {
            RuntimeError::from(ExecuteError::Execution {
                source,
                instruction_index: None,
                tx_index: None,
            })
        })
    }

    /// Execute one query entry. Query proving remains intentionally absent.
    pub fn execute_query(
        &self,
        snapshot: &CommittedStateSnapshot,
        entry_id: ir::EntryId,
        params: &[PortableValue],
        context: &ir::ContextInput,
    ) -> Result<exec::QueryExecutionResult, RuntimeError> {
        let params = self.decode_query_params(entry_id, params)?;
        let context = self.decode_context_input(context)?;
        self.execute_query_typed(snapshot, entry_id, &params, &context)
    }

    fn execute_query_typed(
        &self,
        snapshot: &CommittedStateSnapshot,
        entry_id: ir::EntryId,
        params: &[TypedValue],
        context: &ContextValues,
    ) -> Result<exec::QueryExecutionResult, RuntimeError> {
        snapshot.validate(&self.runtime_program.state, self.type_runtimes())?;
        exec::execute_query(
            self.execution_program(),
            entry_id,
            params,
            context,
            snapshot,
            &exec::ExecContext {
                hasher: &PoseidonHasher::new(),
                type_runtimes: self.type_runtimes(),
                capability_executor: None,
                state_runtime: &self.runtime_program.state,
            },
        )
        .map_err(|error| {
            RuntimeError::from(ExecuteError::Execution {
                source: error.error,
                instruction_index: Some(error.op_index),
                tx_index: None,
            })
        })
    }

    fn decode_entry_batch(&self, batch: &ir::EntryBatch) -> Result<Vec<TxCall>, RuntimeError> {
        decode_entry_batch_on_state(&self.runtime_program, batch)
    }

    fn decode_query_params(
        &self,
        entry_id: ir::EntryId,
        params: &[PortableValue],
    ) -> Result<Vec<TypedValue>, RuntimeError> {
        let entry = self
            .execution_program()
            .entry_definition(entry_id)
            .map_err(|error| ExecuteError::Validation {
                detail: error.to_string(),
            })?;
        if entry.kind != ir::EntryKind::Query {
            return Err(ExecuteError::Validation {
                detail: format!("entry {} is not a query entry", entry_id.0),
            }
            .into());
        }
        self.decode_params(&entry.params, params)
    }

    fn decode_params(
        &self,
        expected: &[ir::ParamDecl],
        params: &[PortableValue],
    ) -> Result<Vec<TypedValue>, RuntimeError> {
        decode_params_on_state(&self.runtime_program, expected, params)
    }

    fn decode_context_input(
        &self,
        context: &ir::ContextInput,
    ) -> Result<ContextValues, RuntimeError> {
        decode_context_input_on_state(&self.runtime_program, context)
    }
}

fn materialize_post_state(
    snapshot: &CommittedStateSnapshot,
    journal: &exec::ExecutionJournal,
    type_runtimes: &TypeRuntimeRegistry,
) -> Result<CommittedStateSnapshot, RuntimeError> {
    let mut state_after = snapshot.clone();
    for write in &journal.state_summary.write_set_final {
        let table = ir::TableId(write.key.table.0);
        let field = ir::FieldId(write.key.col.0);
        match &write.value {
            Some(value) => {
                let portable = type_runtimes.encode_typed(value).map_err(|source| {
                    ExecuteError::Validation {
                        detail: source.to_string(),
                    }
                })?;
                state_after.insert_materialized(write.key.clone(), portable);
            }
            None => state_after.remove_materialized(table, &write.key.key, field),
        }
    }
    Ok(state_after)
}

#[cfg(feature = "prove")]
#[derive(Clone)]
struct PreparedColumnSlot {
    table: TableId,
    col: ColId,
    old_entries: Vec<CommittedEntry>,
    init_cells: Vec<InitCell>,
    access_events: Vec<AccessEvent>,
    writes: Vec<ColumnWrite>,
    property_reads: Vec<PropertyReadClaim>,
}

#[cfg(feature = "prove")]
struct PreparedColumnArtifacts {
    input: PreparedColumnInput,
}

#[cfg(feature = "prove")]
struct PreparedArtifacts {
    public_statement: PublicStatement,
    execution: PreparedTierInput,
    columns: Vec<PreparedColumnArtifacts>,
    root: PreparedTierInput,
}

#[cfg(feature = "prove")]
impl PreparedArtifacts {
    fn into_prepared_machine_input(self, binding_digest: [u8; 32]) -> PreparedMachineInput {
        PreparedMachineInput {
            execution: self.execution,
            columns: self
                .columns
                .into_iter()
                .map(|column| column.input)
                .collect(),
            root: self.root,
            binding_digest,
        }
    }
}

#[cfg(feature = "prove")]
struct PublicStatementSlotLayout {
    aux_slot_limit: usize,
    context_slots: Vec<(ir::ContextFieldId, usize)>,
    param_slot_base: usize,
}

#[cfg(feature = "prove")]
type ContextPreludeArtifacts = (
    Vec<ContextPreludeSlot>,
    Vec<InstructionRecord>,
    Vec<[p3_koala_bear::KoalaBear; 8]>,
);

#[cfg(feature = "prove")]
fn build_public_statement_slot_layout(
    context_field_ids: &[ir::ContextFieldId],
    max_param_count: usize,
) -> Result<PublicStatementSlotLayout, RuntimeError> {
    let reserved_slots = context_field_ids
        .len()
        .checked_add(max_param_count)
        .ok_or_else(|| VerifyError::Validation {
            detail: "reserved public-statement slot count overflowed usize".to_string(),
        })?;
    if reserved_slots > MAX_SLOTS {
        return Err(VerifyError::Validation {
            detail: format!(
                "proof-visible public-statement prelude requires {reserved_slots} reserved slots, exceeding the machine ceiling of {MAX_SLOTS}"
            ),
        }.into());
    }
    let aux_slot_limit = MAX_SLOTS - reserved_slots;
    let context_slots = context_field_ids
        .iter()
        .copied()
        .enumerate()
        .map(|(index, field_id)| (field_id, aux_slot_limit + index))
        .collect::<Vec<_>>();
    Ok(PublicStatementSlotLayout {
        aux_slot_limit,
        context_slots,
        param_slot_base: aux_slot_limit + context_field_ids.len(),
    })
}

#[cfg(feature = "prove")]
fn context_public_statement_bindings(
    runtime_program: &PreparedRuntimeState,
    context: &ContextValues,
) -> Result<Vec<runtime_ir::PublicContextBinding>, RuntimeError> {
    runtime_ir::encode_public_context(
        runtime_program.semantic.proof(),
        context,
        &runtime_program.type_runtimes,
    )
    .map_err(|error| {
        RuntimeError::from(VerifyError::StatementBuild {
            detail: error.to_string(),
        })
    })
}

#[cfg(feature = "prove")]
fn build_context_prelude(
    runtime_program: &PreparedRuntimeState,
    context_bindings: &[runtime_ir::PublicContextBinding],
    layout: &PublicStatementSlotLayout,
) -> Result<ContextPreludeArtifacts, RuntimeError> {
    let canonical_bindings =
        runtime_ir::canonical_public_context(context_bindings).map_err(|error| {
            VerifyError::StatementBuild {
                detail: error.to_string(),
            }
        })?;
    let item_blocks = runtime_ir::canonical_public_context_payload(
        context_bindings,
        &runtime_program.type_runtimes,
        &runtime_program.encoding_runtimes,
        &runtime_program.tuple_encoding_defaults,
    )
    .map_err(|error| VerifyError::StatementBuild {
        detail: error.to_string(),
    })?
    .into_iter()
    .skip(1)
    .collect::<Vec<_>>();

    let mut slots = Vec::with_capacity(canonical_bindings.len());
    let mut records = Vec::with_capacity(canonical_bindings.len());
    for (item_index, binding) in canonical_bindings.iter().enumerate() {
        let slot = layout
            .context_slots
            .iter()
            .find_map(|(field_id, slot)| (*field_id == binding.field).then_some(*slot))
            .ok_or_else(|| VerifyError::Validation {
                detail: format!(
                    "missing reserved execution slot for context field {}",
                    binding.field.0
                ),
            })?;
        let typed = runtime_program
            .type_runtimes
            .decode_portable(&binding.value)
            .map_err(|source| VerifyError::StatementBuild {
                detail: source.to_string(),
            })?;
        let encoded = runtime_ir::encode_public_statement_value(
            &typed,
            &runtime_program.encoding_runtimes,
            &runtime_program.tuple_encoding_defaults,
        )
        .map_err(|source| VerifyError::StatementBuild {
            detail: source.to_string(),
        })?;
        slots.push(ContextPreludeSlot {
            field_id: binding.field,
            slot,
            value: typed.clone(),
            encoded: encoded.field_elements.to_vec(),
        });
        records.push(InstructionRecord {
            opcode: tabula_chips::execution::trace::Opcode::LoadContext,
            tx_index: 0,
            proof_meta0: Some(item_index as u32),
            proof_meta1: Some(binding.field.0),
            proof_meta2: Some(encoded.type_id.0),
            written_slots: vec![slot],
            src1_val: encoded.field_elements.to_vec(),
            writes: vec![(slot, encoded.field_elements.to_vec(), false)],
            ..InstructionRecord::default()
        });
    }
    Ok((slots, records, item_blocks))
}

#[cfg(feature = "prove")]
fn build_param_prelude(
    runtime_program: &PreparedRuntimeState,
    layout: &PublicStatementSlotLayout,
    entry: &ir::Entry,
    call: &TxCall,
    tx_item_index_base: u32,
    tx_index: u32,
) -> Result<(Vec<ParamPreludeSlot>, Vec<InstructionRecord>), RuntimeError> {
    let mut slots = Vec::with_capacity(entry.params.len());
    let mut records = Vec::with_capacity(entry.params.len() + 1);

    records.push(InstructionRecord {
        opcode: tabula_chips::execution::trace::Opcode::TxBegin,
        tx_index,
        proof_meta0: Some(tx_item_index_base),
        proof_meta1: Some(call.entry_id.0),
        proof_meta2: Some(entry.params.len() as u32),
        ..InstructionRecord::default()
    });

    for (param_index, param) in entry.params.iter().enumerate() {
        let value =
            call.params
                .get(param_index)
                .cloned()
                .ok_or_else(|| VerifyError::Validation {
                    detail: format!(
                        "tx {tx_index} is missing parameter {} for entry {}",
                        param.symbol, entry.symbol
                    ),
                })?;
        let encoded = runtime_ir::encode_public_statement_value(
            &value,
            &runtime_program.encoding_runtimes,
            &runtime_program.tuple_encoding_defaults,
        )
        .map_err(|source| VerifyError::StatementBuild {
            detail: source.to_string(),
        })?;
        let slot = layout.param_slot_base + param_index;
        slots.push(ParamPreludeSlot {
            param_id: param.id,
            slot,
            value: value.clone(),
            encoded: encoded.field_elements.to_vec(),
        });

        records.push(InstructionRecord {
            opcode: tabula_chips::execution::trace::Opcode::LoadParam,
            tx_index,
            proof_meta0: Some(tx_item_index_base + 1 + param_index as u32),
            proof_meta1: Some(param_index as u32),
            proof_meta2: Some(encoded.type_id.0),
            written_slots: vec![slot],
            src1_val: encoded.field_elements.to_vec(),
            writes: vec![(slot, encoded.field_elements.to_vec(), false)],
            ..InstructionRecord::default()
        });
    }

    Ok((slots, records))
}

#[cfg(feature = "prove")]
fn build_event_item_bases(
    executed: &exec::ExecutionJournal,
) -> (
    BTreeMap<u32, BTreeMap<usize, u32>>,
    Vec<runtime_ir::ProofEventEffect>,
) {
    let mut per_tx = BTreeMap::new();
    let mut events = Vec::new();
    let mut next_item_index = 0u32;

    for tx in executed.successful_txs() {
        let mut per_op = BTreeMap::new();
        for effect in &tx.event_effects {
            per_op.insert(effect.op_index, next_item_index);
            next_item_index += 1 + effect.args.len() as u32;
            events.push(runtime_ir::ProofEventEffect {
                tx_index: tx.tx_index,
                effect: effect.clone(),
            });
        }
        per_tx.insert(tx.tx_index, per_op);
    }

    (per_tx, events)
}

#[cfg(feature = "prove")]
fn prepare_proof_artifacts(
    runtime_program: &PreparedRuntimeState,
    root_backend_bundle: &RootBackendBundle,
    kit_registry: &ChipKitRegistry,
    snapshot: &CommittedStateSnapshot,
    txs: &[TxCall],
    context: &ContextValues,
    executed: &exec::ExecutionJournal,
) -> Result<PreparedArtifacts, RuntimeError> {
    let mut column_slots = Vec::with_capacity(runtime_program.column_slots.len());
    for slot in &runtime_program.column_slots {
        column_slots.push(PreparedColumnSlot {
            table: slot.table,
            col: slot.col,
            old_entries: snapshot.committed_entries(
                slot.table,
                slot.col,
                &runtime_program.type_runtimes,
            )?,
            init_cells: Vec::new(),
            access_events: Vec::new(),
            writes: Vec::new(),
            property_reads: Vec::new(),
        });
    }
    let column_index = runtime_program
        .column_slots
        .iter()
        .enumerate()
        .map(|(index, slot)| ((slot.table, slot.col), index))
        .collect::<BTreeMap<_, _>>();
    let empty_columns = runtime_program
        .column_slots
        .iter()
        .zip(column_slots.iter())
        .filter_map(|(slot, prepared)| {
            prepared
                .old_entries
                .is_empty()
                .then_some((ir::TableId(slot.table.0), ir::FieldId(slot.col.0)))
        })
        .collect::<BTreeSet<_>>();

    for entry in &executed.state_summary.read_set_old {
        let slot = *column_index
            .get(&(entry.key.table, entry.key.col))
            .ok_or_else(|| ProveError::WitnessGeneration {
                detail: format!(
                    "read-set column ({}, {}) missing from the proof plan",
                    entry.key.table.0, entry.key.col.0
                ),
            })?;
        let value = match &entry.value {
            Some(value) => value.clone(),
            None => runtime_program
                .type_runtimes
                .zero_of(entry.type_id)
                .map_err(|source| ProveError::WitnessGeneration {
                    detail: source.to_string(),
                })?,
        };
        column_slots[slot].init_cells.push(InitCell {
            key: entry.key.clone(),
            value,
            is_null: entry.value.is_none(),
        });
    }
    for entry in &executed.state_summary.write_set_final {
        let slot = *column_index
            .get(&(entry.key.table, entry.key.col))
            .ok_or_else(|| ProveError::WitnessGeneration {
                detail: format!(
                    "write-set column ({}, {}) missing from the proof plan",
                    entry.key.table.0, entry.key.col.0
                ),
            })?;
        column_slots[slot].writes.push(ColumnWrite {
            key: entry.key.key.clone(),
            value: entry.value.clone(),
        });
    }

    let context_bindings = context_public_statement_bindings(runtime_program, context)?;
    let canonical_context_ids = runtime_ir::canonical_public_context(&context_bindings)
        .map_err(|error| VerifyError::StatementBuild {
            detail: error.to_string(),
        })?
        .into_iter()
        .map(|binding| binding.field)
        .collect::<Vec<_>>();
    let max_param_count = txs.iter().map(|call| call.params.len()).max().unwrap_or(0);
    let statement_slot_layout =
        build_public_statement_slot_layout(&canonical_context_ids, max_param_count)?;
    let (context_slots, context_records, public_context_transcript_items) =
        build_context_prelude(runtime_program, &context_bindings, &statement_slot_layout)?;

    let (event_item_bases_by_tx, proof_events) = build_event_item_bases(executed);
    let event_transcript_items = runtime_ir::canonical_event_log_payload(
        &proof_events,
        &runtime_program.encoding_runtimes,
        &runtime_program.tuple_encoding_defaults,
    )
    .map_err(|error| VerifyError::StatementBuild {
        detail: error.to_string(),
    })?
    .into_iter()
    .skip(1)
    .collect::<Vec<_>>();

    let portable_batch = ir::EntryBatch {
        calls: txs
            .iter()
            .map(|call| {
                let params = call
                    .params
                    .iter()
                    .map(|value| runtime_program.type_runtimes.encode_typed(value))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|source| VerifyError::StatementBuild {
                        detail: source.to_string(),
                    })?;
                Ok(ir::EntryCall {
                    entry_id: call.entry_id,
                    params,
                })
            })
            .collect::<Result<Vec<_>, RuntimeError>>()?,
    };
    let tx_batch_transcript_items = runtime_ir::canonical_batch_payload(
        &portable_batch,
        &runtime_program.type_runtimes,
        &runtime_program.encoding_runtimes,
        &runtime_program.tuple_encoding_defaults,
    )
    .map_err(|error| VerifyError::StatementBuild {
        detail: error.to_string(),
    })?
    .into_iter()
    .skip(1)
    .collect::<Vec<_>>();

    let mut tx_prelude_by_index = BTreeMap::new();
    let mut next_tx_item_index = 0u32;
    for (tx_index, call) in txs.iter().enumerate() {
        let entry = runtime_program
            .semantic
            .execution()
            .entry_definition(call.entry_id)
            .map_err(|error| ProveError::WitnessGeneration {
                detail: error.to_string(),
            })?;
        let (param_slots, records) = build_param_prelude(
            runtime_program,
            &statement_slot_layout,
            entry,
            call,
            next_tx_item_index,
            tx_index as u32,
        )?;
        next_tx_item_index += 1 + call.params.len() as u32;
        tx_prelude_by_index.insert(tx_index as u32, (param_slots, records));
    }

    let mut lowered_txs = BTreeMap::new();
    let empty_event_item_bases = BTreeMap::new();
    let mut kit_scratch = tabula_stark::witness_kit::KitScratch::new();
    for tx in executed.successful_txs() {
        for effect in &tx.state_effects {
            let slot = *column_index
                .get(&(effect.key.table, effect.key.col))
                .ok_or_else(|| ProveError::WitnessGeneration {
                    detail: format!(
                        "state effect column ({}, {}) missing from the proof plan",
                        effect.key.table.0, effect.key.col.0
                    ),
                })?;
            let value = match &effect.value {
                Some(value) => value.clone(),
                None => runtime_program
                    .type_runtimes
                    .zero_of(effect.type_id)
                    .map_err(|source| ProveError::WitnessGeneration {
                        detail: source.to_string(),
                    })?,
            };
            column_slots[slot].access_events.push(AccessEvent {
                key: effect.key.clone(),
                time: effect.logical_time,
                is_write: matches!(
                    effect.kind,
                    StateEffectKind::Write | StateEffectKind::Delete
                ),
                value,
                is_null: effect.value.is_none(),
                tx_index: tx.tx_index,
                effect_ordinal_in_tx: effect.effect_ordinal_in_entry,
            });
        }
        for effect in &tx.property_effects {
            let slot = *column_index
                .get(&(effect.table.into(), effect.field.into()))
                .ok_or_else(|| ProveError::WitnessGeneration {
                    detail: format!(
                        "property effect column ({}, {}) missing from the proof plan",
                        effect.table.0, effect.field.0
                    ),
                })?;
            column_slots[slot].property_reads.push(PropertyReadClaim {
                query: effect.query.clone(),
                result: effect.result.clone(),
            });
        }

        let call = txs
            .get(tx.tx_index as usize)
            .ok_or_else(|| ProveError::WitnessGeneration {
                detail: format!("missing tx call {} during witness lowering", tx.tx_index),
            })?;
        let entry = runtime_program
            .semantic
            .execution()
            .entry_definition(tx.entry_id)
            .map_err(|error| ProveError::WitnessGeneration {
                detail: error.to_string(),
            })?;
        let (param_slots, _) =
            tx_prelude_by_index
                .get(&tx.tx_index)
                .ok_or_else(|| ProveError::WitnessGeneration {
                    detail: format!(
                        "missing reserved parameter prelude for tx {} during witness lowering",
                        tx.tx_index
                    ),
                })?;
        lowered_txs.insert(
            tx.tx_index,
            lower_successful_tx::<3>(
                LowerSuccessfulTxInput {
                    tx_index: tx.tx_index,
                    program: runtime_program.semantic.execution().program(),
                    call,
                    entry,
                    context,
                    state_effects: &tx.state_effects,
                    event_effects: &tx.event_effects,
                    property_effects: &tx.property_effects,
                    relation_effects: &tx.relation_effects,
                    empty_columns: &empty_columns,
                    type_runtimes: &runtime_program.type_runtimes,
                    encoding_runtimes: &runtime_program.encoding_runtimes,
                    tuple_encoding_defaults: &runtime_program.tuple_encoding_defaults,
                    hasher: &PoseidonHasher::new(),
                    state_runtime: &runtime_program.state,
                    context_slots: context_slots.as_slice(),
                    param_slots: param_slots.as_slice(),
                    aux_slot_limit: statement_slot_layout.aux_slot_limit,
                    event_item_bases: event_item_bases_by_tx
                        .get(&tx.tx_index)
                        .unwrap_or(&empty_event_item_bases),
                },
                &mut kit_scratch,
            )
            .map_err(ProveError::TraceBuild)?,
        );
    }

    let mut lowered = merge_lowering_outputs(lowered_txs.values(), kit_scratch);
    let mut instruction_records = context_records;
    for tx_index in 0..txs.len() {
        let (_, prelude_records) =
            tx_prelude_by_index.get(&(tx_index as u32)).ok_or_else(|| {
                ProveError::WitnessGeneration {
                    detail: format!("missing tx prelude for tx {tx_index}"),
                }
            })?;
        instruction_records.extend(prelude_records.iter().cloned());
        if let Some(lowered_tx) = lowered_txs.get(&(tx_index as u32)) {
            instruction_records.extend(lowered_tx.instruction_records.iter().cloned());
        }
    }
    lowered.instruction_records = instruction_records;
    tabula_chips::public_context_transcript::PublicContextTranscriptKit::insert_items(
        &mut lowered.kit_scratch,
        public_context_transcript_items,
    );
    tabula_chips::tx_batch_transcript::TxBatchTranscriptKit::insert_items(
        &mut lowered.kit_scratch,
        tx_batch_transcript_items,
    );
    tabula_chips::event_transcript::EventTranscriptKit::insert_items(
        &mut lowered.kit_scratch,
        event_transcript_items,
    );
    let relation_proof = prepare_relation_proof(
        runtime_program.semantic.execution().program(),
        &runtime_program.static_table_artifact,
        &lowered.relation_claims,
    )
    .map_err(|source| ProveError::WitnessGeneration {
        detail: source.to_string(),
    })?;
    if relation_proof.root() != runtime_program.static_table_artifact.root {
        return Err(ProveError::WitnessGeneration {
            detail: "prepared relation proof root diverged from the registered static table root"
                .to_string(),
        }
        .into());
    }

    tabula_chips::relation_table::RelationTableKit::insert_rows(
        &mut lowered.kit_scratch,
        relation_proof
            .table_rows()
            .iter()
            .map(
                |row| tabula_chips::relation_table::RelationTableWitnessRow {
                    relation_id: row.relation_id,
                    input_digest: row.input_digest,
                    output_digest: row.output_digest,
                    lookup_mult: row.lookup_mult,
                },
            )
            .collect(),
    );
    let execution_store =
        prepare_execution_store(&mut lowered, kit_registry).map_err(ProveError::TraceBuild)?;

    let prepared_columns = runtime_program
        .column_slots
        .iter()
        .zip(column_slots.into_iter())
        .map(|(slot, mut prepared)| {
            synthesize_missing_init_cells(runtime_program, slot, &mut prepared)?;
            prepare_column_slot(runtime_program, slot, prepared)
        })
        .collect::<Result<Vec<_>, RuntimeError>>()?;

    let root_bindings = prepared_columns
        .iter()
        .filter_map(|(_, _, proof)| proof.root_binding.clone())
        .collect::<Vec<_>>();
    let witness_preparer = root_backend_bundle.witness_preparer();
    let prepared_root = witness_preparer
        .prepare_root_witness(RootWitnessContext::new(&root_bindings))
        .map_err(|error| {
            let detail = match error {
                tabula_ext::ExtError::Validation { detail } => detail,
                #[cfg(feature = "verify")]
                tabula_ext::ExtError::Setup(source) => source.to_string(),
                tabula_ext::ExtError::RuntimeHook(source)
                | tabula_ext::ExtError::ProofPreparation(source) => source.to_string(),
            };
            ProveError::WitnessGeneration {
                detail: format!(
                    "root witness preparer '{}': {detail}",
                    witness_preparer.name(),
                ),
            }
        })?;
    let (public_statement, root_store) = prepared_root.into_parts();

    Ok(PreparedArtifacts {
        public_statement,
        execution: PreparedTierInput {
            store: execution_store,
        },
        columns: prepared_columns
            .into_iter()
            .map(|(table, col, proof)| PreparedColumnArtifacts {
                input: PreparedColumnInput {
                    key: ColumnSlotKey { table, col },
                    store: proof.store,
                },
            })
            .collect(),
        root: PreparedTierInput { store: root_store },
    })
}

#[cfg(feature = "prove")]
fn synthesize_missing_init_cells(
    runtime_program: &PreparedRuntimeState,
    slot: &ColumnProofSlot,
    prepared: &mut PreparedColumnSlot,
) -> Result<(), RuntimeError> {
    let mut present_rows = prepared
        .init_cells
        .iter()
        .map(|cell| cell.key.key.clone())
        .collect::<BTreeSet<_>>();
    let touched_rows = prepared
        .access_events
        .iter()
        .map(|event| event.key.key.clone())
        .chain(prepared.writes.iter().map(|write| write.key.clone()))
        .collect::<BTreeSet<_>>();
    let old_entries = prepared
        .old_entries
        .iter()
        .map(|entry| (entry.key.clone(), (entry.value.clone(), entry.is_null)))
        .collect::<BTreeMap<_, _>>();
    let required_rows = old_entries
        .keys()
        .cloned()
        .chain(touched_rows.iter().cloned())
        .collect::<BTreeSet<_>>();
    if required_rows.is_empty() {
        return Ok(());
    }
    let field_ty = runtime_program
        .state
        .column_contract(slot.table, slot.col)?
        .ty;

    for row in required_rows {
        if present_rows.contains(&row) {
            continue;
        }
        let (value, is_null) = match old_entries.get(&row) {
            Some((value, is_null)) => (value.clone(), *is_null),
            None => (
                runtime_program
                    .type_runtimes
                    .zero_of(field_ty)
                    .map_err(|source| ProveError::WitnessGeneration {
                        detail: source.to_string(),
                    })?,
                true,
            ),
        };
        prepared.init_cells.push(InitCell {
            key: tabula_core::CommittedCellKey {
                table: slot.table,
                col: slot.col,
                key: row.clone(),
            },
            value,
            is_null,
        });
        present_rows.insert(row);
    }
    prepared.init_cells.sort_by_key(|cell| cell.key.key.clone());
    Ok(())
}

#[cfg(feature = "prove")]
fn prepare_column_slot(
    runtime_program: &PreparedRuntimeState,
    slot: &ColumnProofSlot,
    prepared: PreparedColumnSlot,
) -> Result<(TableId, ColId, PreparedColumnProof), RuntimeError> {
    let backend = runtime_program.state.backend(slot.table, slot.col)?;
    let proof = slot
        .proof_backend
        .prepare_column(ColumnProofContext {
            column: PreparedColumnDelta {
                table: prepared.table,
                col: prepared.col,
                init_cells: prepared.init_cells,
                access_events: prepared.access_events,
                writes: prepared.writes.clone(),
                is_touched: !prepared.writes.is_empty(),
            },
            old_entries: prepared.old_entries,
            property_reads: prepared.property_reads,
        })
        .map_err(|error| {
            let detail = match error {
                tabula_ext::ExtError::Validation { detail } => detail,
                #[cfg(feature = "verify")]
                tabula_ext::ExtError::Setup(source) => source.to_string(),
                tabula_ext::ExtError::RuntimeHook(source)
                | tabula_ext::ExtError::ProofPreparation(source) => source.to_string(),
            };
            ProveError::WitnessGeneration { detail }
        })?;
    match (
        &proof.root_binding,
        backend.root_binding_contract.receives_commitment,
    ) {
        (Some(binding), true) => {
            if binding.table != slot.table
                || binding.col != slot.col
                || binding.root_binding_family != backend.root_binding_contract.root_binding_family
                || binding.column_profile_hash != backend.root_binding_contract.column_profile_hash
                || binding.binding_digest != backend.root_binding_contract.binding_digest
            {
                return Err(ProveError::WitnessGeneration {
                    detail: format!(
                        "prepared column proof ({}, {}) returned a root binding that does not match the sealed backend contract",
                        slot.table.0, slot.col.0,
                    ),
                }.into());
            }
        }
        (None, true) => {
            return Err(ProveError::WitnessGeneration {
                detail: format!(
                    "prepared column proof ({}, {}) omitted a required root binding",
                    slot.table.0, slot.col.0,
                ),
            }
            .into());
        }
        (Some(_), false) => {
            return Err(ProveError::WitnessGeneration {
                detail: format!(
                    "prepared column proof ({}, {}) returned an unexpected root binding",
                    slot.table.0, slot.col.0,
                ),
            }
            .into());
        }
        (None, false) => {}
    }
    Ok((slot.table, slot.col, proof))
}

/// Prepare the machine input and public statement without running the prover.
///
/// Exposed only for tests that need to tamper with witness store contents
/// before proving. Production code must use [`prepare_proof_request_on_prepared_state`]
/// or [`prepare_proof_request_on_prepared_state`] instead.
#[cfg(all(test, feature = "prove"))]
fn prepare_proof_machine_input(
    state: &PreparedRuntimeState,
    root_backend_bundle: &RootBackendBundle,
    kit_registry: &ChipKitRegistry,
    input: &ProveInput<'_>,
) -> Result<(PreparedMachineInput, PublicStatement), RuntimeError> {
    let typed_context = decode_context_input_on_state(state, input.context)?;
    let typed_txs = decode_entry_batch_on_state(state, input.batch)?;
    let applied_tx_digest = runtime_ir::compute_applied_tx_digest(
        input.batch,
        &state.type_runtimes,
        &state.encoding_runtimes,
        &state.tuple_encoding_defaults,
    )
    .map_err(|error| VerifyError::StatementBuild {
        detail: error.to_string(),
    })?;
    let proof_artifacts = prepare_proof_artifacts(
        state,
        root_backend_bundle,
        kit_registry,
        input.snapshot,
        &typed_txs,
        &typed_context,
        input.executed,
    )?;
    let public_statement = materialize_public_statement_on_state(
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
    Ok((machine_input, public_statement))
}

#[cfg(all(test, feature = "prove"))]
mod relation_proof_tests {
    use super::*;
    use crate::PreparedVerifier;
    use crate::verifier::relation_table_root_from_proof;
    use tabula_core::error::TabulaError;

    use std::cmp::Ordering;
    use std::sync::Arc;

    use p3_field::PrimeCharacteristicRing;
    use p3_koala_bear::KoalaBear;
    use tabula_chips::event_transcript::EVENT_TRANSCRIPT_WITNESS_LABEL;
    use tabula_chips::execution::EXECUTION_STANDARD_VALUE_WIDTH;
    use tabula_chips::execution::trace::{InstructionRecord, Opcode};
    use tabula_chips::relation_table::RELATION_TABLE_CHIP_ID;
    use tabula_chips::relation_table::{RELATION_TABLE_WITNESS_LABEL, RelationTableWitnessRow};
    use tabula_chips::relation_transcript::{
        RELATION_TRANSCRIPT_WITNESS_LABEL, RelationTranscriptCall,
    };
    use tabula_contract::format::typed_tuple::{TypedTupleRole, compute_typed_tuple_digest};
    use tabula_core::{EncodingProfileId, PortableValue, TypeId};
    use tabula_ext::root::{
        RootBackend, RootBackendBundle, RootWitnessPreparer, SmtRootWitnessPreparer,
    };
    use tabula_machine::{BackendProver, RootProofBackend, SmtRootProofBackend};
    use tabula_profile::{
        CanonicalNullEncoding, EncodingClass, EncodingProfile, FieldFamily, GenericIrFamily,
        HostValueFamily, NullSemantics, TranscriptSerialization, TypeCapabilities, TypeDescriptor,
        ZeroValueSpec,
    };
    use tabula_stark::trace::witness_labels;
    use tabula_testing::exec::{
        context_input, register_program_from_source, register_program_from_source_with_catalogs,
        tx_batch,
    };
    use tabula_types::{
        ArithmeticOp, EncodingRuntime, TypeRuntime, TypedValue, bool_portable, u64_portable,
        u64_typed,
    };
    use tabula_witness::stark::{LowerSuccessfulTxInput, lower_successful_tx};
    use tabula_witness::{RelationClaim, RelationClaimKind, prepare_relation_proof};

    const TEST_EXTRA_TYPE_ID: TypeId = TypeId(90_001);
    const TEST_EXTRA_ENCODING_ID: EncodingProfileId = EncodingProfileId(90_001);

    fn relation_source() -> &'static str {
        r#"
program RelationProof

context {
  caller: u64;
  epoch: u64;
}

state {
  table accounts(key id: u64) {
    tier: u64 @ssmc;
  }
}

relation AllowedTier(tier: u64) = enum { 0, 1, 2, 3 };
relation ValidEpoch(epoch: u64) = range(10, 13);
relation PreferredCaller(actor: u64) = set { 7, 8 };
relation PromoteTier(tier: u64) -> promoted: u64 = map {
  0 => 1,
  1 => 2,
  2 => 3,
  3 => 3,
};

tx enroll(flag: bool, id: u64, tier: u64) {
  assert relation AllowedTier(tier);
  assert relation ValidEpoch(epoch);
  if flag {
    let promoted = eval relation PromoteTier(tier);
    accounts[id].tier = promoted;
  } else {
    assert relation PreferredCaller(caller);
  }
  return;
}
"#
    }

    fn event_debug_source() -> &'static str {
        r#"
program EventTranscriptDebug

context {
  caller: u64;
}

event Registered(id: u64, actor: u64);

tx register(id: u64) {
  emit Registered(id, caller);
  return;
}
"#
    }

    fn extract_event_items(records: &[InstructionRecord]) -> Vec<(u32, [KoalaBear; 8])> {
        let mut items = records
            .iter()
            .filter_map(|record| match record.opcode {
                Opcode::EmitEventHeader => Some((
                    record.proof_meta0.expect("event header item index"),
                    [
                        KoalaBear::ONE,
                        KoalaBear::new(record.tx_index),
                        KoalaBear::new(
                            record
                                .instruction_index
                                .expect("event header instruction index"),
                        ),
                        KoalaBear::new(record.proof_meta1.expect("event header ordinal")),
                        KoalaBear::new(record.proof_meta2.expect("event header id")),
                        KoalaBear::new(record.proof_meta3.expect("event header arg count")),
                        KoalaBear::ZERO,
                        KoalaBear::ZERO,
                    ],
                )),
                Opcode::EmitEventArg => Some((
                    record.proof_meta0.expect("event arg item index"),
                    [
                        KoalaBear::new(2),
                        KoalaBear::new(record.tx_index),
                        KoalaBear::new(record.proof_meta1.expect("event arg ordinal")),
                        KoalaBear::new(record.proof_meta2.expect("event arg index")),
                        KoalaBear::new(record.proof_meta3.expect("event arg type id")),
                        *record.src1_val.first().expect("event arg limb 0"),
                        *record.src1_val.get(1).expect("event arg limb 1"),
                        *record.src1_val.get(2).expect("event arg limb 2"),
                    ],
                )),
                _ => None,
            })
            .collect::<Vec<_>>();
        items.sort_unstable_by_key(|(item_index, _)| *item_index);
        items
    }

    fn guarded_relation_source() -> &'static str {
        r#"
program GuardedRelation

context {
  caller: u64;
}

state {
  table accounts(key id: u64) {
    tier: u64 @ssmc;
  }
}

relation PromoteTier(tier: u64) -> promoted: u64 = map {
  1 => 2,
  2 => 3,
  3 => 3,
};

tx maybe_promote(flag: bool, id: u64, tier: u64) {
  if flag {
    let promoted = eval relation PromoteTier(tier);
    accounts[id].tier = promoted;
  } else {
    assert true;
  }
  return;
}
"#
    }

    fn capability_source() -> &'static str {
        r#"
use capability demo_hash;

program DeferredCapability

tx scan(id: u64) {
  let digest = demo_hash(id);
  assert true;
  return;
}
"#
    }

    fn relation_context(caller: u64, epoch: u64) -> ir::ContextInput {
        context_input([
            (ir::ContextFieldId(0), u64_portable(caller)),
            (ir::ContextFieldId(1), u64_portable(epoch)),
        ])
    }

    fn guarded_context(caller: u64) -> ir::ContextInput {
        context_input([(ir::ContextFieldId(0), u64_portable(caller))])
    }

    fn relation_snapshot(registered: &RegisteredProgram) -> CommittedStateSnapshot {
        let runtime = TabulaRuntime::builder(registered.clone())
            .expect("create runtime builder")
            .build()
            .expect("build runtime");
        runtime
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
            .expect("build relation snapshot")
    }

    fn runtime_for_source(
        source: &str,
    ) -> (RegisteredProgram, TabulaRuntime, crate::PreparedProver) {
        let registered = register_program_from_source(source);
        let runtime = TabulaRuntime::builder(registered.clone())
            .expect("create runtime builder")
            .build()
            .expect("build runtime");
        let prover = crate::PreparedProver::builder(registered.clone())
            .expect("create prover builder")
            .build()
            .expect("build prepared prover");
        (registered, runtime, prover)
    }

    #[derive(Debug)]
    struct EmptyFamilyRootProofBackend;

    impl RootProofBackend for EmptyFamilyRootProofBackend {
        fn name(&self) -> &str {
            "empty_family_root_proof"
        }

        fn supported_root_binding_families(&self) -> &'static [tabula_core::RootProfileId] {
            &[]
        }

        fn airs(&self) -> Vec<Box<dyn tabula_machine::backend::AnyRap>> {
            SmtRootProofBackend.airs()
        }

        fn dyn_chips(&self) -> Vec<Box<dyn tabula_stark::trace::DynChip>> {
            SmtRootProofBackend.dyn_chips()
        }
    }

    #[derive(Clone, Copy, Debug, Default)]
    struct EmptyFamilyRootBackend;

    impl RootBackend for EmptyFamilyRootBackend {
        fn name(&self) -> &str {
            "empty_family_root"
        }

        fn proof_backend(&self) -> Arc<dyn RootProofBackend> {
            Arc::new(EmptyFamilyRootProofBackend)
        }

        fn witness_preparer(&self) -> Arc<dyn RootWitnessPreparer> {
            Arc::new(SmtRootWitnessPreparer)
        }
    }

    #[test]
    fn committed_snapshot_decode_rejects_duplicate_cells() {
        let (_registered, runtime, _prover) = runtime_for_source(relation_source());
        let error = runtime
            .decode_committed_snapshot([
                (
                    ir::TableId(0),
                    0u64.to_le_bytes().to_vec(),
                    ir::FieldId(0),
                    u64_portable(1),
                ),
                (
                    ir::TableId(0),
                    0u64.to_le_bytes().to_vec(),
                    ir::FieldId(0),
                    u64_portable(2),
                ),
            ])
            .expect_err("duplicate committed cells must fail");

        assert!(
            error
                .to_string()
                .contains("duplicate committed cell 0.0 key"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn logical_state_materialization_rejects_duplicate_cells() {
        let (_registered, runtime, _prover) = runtime_for_source(relation_source());
        let error = runtime
            .materialize_logical_state([
                (
                    ir::TableId(0),
                    vec![u64_portable(0)],
                    ir::FieldId(0),
                    u64_portable(1),
                ),
                (
                    ir::TableId(0),
                    vec![u64_portable(0)],
                    ir::FieldId(0),
                    u64_portable(2),
                ),
            ])
            .expect_err("duplicate logical cells must fail");

        assert!(
            error
                .to_string()
                .contains("duplicate logical state cell 0.0 key"),
            "unexpected error: {error}"
        );
    }

    fn entry_id(runtime: &TabulaRuntime, symbol: &str) -> ir::EntryId {
        runtime
            .execution_program()
            .program()
            .entries
            .iter()
            .find(|entry| entry.symbol == symbol)
            .map_or_else(|| panic!("missing entry '{symbol}'"), |entry| entry.id)
    }

    fn prove_input<'a>(
        snapshot: &'a CommittedStateSnapshot,
        batch: &'a ir::EntryBatch,
        context: &'a ir::ContextInput,
        executed: &'a exec::ExecutionJournal,
    ) -> ProveInput<'a> {
        ProveInput {
            snapshot,
            batch,
            context,
            executed,
        }
    }

    #[derive(Clone)]
    struct ExtraTypeRuntime {
        descriptor: TypeDescriptor,
    }

    impl ExtraTypeRuntime {
        fn new() -> Self {
            let descriptor = TypeDescriptor::new(
                TEST_EXTRA_TYPE_ID,
                "test-extra-u64",
                Some("extra runtime used only to prove host overrides do not affect static relation roots".to_string()),
                HostValueFamily::UnsignedInt { bits: 64 },
                GenericIrFamily::UnsignedInteger,
                TypeCapabilities {
                    equality: true,
                    ordering: true,
                    arithmetic: true,
                },
                ZeroValueSpec::IntegerZero,
                NullSemantics::NullableWithCanonicalZero,
            )
            .expect("build extra type descriptor");
            Self { descriptor }
        }
    }

    impl TypeRuntime for ExtraTypeRuntime {
        fn type_id(&self) -> TypeId {
            self.descriptor.type_id
        }

        fn descriptor(&self) -> &TypeDescriptor {
            &self.descriptor
        }

        fn zero_typed(&self) -> TypedValue {
            TypedValue::new(self.type_id(), 0u64.to_le_bytes().to_vec())
        }

        fn encode_portable(&self, value: &TypedValue) -> Result<PortableValue, TabulaError> {
            Ok(value.clone().into_portable())
        }

        fn decode_portable(&self, value: &PortableValue) -> Result<TypedValue, TabulaError> {
            Ok(TypedValue::new(value.type_id(), value.payload().to_vec()))
        }

        fn validate(&self, value: &TypedValue) -> Result<(), TabulaError> {
            if value.type_id() != self.type_id() {
                return Err(TabulaError::Custom(
                    "unexpected type id for extra runtime".to_string(),
                ));
            }
            Ok(())
        }

        fn eq_value(&self, lhs: &TypedValue, rhs: &TypedValue) -> Result<bool, TabulaError> {
            self.validate(lhs)?;
            self.validate(rhs)?;
            Ok(lhs.payload() == rhs.payload())
        }

        fn cmp_value(&self, lhs: &TypedValue, rhs: &TypedValue) -> Result<Ordering, TabulaError> {
            self.validate(lhs)?;
            self.validate(rhs)?;
            Ok(lhs.payload().cmp(rhs.payload()))
        }

        fn apply_arithmetic(
            &self,
            _op: ArithmeticOp,
            _lhs: &TypedValue,
            _rhs: &TypedValue,
        ) -> Result<TypedValue, TabulaError> {
            Err(TabulaError::Custom(
                "extra runtime arithmetic is not used in this test".to_string(),
            ))
        }

        fn divmod(
            &self,
            _lhs: &TypedValue,
            _rhs: &TypedValue,
        ) -> Result<(TypedValue, TypedValue), TabulaError> {
            Err(TabulaError::Custom(
                "extra runtime divmod is not used in this test".to_string(),
            ))
        }

        fn debug_display(&self, value: &TypedValue) -> Result<String, TabulaError> {
            self.validate(value)?;
            Ok(format!("extra({:?})", value.payload()))
        }
    }

    #[derive(Clone)]
    struct ExtraEncodingRuntime {
        descriptor: EncodingProfile,
    }

    impl ExtraEncodingRuntime {
        fn new(type_descriptor: &TypeDescriptor) -> Self {
            let descriptor = EncodingProfile::new(
                TEST_EXTRA_ENCODING_ID,
                "test-extra-u64-encoding",
                Some("extra encoding used only to prove host overrides do not affect static relation roots".to_string()),
                type_descriptor,
                EncodingClass::FieldElementArray,
                FieldFamily::KoalaBear31,
                2,
                Some(8),
                CanonicalNullEncoding::SeparateNullFlagWithZeroValue,
                TranscriptSerialization::FieldElementsWithNullFlag,
                true,
                true,
            )
            .expect("build extra encoding profile");
            Self { descriptor }
        }
    }

    impl EncodingRuntime for ExtraEncodingRuntime {
        fn encoding_profile_id(&self) -> EncodingProfileId {
            self.descriptor.encoding_profile_id
        }

        fn descriptor(&self) -> &EncodingProfile {
            &self.descriptor
        }

        fn encode_field_elements(&self, value: &TypedValue) -> Result<Vec<KoalaBear>, TabulaError> {
            if value.type_id() != self.descriptor.type_id {
                return Err(TabulaError::Custom(
                    "unexpected type id for extra encoding runtime".to_string(),
                ));
            }
            Ok(vec![KoalaBear::ZERO, KoalaBear::ZERO])
        }

        fn decode_field_elements(
            &self,
            _field_elements: &[KoalaBear],
        ) -> Result<TypedValue, TabulaError> {
            Ok(TypedValue::new(
                self.descriptor.type_id,
                0u64.to_le_bytes().to_vec(),
            ))
        }

        fn encode_transcript_atoms(
            &self,
            value: &TypedValue,
        ) -> Result<Vec<KoalaBear>, TabulaError> {
            self.encode_field_elements(value)
        }

        fn trace_width(&self) -> usize {
            self.descriptor.width as usize
        }
    }

    #[test]
    fn relation_table_rows_reject_claims_missing_from_manifest() {
        let (registered, runtime, _prover) = runtime_for_source(relation_source());
        let error = prepare_relation_proof(
            runtime.execution_program().program(),
            registered.static_table_artifact(),
            &[RelationClaim {
                relation: ir::RelationId(0),
                kind: RelationClaimKind::Assert,
                inputs: vec![u64_typed(9)],
                input_digest: [9; 8],
                outputs: vec![],
                output_digest: [0; 8],
                tx_index: 0,
                effect_ordinal_in_tx: 0,
                op_index: 0,
            }],
        )
        .expect_err("manifest mismatch must fail");

        assert!(
            error
                .to_string()
                .contains("was not present in the sealed manifest"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn lowering_rejects_duplicate_relation_effect_origins() {
        let (_registered, runtime, _prover) = runtime_for_source(relation_source());
        let enroll = entry_id(&runtime, "enroll");
        let batch = tx_batch(vec![ir::EntryCall {
            entry_id: enroll,
            params: vec![bool_portable(true), u64_portable(0), u64_portable(2)],
        }]);
        let context = relation_context(7, 11);
        let snapshot = runtime.empty_state_snapshot();
        let executed = runtime
            .execute_batch(&snapshot, &batch, &context)
            .expect("execute batch");
        let tx = executed
            .successful_txs()
            .next()
            .expect("successful tx")
            .clone();

        let mut duplicated_effects = tx.relation_effects.clone();
        duplicated_effects.push(
            tx.relation_effects
                .first()
                .expect("relation effect")
                .clone(),
        );

        let typed_context = runtime
            .decode_context_input(&context)
            .expect("typed context");
        let typed_txs = runtime.decode_entry_batch(&batch).expect("typed batch");
        let entry = runtime
            .execution_program()
            .entry_definition(enroll)
            .expect("resolved entry");
        let context_slots = Vec::new();
        let param_slots = Vec::new();
        let event_item_bases = BTreeMap::new();

        let mut kit_scratch = tabula_stark::witness_kit::KitScratch::new();
        let error = lower_successful_tx::<EXECUTION_STANDARD_VALUE_WIDTH>(
            LowerSuccessfulTxInput {
                tx_index: tx.tx_index,
                program: runtime.execution_program().program(),
                call: &typed_txs[0],
                entry,
                context: &typed_context,
                state_effects: &tx.state_effects,
                event_effects: &tx.event_effects,
                property_effects: &tx.property_effects,
                relation_effects: &duplicated_effects,
                empty_columns: &BTreeSet::new(),
                type_runtimes: runtime.type_runtimes(),
                encoding_runtimes: runtime.encoding_runtimes(),
                tuple_encoding_defaults: &runtime.runtime_program.tuple_encoding_defaults,
                hasher: &PoseidonHasher::new(),
                state_runtime: &runtime.runtime_program.state,
                context_slots: &context_slots,
                param_slots: &param_slots,
                aux_slot_limit: tabula_chips::execution::MAX_SLOTS,
                event_item_bases: &event_item_bases,
            },
            &mut kit_scratch,
        )
        .expect_err("duplicate relation effects must fail");

        assert!(
            error.to_string().contains("duplicate relation effect"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn untaken_relation_branches_emit_no_relation_claims_or_positive_lookup_counts() {
        let (registered, runtime, prover) = runtime_for_source(guarded_relation_source());
        let batch = tx_batch(vec![ir::EntryCall {
            entry_id: entry_id(&runtime, "maybe_promote"),
            params: vec![bool_portable(false), u64_portable(0), u64_portable(2)],
        }]);
        let context = guarded_context(7);
        let snapshot = relation_snapshot(&registered);
        let executed = runtime
            .execute_batch(&snapshot, &batch, &context)
            .expect("execute guarded batch");

        let (machine_input, _public_statement) = prepare_proof_machine_input(
            &prover.runtime_program,
            &prover.root_backend_bundle,
            &prover.kit_registry,
            &prove_input(&snapshot, &batch, &context, &executed),
        )
        .expect("prepare proof request");

        let transcript_calls = machine_input
            .execution
            .store
            .get::<Vec<RelationTranscriptCall>>(RELATION_TRANSCRIPT_WITNESS_LABEL)
            .expect("relation transcript calls");
        let lookup_rows = machine_input
            .execution
            .store
            .get::<Vec<RelationTableWitnessRow>>(RELATION_TABLE_WITNESS_LABEL)
            .expect("relation lookup rows");

        assert!(transcript_calls.is_empty());
        assert!(
            lookup_rows.iter().all(|row| row.lookup_mult == 0),
            "untaken branches must not contribute positive relation lookup multiplicities",
        );
    }

    #[test]
    fn tampering_relation_table_rows_breaks_proving() {
        let (registered, runtime, prover) = runtime_for_source(relation_source());
        let batch = tx_batch(vec![ir::EntryCall {
            entry_id: entry_id(&runtime, "enroll"),
            params: vec![bool_portable(true), u64_portable(0), u64_portable(2)],
        }]);
        let context = relation_context(7, 11);
        let snapshot = relation_snapshot(&registered);
        let executed = runtime
            .execute_batch(&snapshot, &batch, &context)
            .expect("execute batch");

        let (mut machine_input, _public_statement) = prepare_proof_machine_input(
            &prover.runtime_program,
            &prover.root_backend_bundle,
            &prover.kit_registry,
            &prove_input(&snapshot, &batch, &context, &executed),
        )
        .expect("prepare proof request");

        let mut rows = machine_input
            .execution
            .store
            .get::<Vec<RelationTableWitnessRow>>(RELATION_TABLE_WITNESS_LABEL)
            .expect("relation lookup rows")
            .clone();
        assert!(!rows.is_empty(), "expected relation lookup rows");
        let tampered = rows
            .iter_mut()
            .find(|row| row.lookup_mult > 0)
            .expect("expected at least one consumed relation lookup row");
        tampered.output_digest[0] = tampered.output_digest[0].wrapping_add(1);
        machine_input
            .execution
            .store
            .put(RELATION_TABLE_WITNESS_LABEL, rows);

        assert!(
            BackendProver::new(&runtime.machine)
                .prove_envelope(machine_input)
                .is_err(),
            "tampered relation lookup rows must fail proving"
        );
    }

    #[test]
    fn tampering_execution_bound_relation_outputs_breaks_proving() {
        let (registered, runtime, prover) = runtime_for_source(relation_source());
        let batch = tx_batch(vec![ir::EntryCall {
            entry_id: entry_id(&runtime, "enroll"),
            params: vec![bool_portable(true), u64_portable(0), u64_portable(2)],
        }]);
        let context = relation_context(7, 11);
        let snapshot = relation_snapshot(&registered);
        let executed = runtime
            .execute_batch(&snapshot, &batch, &context)
            .expect("execute batch");

        let (mut machine_input, _public_statement) = prepare_proof_machine_input(
            &prover.runtime_program,
            &prover.root_backend_bundle,
            &prover.kit_registry,
            &prove_input(&snapshot, &batch, &context, &executed),
        )
        .expect("prepare proof request");

        let mut records = machine_input
            .execution
            .store
            .get::<Vec<InstructionRecord>>(witness_labels::EXECUTION_RECORDS)
            .expect("execution records")
            .clone();
        let eval_record = records
            .iter_mut()
            .find(|record| record.opcode == Opcode::RelationProof && record.relation_is_eval)
            .expect("relation eval execution record");
        eval_record.relation_output_vals[0][0] += KoalaBear::ONE;

        machine_input
            .execution
            .store
            .put(witness_labels::EXECUTION_RECORDS, records);

        assert!(
            BackendProver::new(&runtime.machine)
                .prove_envelope(machine_input)
                .is_err(),
            "tampered relation output binding must fail proving"
        );
    }

    #[test]
    fn tampering_relation_effect_identity_breaks_proving() {
        let (registered, runtime, prover) = runtime_for_source(relation_source());
        let batch = tx_batch(vec![ir::EntryCall {
            entry_id: entry_id(&runtime, "enroll"),
            params: vec![bool_portable(true), u64_portable(0), u64_portable(2)],
        }]);
        let context = relation_context(7, 11);
        let snapshot = relation_snapshot(&registered);
        let executed = runtime
            .execute_batch(&snapshot, &batch, &context)
            .expect("execute batch");

        let (mut machine_input, _public_statement) = prepare_proof_machine_input(
            &prover.runtime_program,
            &prover.root_backend_bundle,
            &prover.kit_registry,
            &prove_input(&snapshot, &batch, &context, &executed),
        )
        .expect("prepare proof request");

        let mut calls = machine_input
            .execution
            .store
            .get::<Vec<RelationTranscriptCall>>(RELATION_TRANSCRIPT_WITNESS_LABEL)
            .expect("relation transcript calls")
            .clone();
        assert!(
            calls.len() >= 4,
            "expected multiple relation transcript calls"
        );
        calls[2].effect_ordinal_in_tx = calls[0].effect_ordinal_in_tx;
        machine_input
            .execution
            .store
            .put(RELATION_TRANSCRIPT_WITNESS_LABEL, calls);

        assert!(
            BackendProver::new(&runtime.machine)
                .prove_envelope(machine_input)
                .is_err(),
            "tampered relation effect identity must fail proving"
        );
    }

    #[test]
    fn relation_table_rows_use_empty_output_digest_for_enum_relations() {
        let (registered, runtime, _prover) = runtime_for_source(relation_source());
        let empty_digest = compute_typed_tuple_digest(TypedTupleRole::RelationOutput, &[])
            .expect("empty tuple digest");
        let allowed_rows = registered
            .static_table_artifact()
            .rows
            .iter()
            .filter(|row| row.relation_id == 0)
            .collect::<Vec<_>>();
        assert_eq!(allowed_rows.len(), 4);
        assert!(
            allowed_rows
                .iter()
                .all(|row| row.output_digest == empty_digest)
        );

        let chosen = allowed_rows[2];
        let proof_rows = prepare_relation_proof(
            runtime.execution_program().program(),
            registered.static_table_artifact(),
            &[RelationClaim {
                relation: ir::RelationId(0),
                kind: RelationClaimKind::Assert,
                inputs: vec![u64_typed(2)],
                input_digest: chosen.input_digest,
                outputs: vec![],
                output_digest: chosen.output_digest,
                tx_index: 0,
                effect_ordinal_in_tx: 0,
                op_index: 0,
            }],
        )
        .expect("prepare relation proof rows");
        let rows = proof_rows
            .table_rows()
            .iter()
            .filter(|row| row.relation_id == 0)
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), 4);
        assert!(rows.iter().all(|row| row.output_digest == empty_digest));
        assert_eq!(rows.iter().map(|row| row.lookup_mult).sum::<u32>(), 1);
    }

    #[test]
    fn relation_proof_root_matches_registered_artifact_and_chip_public_values() {
        let (registered, runtime, prover) = runtime_for_source(relation_source());
        let verifier = PreparedVerifier::builder(std::sync::Arc::new(registered.sealed().clone()))
            .expect("create verifier builder")
            .build()
            .expect("build verifier");
        let batch = tx_batch(vec![ir::EntryCall {
            entry_id: entry_id(&runtime, "enroll"),
            params: vec![bool_portable(true), u64_portable(0), u64_portable(2)],
        }]);
        let context = relation_context(7, 11);
        let snapshot = relation_snapshot(&registered);
        let executed = runtime
            .execute_batch(&snapshot, &batch, &context)
            .expect("execute batch");
        let proved = prover
            .prove_and_verify(
                &verifier,
                &ProveInput {
                    snapshot: &snapshot,
                    batch: &batch,
                    context: &context,
                    executed: &executed,
                },
            )
            .expect("prove relation batch");
        let chip_root = relation_table_root_from_proof(&proved.proof, prover.machine())
            .expect("extract relation chip root");

        assert_eq!(
            prover.runtime_program.static_table_artifact.root,
            registered.static_table_artifact().root
        );
        assert_eq!(
            chip_root,
            Some(registered.static_table_artifact().root),
            "relation table chip root must match the registered artifact root",
        );
        assert_eq!(
            runtime_ir::compute_applied_tx_digest(
                &batch,
                prover.type_runtimes(),
                prover.encoding_runtimes(),
                &prover.runtime_program.tuple_encoding_defaults,
            )
            .expect("batch digest"),
            proved.public_statement.applied_tx_digest.to_bytes()
        );
        assert_eq!(
            executed.successful_txs().count(),
            1,
            "sanity-check proof came from the expected execution batch",
        );
    }

    #[test]
    fn relation_chip_public_values_truncation_fails_verification() {
        let (registered, runtime, prover) = runtime_for_source(relation_source());
        let snapshot = relation_snapshot(&registered);
        let verifier = PreparedVerifier::builder(std::sync::Arc::new(registered.sealed().clone()))
            .expect("create verifier builder")
            .build()
            .expect("build verifier");
        let batch = tx_batch(vec![ir::EntryCall {
            entry_id: entry_id(&runtime, "enroll"),
            params: vec![bool_portable(true), u64_portable(0), u64_portable(2)],
        }]);
        let context = relation_context(7, 11);
        let executed = runtime
            .execute_batch(&snapshot, &batch, &context)
            .expect("execute batch");
        let mut proved = prover
            .prove(&ProveInput {
                snapshot: &snapshot,
                batch: &batch,
                context: &context,
                executed: &executed,
            })
            .expect("prove relation batch");
        let relation_opening = proved
            .proof
            .execution
            .chip_openings
            .iter_mut()
            .find(|opening| opening.chip_id == RELATION_TABLE_CHIP_ID)
            .expect("relation chip opening");
        relation_opening.public_values.pop();

        let verifier_err = verifier
            .verify(&proved.proof, &proved.public_statement)
            .expect_err("truncated relation chip public values must fail verifier validation");
        assert!(
            verifier_err
                .to_string()
                .contains("machine metadata requires 8"),
            "unexpected verifier error: {verifier_err}"
        );
    }

    #[test]
    fn relation_chip_public_values_append_fails_verification() {
        let (registered, runtime, prover) = runtime_for_source(relation_source());
        let snapshot = relation_snapshot(&registered);
        let verifier = PreparedVerifier::builder(std::sync::Arc::new(registered.sealed().clone()))
            .expect("create verifier builder")
            .build()
            .expect("build verifier");
        let batch = tx_batch(vec![ir::EntryCall {
            entry_id: entry_id(&runtime, "enroll"),
            params: vec![bool_portable(true), u64_portable(0), u64_portable(2)],
        }]);
        let context = relation_context(7, 11);
        let executed = runtime
            .execute_batch(&snapshot, &batch, &context)
            .expect("execute batch");
        let mut proved = prover
            .prove(&ProveInput {
                snapshot: &snapshot,
                batch: &batch,
                context: &context,
                executed: &executed,
            })
            .expect("prove relation batch");
        let relation_opening = proved
            .proof
            .execution
            .chip_openings
            .iter_mut()
            .find(|opening| opening.chip_id == RELATION_TABLE_CHIP_ID)
            .expect("relation chip opening");
        relation_opening.public_values.push(KoalaBear::ZERO);

        let verifier_err = verifier
            .verify(&proved.proof, &proved.public_statement)
            .expect_err("extended relation chip public values must fail verifier validation");
        assert!(
            verifier_err
                .to_string()
                .contains("machine metadata requires 8"),
            "unexpected verifier error: {verifier_err}"
        );
    }

    #[test]
    fn missing_relation_chip_opening_still_fails_verification() {
        let (registered, runtime, prover) = runtime_for_source(relation_source());
        let snapshot = relation_snapshot(&registered);
        let verifier = PreparedVerifier::builder(std::sync::Arc::new(registered.sealed().clone()))
            .expect("create verifier builder")
            .build()
            .expect("build verifier");
        let batch = tx_batch(vec![ir::EntryCall {
            entry_id: entry_id(&runtime, "enroll"),
            params: vec![bool_portable(true), u64_portable(0), u64_portable(2)],
        }]);
        let context = relation_context(7, 11);
        let executed = runtime
            .execute_batch(&snapshot, &batch, &context)
            .expect("execute batch");
        let mut proved = prover
            .prove(&ProveInput {
                snapshot: &snapshot,
                batch: &batch,
                context: &context,
                executed: &executed,
            })
            .expect("prove relation batch");
        proved
            .proof
            .execution
            .chip_openings
            .retain(|opening| opening.chip_id != RELATION_TABLE_CHIP_ID);

        let verifier_err = verifier
            .verify(&proved.proof, &proved.public_statement)
            .expect_err("missing relation chip opening must fail verifier validation");
        assert!(
            verifier_err
                .to_string()
                .contains("relation table chip opening is missing"),
            "unexpected verifier error: {verifier_err}"
        );
    }

    #[test]
    fn bundled_root_authority_rejects_unsupported_binding_families() {
        let registered = register_program_from_source(relation_source());
        let err = TabulaRuntime::builder(registered.clone())
            .expect("create runtime builder")
            .with_root_backend_bundle(RootBackendBundle::new(EmptyFamilyRootBackend))
            .build()
            .err()
            .expect("runtime build must reject unsupported bundled root families");
        assert!(
            err.to_string()
                .contains("bundled root authority does not support binding family"),
            "unexpected runtime build error: {err}"
        );

        let err = PreparedVerifier::builder(std::sync::Arc::new(registered.sealed().clone()))
            .expect("create verifier builder")
            .with_root_backend_bundle(RootBackendBundle::new(EmptyFamilyRootBackend))
            .build()
            .err()
            .expect("verifier build must reject unsupported bundled root families");
        assert!(
            err.to_string()
                .contains("bundled root authority does not support binding family"),
            "unexpected verifier build error: {err}"
        );
    }

    #[test]
    fn event_transcript_witness_matches_execution_event_rows() {
        let registered = register_program_from_source(event_debug_source());
        let runtime = TabulaRuntime::builder(registered.clone())
            .expect("create runtime builder")
            .build()
            .expect("build runtime");
        let prover = crate::PreparedProver::builder(registered)
            .expect("create prover builder")
            .build()
            .expect("build prover");
        let snapshot = runtime.empty_state_snapshot();
        let register = runtime
            .execution_program()
            .program()
            .entries
            .iter()
            .find(|entry| entry.symbol == "register")
            .map(|entry| entry.id)
            .expect("register entry");
        let batch = tx_batch(vec![ir::EntryCall {
            entry_id: register,
            params: vec![u64_portable(1)],
        }]);
        let context = context_input([(ir::ContextFieldId(0), u64_portable(7))]);
        let executed = runtime
            .execute_batch(&snapshot, &batch, &context)
            .expect("execute event batch");
        let typed_context = runtime
            .decode_context_input(&context)
            .expect("decode context");
        let typed_txs = runtime.decode_entry_batch(&batch).expect("decode batch");

        let prepared = prepare_proof_artifacts(
            &prover.runtime_program,
            &prover.root_backend_bundle,
            &prover.kit_registry,
            &snapshot,
            &typed_txs,
            &typed_context,
            &executed,
        )
        .expect("prepare proof artifacts");

        let records = prepared
            .execution
            .store
            .get::<Vec<InstructionRecord>>(witness_labels::EXECUTION_RECORDS)
            .expect("execution records");
        let transcript_items = prepared
            .execution
            .store
            .get::<Vec<[KoalaBear; 8]>>(EVENT_TRANSCRIPT_WITNESS_LABEL)
            .expect("event transcript items");

        let execution_items = extract_event_items(records);
        let witness_items = transcript_items
            .iter()
            .copied()
            .enumerate()
            .map(|(index, block)| (index as u32, block))
            .collect::<Vec<_>>();

        assert_eq!(execution_items, witness_items);
    }

    #[test]
    fn native_runtime_rejects_capability_calls_with_explicit_subset_error() {
        let catalogs = tabula_compiler::CompilerCatalogs::standard()
            .expect("standard catalogs")
            .with_capability_descriptor(tabula_compiler::SourceCapabilityDescriptor {
                path: "demo_hash".into(),
                inputs: vec![tabula_profile::TYPE_U64_ID],
                outputs: vec![tabula_profile::TYPE_BYTES32_ID],
                totality: ir::CapabilityTotality::Total,
                query_policy: ir::CapabilityQueryPolicy::QuerySafe,
                proof_visibility: ir::CapabilityProofVisibility::OpaqueRuntimeOnly,
                hash_family: None,
            })
            .expect("demo hash capability descriptor");
        let registered = register_program_from_source_with_catalogs(capability_source(), &catalogs);

        // Engine path (TabulaRuntime / prepare_executor) runs validate_core_first_program
        // which rejects capability calls. The verifier path is IR-free and does not run
        // this check — the binding-digest gate serves as the primary gating mechanism there.
        let err = TabulaRuntime::builder(registered.clone())
            .expect("create runtime builder")
            .build()
            .err()
            .expect("capability-backed program must be rejected before native proving");
        let rendered = err.to_string();
        assert!(
            rendered.contains("outside the current native proving subset"),
            "unexpected runtime build error: {rendered}"
        );
        assert!(
            rendered.contains("CallCapability"),
            "unexpected runtime build error: {rendered}"
        );
    }

    #[test]
    fn host_runtime_overrides_do_not_change_compiler_sealed_static_table_root() {
        let registered = register_program_from_source(relation_source());
        let extra_type = ExtraTypeRuntime::new();
        let extra_encoding = ExtraEncodingRuntime::new(extra_type.descriptor());
        let host_environment = HostEnvironment::standard()
            .expect("standard host environment")
            .with_runtime_registries(
                crate::host::RuntimeRegistries::standard()
                    .expect("standard runtime registries")
                    .with_type_runtime(extra_type.clone())
                    .expect("register extra type runtime")
                    .with_encoding_runtime(extra_encoding)
                    .expect("register extra encoding runtime"),
            );

        let runtime = TabulaRuntime::builder(registered.clone())
            .expect("create runtime builder")
            .with_host_environment(host_environment.clone())
            .build()
            .expect("build runtime with extra host runtimes");
        let prover = crate::PreparedProver::builder(registered.clone())
            .expect("create prover builder")
            .with_host_environment(host_environment.clone())
            .build()
            .expect("build prover with extra host runtimes");
        let verifier = PreparedVerifier::builder(std::sync::Arc::new(registered.sealed().clone()))
            .expect("create verifier builder")
            .with_host_environment(host_environment)
            .build()
            .expect("build verifier with extra host runtimes");

        let batch = tx_batch(vec![ir::EntryCall {
            entry_id: entry_id(&runtime, "enroll"),
            params: vec![bool_portable(true), u64_portable(0), u64_portable(2)],
        }]);
        let context = relation_context(7, 11);
        let snapshot = relation_snapshot(&registered);
        let executed = runtime
            .execute_batch(&snapshot, &batch, &context)
            .expect("execute relation batch under custom host environment");
        let proved = prover
            .prove(&ProveInput {
                snapshot: &snapshot,
                batch: &batch,
                context: &context,
                executed: &executed,
            })
            .expect("prove relation batch under custom host environment");

        assert_eq!(
            prover.runtime_program.static_table_artifact.root,
            registered.static_table_artifact().root
        );
        verifier
            .verify(&proved.proof, &proved.public_statement)
            .expect("verify proof under custom host environment");
    }
}
