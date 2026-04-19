//! Public-statement materialization helpers and post-state binding-digest wiring.

#[cfg(feature = "prove")]
use tabula_contract::PublicStatement;
use tabula_executor as exec;
use tabula_ir as ir;
use tabula_types::TypeRuntimeRegistry;

#[cfg(feature = "prove")]
use crate::prepared_state::PreparedRuntimeState;
use crate::error::{ExecuteError, RuntimeError};
#[cfg(feature = "prove")]
use crate::error::VerifyError;
use crate::semantics as runtime_ir;
use crate::snapshot::CommittedStateSnapshot;
#[cfg(feature = "prove")]
use tabula_types::ContextValues;

#[cfg(feature = "prove")]
pub(crate) fn materialize_public_statement_on_state(
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

pub(crate) fn materialize_post_state(
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
