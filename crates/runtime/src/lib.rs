//! Tabula execution runtime.
//!
//! Canonical batch execution pipeline, separated from the compiler
//! to enable independent consumption by CLI, daemon, and embedded
//! applications.
//!
//! # Architecture
//!
//! The runtime sits between the executor (zero-crypto deterministic VM) and
//! the machine (STARK prover). It assembles the execution pipeline:
//! normalize state -> build snapshot -> execute -> consistency check -> merge.
//!
//! ```text
//! compiler (compile/load/register) -> runtime (execute/prove) -> machine (STARK)
//! ```
//!
//! # Feature gating
//!
//! - **No features** (default): only [`run_batch()`] — zero crypto deps.
//! - **`prove`**: adds [`TabulaRuntime`], [`RuntimeBuilder`], and the full
//!   witness → trace → prove pipeline.

mod error;
mod execute;

#[cfg(feature = "prove")]
mod builder;
#[cfg(feature = "prove")]
mod committed_state;
#[cfg(feature = "prove")]
pub mod prove;
#[cfg(feature = "prove")]
mod runtime;

pub use error::{RuntimeError, RuntimeResult};
pub use execute::{BatchInput, CompiledBatchInput, ExecutedBatch, run_batch, run_compiled_batch};

#[cfg(feature = "prove")]
pub use builder::RuntimeBuilder;
#[cfg(feature = "prove")]
pub use prove::{ProofSummary, ProveInput, ProveResult, VerifiedResult, digest_to_hex};
#[cfg(feature = "prove")]
/// Prepared runtime built once per [`tabula_artifact::CompiledProgram`].
pub use runtime::TabulaRuntime as PreparedRuntime;
#[cfg(feature = "prove")]
pub use runtime::TabulaRuntime;
