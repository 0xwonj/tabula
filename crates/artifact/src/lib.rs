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
pub use state::{StateCell, StateFile, merge_output_state_cells, normalize_state};

// Batch
pub use batch::{BatchFile, TxInput, parse_hex_32};

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
