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

// Artifact domain types (direct re-export for downstream use)
pub use tabula_artifact::{
    CreateInstanceCommand, ExecutionReceipt, ExecutionSummary, GetInstanceCommand,
    GetProgramCommand, GetRunCommand, InputRef, InstanceId, InstanceRecord, InstanceStatus,
    ListInstancesCommand, ListProgramsCommand, ListRunsCommand, ProgramId, ProgramInline,
    ProgramRecord, RegisterProgramCommand, RunId, RunRecord, RunStatus, StarkProofSummary,
    SubmitRunCommand, VerifyOutcome, VerifyRunCommand,
};

// Daemon-local types
pub use capabilities::{Capabilities, CapabilityClientKind, CapabilityInputMode};
pub use engine::LocalEngine;
pub use error::{ErrorKind, ServiceError, ServiceResult};
pub use io::FileAccessPolicy;
