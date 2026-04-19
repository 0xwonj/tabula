//! Prepared-once execute handle parallel to [`crate::PreparedProver`] and
//! [`crate::PreparedVerifier`].
//!
//! [`PreparedExecutor`] is the executor analogue: one handle per registered
//! native program, `&self` per-call surface, stateless w.r.t. snapshots,
//! `Send + Sync + 'static`. The free function [`prepare_executor`] is the
//! canonical construction path; `&PreparedOptions` carries the
//! host-environment / machine-config / root-backend knobs.
//!
//! Unlike the prover, the executor does not own a machine or root-backend
//! bundle: executing a batch or query is zero-crypto. The prepared state
//! (semantic program, resolved state runtime, type / encoding registries)
//! is shared — the handle holds it behind `Arc` so multiple executors over
//! the same program can share the expensive setup.

use std::sync::Arc;

use tabula_compiler::RegisteredProgram;
use tabula_core::PortableValue;
use tabula_executor as exec;
use tabula_ir as ir;
use tabula_types::{EncodingRuntimeRegistry, TypeRuntimeRegistry};

use crate::error::{ExecuteError, RuntimeError, SetupError};
use crate::execution::{
    self, ExecutionReceipt,
};
use crate::host::HostEnvironment;
use crate::options::PreparedOptions;
use crate::prepared_state::{PreparedRuntimeState, build_prepared_runtime};
use crate::snapshot::{CommittedStateSnapshot, LogicalStateCell};

/// Prepared executor handle for one registered native program.
///
/// Owns the prepared-once runtime state behind [`Arc`]. Per-call operations
/// (`execute_batch`, `execute_query`, `materialize_logical_state`,
/// `project_logical_state`) borrow the handle shared and build all mutable
/// state in locals so two calls with equal inputs produce equal outputs.
///
/// Construction goes through [`prepare_executor`]; unlike
/// [`crate::PreparedProver`] there is no machine or root-backend bundle
/// because execution is zero-crypto.
#[non_exhaustive]
pub struct PreparedExecutor {
    pub(crate) state: Arc<PreparedRuntimeState>,
}

impl PreparedExecutor {
    /// Borrow the installed type runtimes.
    pub fn type_runtimes(&self) -> &TypeRuntimeRegistry {
        &self.state.type_runtimes
    }

    /// Look up an entry id by source symbol.
    ///
    /// Returns `None` if no entry in this program has a matching symbol.
    /// Entry ids are stable per compiled program — two executors over the
    /// same registered program always resolve a given symbol to the same id.
    pub fn entry_id_by_symbol(&self, symbol: &str) -> Option<ir::EntryId> {
        self.state
            .semantic
            .execution()
            .program()
            .entries
            .iter()
            .find(|entry| entry.symbol == symbol)
            .map(|entry| entry.id)
    }

    /// Borrow the installed encoding runtimes.
    pub fn encoding_runtimes(&self) -> &EncodingRuntimeRegistry {
        &self.state.encoding_runtimes
    }

    /// Create an empty committed state snapshot for this program.
    pub fn empty_state_snapshot(&self) -> CommittedStateSnapshot {
        CommittedStateSnapshot::empty()
    }

    /// Decode and validate one committed snapshot payload against this program's sealed state
    /// contract.
    ///
    /// Accepts cells with already-encoded (committed) keys; validates the payload and returns a
    /// ready-to-use [`CommittedStateSnapshot`]. Prefer [`Self::materialize_logical_state`] when
    /// building snapshots from logical (decoded) keys.
    pub fn decode_committed_snapshot<I>(
        &self,
        cells: I,
    ) -> Result<CommittedStateSnapshot, ExecuteError>
    where
        I: IntoIterator<Item = (ir::TableId, Vec<u8>, ir::FieldId, PortableValue)>,
    {
        execution::decode_committed_snapshot(&self.state, cells).map_err(route_to_execute)
    }

    /// Materialize one logical keyed state input into a committed snapshot.
    pub fn materialize_logical_state<I>(
        &self,
        cells: I,
    ) -> Result<CommittedStateSnapshot, ExecuteError>
    where
        I: IntoIterator<Item = (ir::TableId, Vec<PortableValue>, ir::FieldId, PortableValue)>,
    {
        execution::materialize_logical_state(&self.state, cells).map_err(route_to_execute)
    }

    /// Project one committed snapshot back into logical keyed cells.
    pub fn project_logical_state(
        &self,
        snapshot: &CommittedStateSnapshot,
    ) -> Result<Vec<LogicalStateCell>, ExecuteError> {
        execution::project_logical_state(&self.state, snapshot).map_err(route_to_execute)
    }

    /// Execute a canonical tx batch.
    pub fn execute_batch(
        &self,
        snapshot: &CommittedStateSnapshot,
        batch: &ir::EntryBatch,
        context: &ir::ContextInput,
    ) -> Result<exec::ExecutionJournal, ExecuteError> {
        execution::execute_batch(&self.state, snapshot, batch, context).map_err(route_to_execute)
    }

    /// Execute a canonical tx batch and return a runtime-owned receipt.
    pub fn execute_batch_receipt(
        &self,
        snapshot: &CommittedStateSnapshot,
        batch: &ir::EntryBatch,
        context: &ir::ContextInput,
    ) -> Result<ExecutionReceipt, ExecuteError> {
        execution::execute_batch_receipt(&self.state, snapshot, batch, context)
            .map_err(route_to_execute)
    }

    /// Execute one query entry. Query proving remains intentionally absent.
    pub fn execute_query(
        &self,
        snapshot: &CommittedStateSnapshot,
        entry_id: ir::EntryId,
        params: &[PortableValue],
        context: &ir::ContextInput,
    ) -> Result<exec::QueryExecutionResult, ExecuteError> {
        execution::execute_query(&self.state, snapshot, entry_id, params, context)
            .map_err(route_to_execute)
    }
}

/// Build a [`PreparedExecutor`] from a shared registered program and an
/// option bundle.
///
/// Runs `validate_sealed_artifact` on the registered program, then
/// delegates to `build_prepared_runtime`, which in turn runs
/// `validate_core_first_program` to reject capability-backed programs
/// (see §5.3 of the SP-5 decomposition design).
pub fn prepare_executor(
    registered: Arc<RegisteredProgram>,
    opts: &PreparedOptions,
) -> Result<PreparedExecutor, ExecuteError> {
    let program = Arc::try_unwrap(registered).unwrap_or_else(|shared| (*shared).clone());
    program
        .validate_sealed_artifact()
        .map_err(|e| ExecuteError::Validation {
            detail: e.to_string(),
        })?;
    let host_environment: HostEnvironment = opts.host_environment().clone();
    let machine_stark_config = opts.machine_stark_config().clone();
    #[cfg(feature = "prove")]
    let root_backend = opts.root_backend().0.clone();
    #[cfg(not(feature = "prove"))]
    let root_backend = std::sync::Arc::clone(&opts.root_backend().0);

    let build = build_prepared_runtime(
        &program,
        &host_environment,
        &machine_stark_config,
        #[cfg(feature = "prove")]
        root_backend,
        #[cfg(not(feature = "prove"))]
        root_backend,
    )
    .map_err(|e| ExecuteError::Validation {
        detail: e.to_string(),
    })?;
    // The executor does not need the machine or the root-backend bundle:
    // execution is zero-crypto. Drop them so the handle's footprint is
    // purely the prepared state.
    Ok(PreparedExecutor {
        state: Arc::new(build.runtime_program),
    })
}

/// Narrow a [`RuntimeError`] to [`ExecuteError`] for the executor surface.
///
/// The underlying execute/snapshot helpers in `execution.rs` and
/// `snapshot.rs` currently widen to `RuntimeError` through
/// `ExecuteError` (execution failures) and `SetupError::Validation`
/// (snapshot construction failures). Both map to `ExecuteError` on the
/// executor surface: an executor call that fails snapshot validation is
/// an execute-side validation error from the caller's point of view.
fn route_to_execute(error: RuntimeError) -> ExecuteError {
    match error {
        RuntimeError::Execute(inner) => inner,
        RuntimeError::Setup(SetupError::Validation { detail }) => {
            ExecuteError::Validation { detail }
        }
        other => unreachable!(
            "execution helpers only produce Execute or Setup::Validation, got: {other:?}"
        ),
    }
}

// Load-bearing Send+Sync+'static: PreparedExecutor must be cheap to share.
const _: fn() = || {
    fn assert_send_sync_static<T: Send + Sync + 'static>() {}
    assert_send_sync_static::<PreparedExecutor>();
};
