//! Deterministic execution engine for the canonical `tabula_ir` program model.
//!
//! The root re-exports the stable execution nouns directly.

mod host;
mod machine;
mod program;
mod state;
mod surface;

pub use host::{CapabilityExecutor, CapabilityHandler, CapabilityRegistry, StateRuntimeView};
pub use machine::{execute_batch, execute_query};
pub use program::{ResolvedExecutionProgram, ResolvedTable};
pub use state::{Overlay, OverlayResult};
pub use surface::{
    CapabilityEffect, ContextValues, ExecContext, ExecuteError, ExecutionJournal,
    ExecutionStateSummary, FailedTxExecution, QueryExecutionResult, RelationEffect,
    RelationEffectKind, StateEffectKind, StatePropertyEffect, SuccessfulTxExecution, TxCall,
    TxExecutionOutcome, TypedEventEffect, TypedStateEffect, TypedStateSnapshot, TypedStateWrite,
};
