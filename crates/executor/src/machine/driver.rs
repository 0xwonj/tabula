//! Entry-point functions for executing queries and transaction batches.

use tabula_core::error::TabulaError;
use tabula_core::traits::StateView;
use tabula_ir as ir;
use tabula_types::{ContextValues, TxCall, TypeRuntimeRegistry, TypedValue};

use crate::machine::entry::EntryMachineCore;
use crate::program::{ResolvedEntry, ResolvedExecutionProgram};
use crate::state::{Overlay, OverlayResult};
use crate::surface::{
    ExecContext, ExecuteError, ExecutionJournal, ExecutionStateSummary, FailedTxExecution,
    QueryExecutionResult, SuccessfulTxExecution, TxExecutionOutcome, TypedStateSnapshot,
    TypedStateWrite,
};

/// Execute a single query entry and return its result with all observed effects.
pub fn execute_query<S: StateView>(
    program: &ResolvedExecutionProgram,
    entry_id: ir::EntryId,
    params: &[TypedValue],
    context: &ContextValues,
    snapshot: &S,
    exec: &ExecContext<'_>,
) -> Result<QueryExecutionResult, ExecuteError> {
    validate_context(program, context, exec.type_runtimes)
        .map_err(|error| ExecuteError { error, op_index: 0 })?;
    let entry = program
        .entry(entry_id)
        .map_err(|error| ExecuteError { error, op_index: 0 })?;
    if entry.definition.kind != ir::EntryKind::Query {
        return Err(ExecuteError {
            error: TabulaError::InvalidIr(format!(
                "entry {} is not a query",
                entry.definition.symbol
            )),
            op_index: 0,
        });
    }
    validate_params(entry, params, exec.type_runtimes)
        .map_err(|error| ExecuteError { error, op_index: 0 })?;

    let mut overlay = Overlay::new(snapshot, exec.type_runtimes);
    let machine = EntryMachineCore::new(program, entry, params, context, &mut overlay, exec, 0);
    let result = machine.execute().map_err(|trap| ExecuteError {
        error: trap.error,
        op_index: trap.op_index,
    })?;
    let overlay = overlay
        .into_result()
        .map_err(|error| ExecuteError { error, op_index: 0 })?;

    Ok(QueryExecutionResult {
        returns: result.returns,
        state_summary: map_overlay_summary(overlay),
        state_effects: result.state_effects,
        property_effects: result.property_effects,
        relation_effects: result.relation_effects,
        capability_effects: result.capability_effects,
        event_effects: result.event_effects,
    })
}

/// Execute an ordered batch of transactions and return the execution journal.
pub fn execute_batch<S: StateView>(
    program: &ResolvedExecutionProgram,
    txs: &[TxCall],
    context: &ContextValues,
    snapshot: &S,
    exec: &ExecContext<'_>,
) -> Result<ExecutionJournal, TabulaError> {
    validate_context(program, context, exec.type_runtimes)?;

    let mut overlay = Overlay::new(snapshot, exec.type_runtimes);
    let mut outcomes = Vec::with_capacity(txs.len());
    let mut next_logical_time = 0u64;

    for (tx_index, tx) in txs.iter().enumerate() {
        let tx_index = tx_index as u32;
        let entry = match program.entry(tx.entry_id) {
            Ok(entry) => entry,
            Err(error) => {
                outcomes.push(TxExecutionOutcome::Failed(FailedTxExecution {
                    tx_index,
                    entry_id: tx.entry_id,
                    reason: error.to_string(),
                    failed_op_index: None,
                }));
                continue;
            }
        };
        if entry.definition.kind != ir::EntryKind::Tx {
            outcomes.push(TxExecutionOutcome::Failed(FailedTxExecution {
                tx_index,
                entry_id: tx.entry_id,
                reason: format!("entry {} is not a tx", entry.definition.symbol),
                failed_op_index: None,
            }));
            continue;
        }
        if let Err(error) = validate_params(entry, &tx.params, exec.type_runtimes) {
            outcomes.push(TxExecutionOutcome::Failed(FailedTxExecution {
                tx_index,
                entry_id: tx.entry_id,
                reason: error.to_string(),
                failed_op_index: None,
            }));
            continue;
        }

        overlay.checkpoint();
        let machine = EntryMachineCore::new(
            program,
            entry,
            &tx.params,
            context,
            &mut overlay,
            exec,
            next_logical_time,
        );
        match machine.execute() {
            Ok(result) => {
                overlay.discard_checkpoint();
                next_logical_time = result.next_logical_time;
                outcomes.push(TxExecutionOutcome::Success(SuccessfulTxExecution {
                    tx_index,
                    entry_id: tx.entry_id,
                    state_effects: result.state_effects,
                    property_effects: result.property_effects,
                    relation_effects: result.relation_effects,
                    capability_effects: result.capability_effects,
                    event_effects: result.event_effects,
                }));
            }
            Err(trap) => {
                overlay.rollback();
                match trap.kind {
                    crate::machine::entry::TrapKind::Semantic => {
                        outcomes.push(TxExecutionOutcome::Failed(FailedTxExecution {
                            tx_index,
                            entry_id: tx.entry_id,
                            reason: trap.error.to_string(),
                            failed_op_index: Some(trap.op_index),
                        }));
                    }
                    crate::machine::entry::TrapKind::Fatal => return Err(trap.error),
                }
            }
        }
    }

    let overlay = overlay.into_result()?;
    Ok(ExecutionJournal {
        state_summary: map_overlay_summary(overlay),
        txs: outcomes,
    })
}

pub(crate) fn validate_context(
    program: &ResolvedExecutionProgram,
    context: &ContextValues,
    type_runtimes: &TypeRuntimeRegistry,
) -> Result<(), TabulaError> {
    if context.fields.len() != program.program().context.fields.len() {
        return Err(TabulaError::ParamSchemaMismatch(format!(
            "program expects {} context values but got {}",
            program.program().context.fields.len(),
            context.fields.len()
        )));
    }
    for field in &program.program().context.fields {
        let value = context.fields.get(&field.id).ok_or_else(|| {
            TabulaError::ParamSchemaMismatch(format!("missing context field {}", field.symbol))
        })?;
        if field.ty != value.type_id() {
            return Err(TabulaError::ParamSchemaMismatch(format!(
                "context field {} expects type {} but got {}",
                field.symbol,
                field.ty.0,
                value.type_id().0
            )));
        }
        type_runtimes.resolve(value.type_id())?.validate(value)?;
    }
    Ok(())
}

pub(crate) fn validate_params(
    entry: &ResolvedEntry,
    params: &[TypedValue],
    type_runtimes: &TypeRuntimeRegistry,
) -> Result<(), TabulaError> {
    if entry.definition.params.len() != params.len() {
        return Err(TabulaError::ParamSchemaMismatch(format!(
            "entry {} expects {} params but got {}",
            entry.definition.symbol,
            entry.definition.params.len(),
            params.len()
        )));
    }
    for (param, value) in entry.definition.params.iter().zip(params) {
        if param.ty != value.type_id() {
            return Err(TabulaError::ParamSchemaMismatch(format!(
                "param {} expects type {} but got {}",
                param.symbol,
                param.ty.0,
                value.type_id().0
            )));
        }
        type_runtimes.resolve(value.type_id())?.validate(value)?;
    }
    Ok(())
}

pub(crate) fn map_overlay_summary(overlay: OverlayResult) -> ExecutionStateSummary {
    ExecutionStateSummary {
        read_set_old: overlay
            .read_set_old
            .into_iter()
            .map(|entry| TypedStateSnapshot {
                key: entry.key,
                type_id: entry.type_id,
                value: entry.value,
            })
            .collect(),
        write_set_final: overlay
            .write_set_final
            .into_iter()
            .map(|entry| TypedStateWrite {
                key: entry.key,
                type_id: entry.type_id,
                value: entry.value,
            })
            .collect(),
    }
}
