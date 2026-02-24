#![warn(missing_docs)]
#![deny(unused)]

//! Canonical artifact models and helpers shared by adapters and orchestration.

mod batch;
mod commands;
mod error;
#[cfg(not(target_arch = "wasm32"))]
mod io;
mod program;
mod receipt;
mod records;
mod state;

// Error
pub use error::ArtifactError;

// Program
pub use program::ProgramArtifact;

// State
pub use state::{merge_output_state_cells, normalize_state, StateCell, StateFile};

// Batch
pub use batch::{parse_hex_32, BatchFile, TxInput};

// Receipt
pub use receipt::{ChipSummary, ExecutionReceipt, StarkProofSummary};

// Records
pub use records::{
    ExecutionSummary, InstanceId, InstanceRecord, InstanceStatus, ProgramId, ProgramRecord, RunId,
    RunRecord, RunStatus, VerifyOutcome,
};

// Commands
pub use commands::{
    BatchInputRef, CreateInstanceCommand, GetInstanceCommand, GetProgramCommand, GetRunCommand,
    InputRef, ListInstancesCommand, ListProgramsCommand, ListRunsCommand, ProgramInline,
    ProgramInputRef, RegisterProgramCommand, StateInputRef, SubmitRunCommand, VerifyRunCommand,
};

// IO (non-wasm only)
#[cfg(not(target_arch = "wasm32"))]
pub use io::{load_json, write_json};
