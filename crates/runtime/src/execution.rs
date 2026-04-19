//! Batch execution and query dispatch over prepared runtime state.
//!
//! Hosts the free-function execute/query surface and [`ExecutionReceipt`].
//! [`crate::PreparedExecutor`] methods are thin wrappers that forward into
//! these functions; [`crate::PreparedProver`] also calls into this module
//! directly.

#![cfg(feature = "verify")]

use tabula_commitment::PoseidonHasher;
use tabula_core::PortableValue;
use tabula_executor as exec;
use tabula_ir as ir;
use tabula_types::{ContextValues, TxCall, TypedValue};

use crate::error::{ExecuteError, RuntimeError};
use crate::prelude;
use crate::prepared_state::PreparedRuntimeState;
use crate::snapshot::{CommittedStateSnapshot, LogicalStateCell};
use crate::statement;

/// Runtime-owned execution result including exact inputs and post-state.
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

/// Materialize one logical keyed state input into a committed snapshot.
pub(crate) fn materialize_logical_state<I>(
    state: &PreparedRuntimeState,
    cells: I,
) -> Result<CommittedStateSnapshot, RuntimeError>
where
    I: IntoIterator<Item = (ir::TableId, Vec<PortableValue>, ir::FieldId, PortableValue)>,
{
    CommittedStateSnapshot::from_cells(&state.state, &state.type_runtimes, cells)
}

/// Decode and validate one committed snapshot payload against the sealed state contract.
///
/// Accepts cells with already-encoded (committed) keys; validates the payload
/// and returns a ready-to-use [`CommittedStateSnapshot`].
pub(crate) fn decode_committed_snapshot<I>(
    state: &PreparedRuntimeState,
    cells: I,
) -> Result<CommittedStateSnapshot, RuntimeError>
where
    I: IntoIterator<Item = (ir::TableId, Vec<u8>, ir::FieldId, PortableValue)>,
{
    CommittedStateSnapshot::from_committed_cells(&state.state, &state.type_runtimes, cells)
}

/// Project one committed snapshot back into logical keyed cells.
pub(crate) fn project_logical_state(
    state: &PreparedRuntimeState,
    snapshot: &CommittedStateSnapshot,
) -> Result<Vec<LogicalStateCell>, RuntimeError> {
    snapshot.validate(&state.state, &state.type_runtimes)?;
    snapshot
        .cells()
        .map(|(key, value)| {
            let logical_key = state
                .state
                .key_codec(key.table)?
                .decode_key(&key.key)
                .map_err(|error| ExecuteError::Validation {
                    detail: error.to_string(),
                })?
                .into_iter()
                .map(|value| {
                    state.type_runtimes.encode_typed(&value).map_err(|source| {
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
pub(crate) fn execute_batch(
    state: &PreparedRuntimeState,
    snapshot: &CommittedStateSnapshot,
    batch: &ir::EntryBatch,
    context: &ir::ContextInput,
) -> Result<exec::ExecutionJournal, RuntimeError> {
    let txs = prelude::decode_entry_batch_on_state(state, batch)?;
    let context = prelude::decode_context_input_on_state(state, context)?;
    execute_batch_typed(state, snapshot, &txs, &context)
}

/// Execute a canonical tx batch and return a runtime-owned receipt.
pub(crate) fn execute_batch_receipt(
    state: &PreparedRuntimeState,
    snapshot: &CommittedStateSnapshot,
    batch: &ir::EntryBatch,
    context: &ir::ContextInput,
) -> Result<ExecutionReceipt, RuntimeError> {
    let journal = execute_batch(state, snapshot, batch, context)?;
    let state_after = statement::materialize_post_state(snapshot, &journal, &state.type_runtimes)?;
    Ok(ExecutionReceipt {
        snapshot: snapshot.clone(),
        batch: batch.clone(),
        context: context.clone(),
        state_after,
        journal,
    })
}

fn execute_batch_typed(
    state: &PreparedRuntimeState,
    snapshot: &CommittedStateSnapshot,
    txs: &[TxCall],
    context: &ContextValues,
) -> Result<exec::ExecutionJournal, RuntimeError> {
    snapshot.validate(&state.state, &state.type_runtimes)?;
    exec::execute_batch(
        state.semantic.execution(),
        txs,
        context,
        snapshot,
        &exec::ExecContext {
            hasher: &PoseidonHasher::new(),
            type_runtimes: &state.type_runtimes,
            capability_executor: None,
            state_runtime: &state.state,
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
pub(crate) fn execute_query(
    state: &PreparedRuntimeState,
    snapshot: &CommittedStateSnapshot,
    entry_id: ir::EntryId,
    params: &[PortableValue],
    context: &ir::ContextInput,
) -> Result<exec::QueryExecutionResult, RuntimeError> {
    let params = decode_query_params(state, entry_id, params)?;
    let context = prelude::decode_context_input_on_state(state, context)?;
    execute_query_typed(state, snapshot, entry_id, &params, &context)
}

fn execute_query_typed(
    state: &PreparedRuntimeState,
    snapshot: &CommittedStateSnapshot,
    entry_id: ir::EntryId,
    params: &[TypedValue],
    context: &ContextValues,
) -> Result<exec::QueryExecutionResult, RuntimeError> {
    snapshot.validate(&state.state, &state.type_runtimes)?;
    exec::execute_query(
        state.semantic.execution(),
        entry_id,
        params,
        context,
        snapshot,
        &exec::ExecContext {
            hasher: &PoseidonHasher::new(),
            type_runtimes: &state.type_runtimes,
            capability_executor: None,
            state_runtime: &state.state,
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

fn decode_query_params(
    state: &PreparedRuntimeState,
    entry_id: ir::EntryId,
    params: &[PortableValue],
) -> Result<Vec<TypedValue>, RuntimeError> {
    let entry = state
        .semantic
        .execution()
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
    prelude::decode_params_on_state(state, &entry.params, params)
}
