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
//! Verification is statement-first: [`PublicStatement`] is the proved object,
//! `BoundStatement` is the verifier-side binding over one sealed
//! artifact, and the prepared verifier state lives in the runtime verifier
//! surface rather than the contract layer. Shared registered-program setup is
//! derived in `bootstrap`, then consumed by `engine` for proving and
//! `verifier` for verification.
//!
//! ```text
//! compiler (compile/load/register) -> runtime (execute/prove) -> machine (STARK)
//! ```
//!
//! # Feature gating
//!
//! - **No features** (default): shared semantic helpers and error types only.
//! - **`verify`**: adds [`PreparedVerifier`], [`PreparedVerifierBuilder`],
//!   the [`prepare_verifier`] free function, and the public
//!   [`VerifierState`] type for native proof verification.
//! - **`prove`**: adds [`TabulaRuntime`], [`RuntimeBuilder`],
//!   [`PreparedProver`], [`PreparedProverBuilder`], the
//!   [`prepare_prover`] free function, and the full native
//!   witness → trace → prove pipeline. Implies `verify`.

#[cfg(feature = "verify")]
mod bootstrap;
#[cfg(feature = "verify")]
mod engine;
mod error;
#[cfg(feature = "verify")]
mod host;
#[cfg(feature = "prove")]
mod proof_summary;
#[cfg(feature = "prove")]
mod prover;
pub mod semantics;
#[cfg(feature = "verify")]
mod state_runtime;
#[cfg(feature = "verify")]
mod verifier;

pub use error::{RuntimeError, RuntimeResult};

#[cfg(feature = "verify")]
pub use bootstrap::program::RelationPolicy;
#[cfg(feature = "verify")]
pub use engine::{CommittedStateSnapshot, ExecutionReceipt, RuntimeBuilder, TabulaRuntime};
#[cfg(feature = "prove")]
pub use engine::{ProveInput, ProveResult, VerifiedResult};
#[cfg(feature = "verify")]
pub use host::{HostEnvironment, InstalledSchemes, RuntimeRegistries, SmtScheme, SsmcScheme};
#[cfg(feature = "prove")]
pub use proof_summary::ProofSummary;
#[cfg(feature = "prove")]
pub use prover::{PreparedProver, PreparedProverBuilder, prepare_prover};
#[cfg(feature = "verify")]
pub use tabula_contract::{BoundStatement, PublicStatement};
#[cfg(feature = "verify")]
pub use verifier::{PreparedVerifier, PreparedVerifierBuilder, VerifierState, prepare_verifier};
