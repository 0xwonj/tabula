//! Core types, traits, and error definitions for the Tabula kernel.

pub mod error;
pub mod execution;
pub mod ids;
pub mod state;
#[cfg(any(feature = "test-utils", test))]
pub mod testing;
pub mod traits;

// ── Identifier vocabulary ──
pub use ids::{
    ColId, ColumnCommitmentId, ColumnLayoutKind, ColumnProfileId, CommittedCellKey, CommittedKey,
    ContextFieldId, Digest, EncodingProfileId, EntryId, EventId, ProgramId, RootProfileId,
    RootProofFamilyId, RowKey, SchemeId, SchemeProfileId, StateRoot, TableCommitmentId, TableId,
    TxTypeId, TypeId,
};

// ── State model ──
pub use state::portable::PortableValue;
pub use state::schema::{
    CommittedKeyLayout, KeyComponentSchema, KeyOrderingFamily, ProgramExecutionContract,
    StateColumnContract, StateContract, StateTableContract, TableKeyContract,
};

// ── Execution boundary ──
pub use execution::property::{CommittedPropertyQuery, PropertyAggregateKind, PropertyQueryKind};
pub use execution::tx::{
    Batch, MachineCapabilities, ProgramBudgets, ProgramMachineShape, Transaction,
};
pub use execution::{
    CapabilityTranscriptId, CapabilityTranscriptSignature, CapabilityTranscriptValueProfile,
};

// ── Execution output ──
pub use execution::{
    AccessEvent, BatchReport, CapabilityCallEvent, ETraceEventId, EmittedEvent,
    ExecutionConsistencyStatus, LogicalTime, OpKind, PropertyQueryResult, PropertyReadResult,
    TxResult,
};

// ── Default implementations ──
pub use state::in_memory::{InMemoryState, InMemoryStaticTables};
