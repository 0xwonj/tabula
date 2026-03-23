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
//! Compiler-owned descriptor catalogs are sealed before this layer; concrete
//! scheme and precompile backends are installed through [`HostEnvironment`].
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
mod error;
mod execute;
mod host;
#[cfg(any(feature = "prove", feature = "verify"))]
mod machine_config;
#[cfg(any(feature = "prove", feature = "verify"))]
mod program;
#[cfg(feature = "prove")]
mod proving;
#[cfg(feature = "prove")]
mod runtime;
#[cfg(any(feature = "prove", feature = "verify"))]
mod schemes;
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
pub use host::{HostEnvironment, HostTypeRuntimes, InstalledPrecompiles, InstalledSchemes};
#[cfg(any(feature = "prove", feature = "verify"))]
pub use machine_config::MachineConfig;
#[cfg(any(feature = "prove", feature = "verify"))]
pub use program::Binding;
#[cfg(feature = "prove")]
pub use program::ResolvedProgram;
#[cfg(feature = "prove")]
pub use proving::{ProofSummary, ProveInput, ProveResult, VerifiedResult, digest_to_hex};
#[cfg(feature = "prove")]
/// Runtime built once per [`tabula_compiler::SealedProgram`].
pub use runtime::TabulaRuntime;
#[cfg(any(feature = "prove", feature = "verify"))]
pub use schemes::{SmtScheme, SsmcScheme};
#[cfg(feature = "verify")]
pub use verifier::{Verifier, VerifierBuilder};
