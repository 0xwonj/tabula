//! Service layer — engine, catalog, capabilities, and supporting modules.

pub mod capabilities;
mod catalog;
pub mod engine;
pub mod error;
mod execute;
pub mod io;
#[cfg(feature = "stark")]
pub(crate) mod prove;
mod receipt;
mod types;

pub use types::{
    BatchInputRef, ChipSummary, CreateInstanceCommand, ExecutionReceipt, ExecutionSummary,
    GetInstanceCommand, GetProgramCommand, GetRunCommand, InputRef, InstanceId, InstanceRecord,
    InstanceStatus, ListInstancesCommand, ListProgramsCommand, ListRunsCommand, ProgramId,
    ProgramInline, ProgramInputRef, ProgramRecord, RegisterProgramCommand, RunId, RunRecord,
    RunStatus, StarkProofSummary, StateInputRef, SubmitRunCommand, VerifyOutcome, VerifyRunCommand,
};

// Daemon-local types
pub use capabilities::{Capabilities, CapabilityClientKind, CapabilityInputMode};
pub use engine::LocalEngine;
pub use error::{ErrorKind, ServiceError, ServiceResult};
pub use io::FileAccessPolicy;
