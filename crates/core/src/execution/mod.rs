//! Execution-boundary vocabulary shared across runtime, IR, and proof prep.

pub mod capability_transcript;
pub mod property;
pub mod report;
pub mod tx;

pub use capability_transcript::{
    CapabilityTranscriptId, CapabilityTranscriptSignature, CapabilityTranscriptValueProfile,
};
pub use property::{
    CommittedPropertyQuery, PropertyAggregateKind, PropertyQueryKind, PropertyQueryResult,
    PropertyReadResult,
};
pub use report::{
    AccessEvent, BatchReport, CapabilityCallEvent, ETraceEventId, EmittedEvent,
    ExecutionConsistencyStatus, LogicalTime, OpKind, TxResult,
};
pub use tx::{
    Batch, MachineCapabilities, NATIVE_MAX_KEY_COMPONENTS, NATIVE_MAX_KEY_FES, NATIVE_MAX_SLOTS,
    ProgramBudgets, ProgramMachineShape, Transaction,
};
