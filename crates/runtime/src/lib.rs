//! Tabula execution runtime.
//!
//! Canonical batch execution pipeline, separated from the compiler
//! to enable independent consumption by CLI, daemon, and embedded
//! applications.
//!
//! # Architecture
//!
//! The runtime is the default public proving surface.
//! It sits between the executor (zero-crypto deterministic VM) and
//! the machine (advanced STARK backend). It assembles the execution pipeline:
//! normalize state -> build snapshot -> execute -> consistency check -> merge.
//!
//! ```text
//! compiler (compile/load/register) -> runtime (execute/prove) -> machine (STARK)
//! ```
//!
//! # Feature gating
//!
//! - **No features** (default): only [`run_batch()`] — zero crypto deps.
//! - **`verify`**: adds [`ProgramVerifier`] and [`ProgramVerifierBuilder`] for
//!   proof verification against sealed program artifacts.
//! - **`prove`**: adds [`TabulaRuntime`], [`RuntimeBuilder`], and the full
//!   witness → trace → prove pipeline. Implies `verify`.

#[cfg(any(feature = "prove", feature = "verify"))]
mod assembly;
#[cfg(feature = "prove")]
mod builder;
#[cfg(feature = "prove")]
mod capabilities;
#[cfg(any(feature = "prove", feature = "verify"))]
mod columns;
mod error;
mod execute;
#[cfg(any(feature = "prove", feature = "verify"))]
mod program;
#[cfg(feature = "prove")]
mod proving;
#[cfg(feature = "prove")]
mod runtime;
#[cfg(feature = "verify")]
mod verifier;

pub use error::{RuntimeError, RuntimeResult};
pub use execute::{BatchInput, CompiledBatchInput, ExecutedBatch, run_batch, run_compiled_batch};

#[cfg(feature = "prove")]
pub use builder::RuntimeBuilder;
#[cfg(feature = "prove")]
pub use capabilities::PrecompileRegistration;
#[cfg(any(feature = "prove", feature = "verify"))]
pub use columns::{ColumnPlan, ColumnSchemeFactory, ColumnViews, RuntimeColumn};
#[cfg(feature = "prove")]
pub use columns::ProofInputBuilder;
#[cfg(any(feature = "prove", feature = "verify"))]
pub use columns::{SmtScheme, SsmcScheme};
#[cfg(any(feature = "prove", feature = "verify"))]
pub use program::ProgramBinding;
#[cfg(feature = "prove")]
pub use program::RuntimeProgram;
#[cfg(feature = "prove")]
pub use proving::{ProofSummary, ProveInput, ProveResult, VerifiedResult, digest_to_hex};
#[cfg(feature = "prove")]
/// Runtime built once per [`tabula_compiler::CompiledProgram`].
pub use runtime::TabulaRuntime;
#[cfg(feature = "verify")]
pub use verifier::{ProgramVerifier, ProgramVerifierBuilder};
