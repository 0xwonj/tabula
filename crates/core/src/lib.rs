//! Core types, traits, and error definitions for the Tabula kernel.

pub mod error;
mod event;
#[cfg(any(feature = "test-utils", test))]
pub mod mock;
mod nonce;
mod query;
mod sig;
pub mod state;
pub mod traits;
mod tx;

// ── State model ──
pub use state::id::{
    CellKey, ColId, ColumnCommitmentId, ColumnLayoutKind, ColumnProfileId, Digest,
    EncodingProfileId, RootProfileId, RootProofFamilyId, RowKey, SchemeId, SchemeProfileId,
    StateRoot, TableCommitmentId, TableId, TxTypeId, TypeId,
};
pub use state::portable::PortableValue;
pub use state::schema::{ColumnDef, TableSchema};

// ── Transaction model ──
pub use tx::{Batch, ProgramBudgets, Transaction};

// ── Execution output ──
pub use event::{
    AccessEvent, BatchReport, ETraceEventId, EmittedEvent, ExecutionConsistencyStatus, LogicalTime,
    OpKind, PrecompileEvent, PropertyQueryResult, PropertyReadResult, TxResult,
};
pub use query::PropertyQueryKind;

// ── Default implementations ──
pub use nonce::SequentialNonce;
pub use sig::NoopSigVerifier;
pub use state::in_memory::{InMemoryState, InMemoryStaticTables};
