//! Deterministic execution engine for the canonical `tabula_ir` program model.
//!
//! The root re-exports the stable execution nouns directly.

mod host;
mod machine;
mod program;
mod state;
mod surface;

pub use host::{CapabilityExecutor, CapabilityHandler, CapabilityRegistry};
pub use machine::{execute_batch, execute_query};
pub use program::{ResolvedExecutionProgram, ResolvedTable};
pub use state::{Overlay, OverlayResult};
pub use surface::{
    CapabilityEffect, ExecContext, ExecuteError, ExecutionJournal, ExecutionStateSummary,
    FailedTxExecution, QueryExecutionResult, SuccessfulTxExecution, TxExecutionOutcome,
    TypedStateSnapshot, TypedStateWrite,
};
