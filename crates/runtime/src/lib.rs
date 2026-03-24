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
//! Compiler-owned descriptor catalogs are sealed before this layer. Concrete
//! scheme and precompile backends are installed through `HostEnvironment` on
//! the `verify` / `prove` runtime surface.
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

#[cfg(any(feature = "prove", feature = "verify"))]
mod bootstrap;
mod error;
mod execute;
#[cfg(any(feature = "prove", feature = "verify"))]
mod host;
mod policy;
#[cfg(any(feature = "prove", feature = "verify"))]
mod program;
#[cfg(feature = "prove")]
mod proving;
#[cfg(feature = "prove")]
mod runtime;
#[cfg(test)]
mod testing;
#[cfg(feature = "verify")]
mod verifier;

pub use error::{RuntimeError, RuntimeResult};
pub use execute::{
    BatchInput, CompiledBatchInput, ExecutionEnvelope, run_batch, run_compiled_batch,
};

#[cfg(feature = "prove")]
pub use bootstrap::RuntimeBuilder;
#[cfg(any(feature = "prove", feature = "verify"))]
pub use host::{
    HostEnvironment, InstalledPrecompiles, InstalledSchemes, RuntimeRegistries, SmtScheme,
    SsmcScheme,
};
#[cfg(any(feature = "prove", feature = "verify"))]
pub use program::Binding;
#[cfg(feature = "prove")]
pub use program::{ProofPlan, ResolvedProofProgram, RuntimeProgram};
#[cfg(feature = "prove")]
pub use proving::{ProofSummary, ProveInput, ProveResult, VerifiedResult, digest_to_hex};
#[cfg(feature = "prove")]
/// Runtime built once per [`tabula_compiler::SealedProgram`].
pub use runtime::TabulaRuntime;
#[cfg(feature = "verify")]
pub use verifier::{Verifier, VerifierBuilder};
