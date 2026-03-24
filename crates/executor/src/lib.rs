//! Deterministic execution engine for the Tabula kernel.
//!
//! # Primary API
//!
//! Most callers need only the re-exports below. The underlying modules remain
//! `pub` for advanced use cases (custom environments, overlay introspection).
//!
//! - [`execute_batch`] — execute a full transaction batch against the
//!   canonical resolved execution contract
//! - [`derive_batch_report`] / [`derive_portable_state_summary`] /
//!   [`derive_consistency_status`] — explicit
//!   reporting projections from the canonical journal
//! - [`BatchEnv`] — execution environment (hasher, static tables, type runtimes, optional services)
//! - [`execute_tx`] — execute a single transaction body for tests and harnesses
//! - [`Overlay`] / [`OverlayResult`] — per-tx state overlay

pub mod batch;
pub mod consistency;
mod execution_state;
pub mod interpreter;
pub mod journal;
pub mod overlay;
pub mod precompile;
pub mod property;
pub mod resolve;
pub mod resolved_program;

// ── Curated re-exports ──────────────────────────────────────────────────────

pub use batch::{BatchEnv, execute_batch};
pub use interpreter::{execute, execute_tx};
pub use journal::{
    ExecutionJournal, ExecutionStateSummary, FailedAccessObservation, FailedTxExecution,
    IrHashEffect, PortableStateSummary, SuccessfulTxExecution, TxExecutionOutcome,
    TypedAccessEffect, TypedPrecompileCallEffect, TypedPropertyReadEffect, TypedStateSnapshot,
    TypedStateWrite, derive_batch_report, derive_consistency_status, derive_portable_state_summary,
};
pub use overlay::{Overlay, OverlayResult};
pub use precompile::{PrecompileHandler, PrecompileRegistry};
pub use property::{CommittedStateProvider, PropertyQueryHandler, PropertyQueryRegistry};
pub use resolved_program::{ResolvedColumnLayout, ResolvedExecutionProgram, ResolvedTxDefinition};
