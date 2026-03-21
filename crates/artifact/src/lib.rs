//! Canonical artifact models and helpers shared by adapters and orchestration.

mod batch;
mod canonical;
mod error;
#[cfg(not(target_arch = "wasm32"))]
mod io;
mod program;
mod receipt;
mod state;

// Error
pub use error::ArtifactError;

// Program
pub use program::{Artifact, ColumnProofPlan, PrecompileDescriptor, SchemeDescriptor};

// State
pub use state::{State, StateEntry, merge_output_state_entries, normalize_state};

// Batch
pub use batch::{TransactionBatch, TransactionInput, parse_hex_32};

// Statement
pub use receipt::Statement;

// IO (non-wasm only)
#[cfg(not(target_arch = "wasm32"))]
pub use io::{load_json, write_json};
