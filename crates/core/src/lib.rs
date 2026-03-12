//! Core types, traits, and error definitions for the Tabula kernel.

pub mod error;
mod event;
#[cfg(any(feature = "test-utils", test))]
pub mod mock;
mod state;
pub mod traits;
mod tx;

// ── State model ──
pub use state::id::{
    CellKey, ColId, ColumnCommitmentId, Digest, RowKey, StateRoot, TableCommitmentId, TableId,
    TxTypeId,
};
pub use state::schema::{ColumnDef, TableSchema};
pub use state::value::{Value, ValueType, zero_value};

// ── Transaction model ──
pub use tx::{Batch, ProgramBudgets, Transaction};

// ── Execution output ──
pub use event::{
    AccessEvent, ETraceEventId, EmittedEvent, ExecutionConsistencyStatus, ExecutionResult,
    LogicalTime, OpKind, TxOutcome,
};
