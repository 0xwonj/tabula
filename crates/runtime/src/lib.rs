//! Tabula execution runtime.
//!
//! Canonical batch execution pipeline, separated from the compiler
//! to enable independent consumption by CLI and embedded
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
//! Compiler-owned descriptor catalogs are sealed before this layer.
//! `HostEnvironment` currently installs runtime registries and scheme
//! backends on the `verify` / `prove` surface; end-to-end capability handler
//! installation remains a follow-up architecture step.
//!
//! ```text
//! compiler (compile/load/register) -> runtime (execute/prove) -> machine (STARK)
//! ```
//!
//! # Feature gating
//!
//! - **No features** (default): shared semantic helpers and error types only.
//! - **`verify`**: adds [`Verifier`] and [`VerifierBuilder`] for
//!   native proof verification against registered programs.
//! - **`prove`**: adds [`TabulaRuntime`], [`RuntimeBuilder`], and the full
//!   native witness → trace → prove pipeline. Implies `verify`.

#[cfg(feature = "verify")]
#[allow(dead_code)]
mod bootstrap;
#[cfg(feature = "verify")]
mod engine;
mod error;
#[cfg(feature = "verify")]
mod host;
#[cfg(feature = "prove")]
mod proof_summary;
pub mod semantics;

pub use error::{RuntimeError, RuntimeResult};

#[cfg(feature = "verify")]
pub use engine::{ExecutionReceipt, ProofStatement, RuntimeBuilder, StateSnapshot, TabulaRuntime};
#[cfg(feature = "prove")]
pub use engine::{ProveInput, ProveResult, VerifiedResult};
#[cfg(feature = "verify")]
pub use engine::{Verifier, VerifierBuilder};
#[cfg(feature = "verify")]
pub use host::{HostEnvironment, InstalledSchemes, RuntimeRegistries, SmtScheme, SsmcScheme};
#[cfg(feature = "prove")]
pub use proof_summary::ProofSummary;
