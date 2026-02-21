//! Service layer — engine, catalog, types, and supporting modules.

mod catalog;
pub mod commands;
pub mod engine;
pub mod error;
mod execute;
pub mod io;
#[cfg(feature = "stark")]
pub(crate) mod prove;
mod receipt;
pub mod types;

pub use commands::{
    CreateInstanceCommand, GetInstanceCommand, GetProgramCommand, GetRunCommand, InputRef,
    ListInstancesCommand, ListProgramsCommand, ListRunsCommand, ProgramInline,
    RegisterProgramCommand, SubmitRunCommand, VerifyRunCommand,
};
pub use engine::LocalEngine;
pub use error::{ErrorKind, ServiceError, ServiceResult};
pub use io::FileAccessPolicy;
pub use tabula_artifact::StarkProofSummary;
pub use types::{
    Capabilities, CapabilityClientKind, CapabilityInputMode, ExecutionReceipt, ExecutionResult,
    InstanceRecord, InstanceStatus, ProgramRecord, RunRecord, RunStatus, VerifyOutcome,
};
