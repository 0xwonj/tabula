//! Tabula execution runtime.
//!
//! Canonical batch execution pipeline, separated from the compiler
//! to enable independent consumption by CLI, daemon, and embedded
//! applications.
//!
//! # Architecture
//!
//! The runtime is the canonical proving engine layer.
//! It sits between the executor (zero-crypto deterministic VM) and
//! the machine (STARK backend). It assembles the execution pipeline:
//! normalize state -> build snapshot -> execute -> consistency check -> merge.
//! Stable runtime APIs remain backend-neutral. Extension authoring lives in
//! `tabula-ext`, while the runtime consumes those contracts and prepares
//! prove/verify resources internally before handing traces to the machine.
//!
//! ```text
//! compiler (compile/load/register) -> runtime (execute/prove) -> machine (STARK)
//! ```
//!
//! # Feature gating
//!
//! - **No features** (default): only [`run_batch()`] — zero crypto deps.
//! - **`verify`**: adds [`Verifier`] and [`VerifierBuilder`] for
//!   proof verification against sealed artifacts.
//! - **`prove`**: adds [`TabulaRuntime`], [`RuntimeBuilder`], and the full
//!   witness → trace → prove pipeline. Implies `verify`.

#[cfg(feature = "prove")]
mod builder;
#[cfg(feature = "prove")]
mod capabilities;
#[cfg(any(feature = "prove", feature = "verify"))]
mod columns;
mod error;
mod execute;
#[cfg(any(feature = "prove", feature = "verify"))]
mod precompile_proofs;
#[cfg(any(feature = "prove", feature = "verify"))]
mod program;
#[cfg(any(feature = "prove", feature = "verify"))]
mod proof_extensions;
#[cfg(feature = "prove")]
mod proving;
#[cfg(feature = "prove")]
mod runtime;
#[cfg(any(feature = "prove", feature = "verify"))]
mod setup;
#[cfg(test)]
mod testing;
#[cfg(feature = "verify")]
mod verifier;

pub use error::{RuntimeError, RuntimeResult};
pub use execute::{BatchInput, CompiledBatchInput, ExecutedBatch, run_batch, run_compiled_batch};

#[cfg(feature = "prove")]
pub use builder::RuntimeBuilder;
#[cfg(any(feature = "prove", feature = "verify"))]
pub use columns::{SmtScheme, SsmcScheme};
#[cfg(any(feature = "prove", feature = "verify"))]
pub use program::Binding;
#[cfg(feature = "prove")]
pub use program::ResolvedProgram;
#[cfg(feature = "prove")]
pub use proving::{ProofSummary, ProveInput, ProveResult, VerifiedResult, digest_to_hex};
#[cfg(feature = "prove")]
/// Runtime built once per [`tabula_compiler::SealedProgram`].
pub use runtime::TabulaRuntime;
#[cfg(feature = "verify")]
pub use verifier::{Verifier, VerifierBuilder};
