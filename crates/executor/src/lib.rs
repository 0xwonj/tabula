//! Deterministic execution engine for the Tabula kernel.
//!
//! # Primary API
//!
//! Most callers need only the re-exports below. The underlying modules remain
//! `pub` for advanced use cases (custom environments, overlay introspection).
//!
//! - [`execute_batch`] — execute a full transaction batch
//! - [`BatchEnv`] — environment (hasher, sig verifier, nonce policy, static tables)
//! - [`execute`] — execute a single transaction body
//! - [`check_consistency_status`] — post-execution consistency audit
//! - [`Overlay`] / [`OverlayResult`] — per-tx state overlay

pub mod batch;
pub mod consistency;
mod execution_state;
pub mod interpreter;
pub mod overlay;
pub mod precompile;
pub mod property;
pub mod resolve;
mod trace_recorder;

// ── Curated re-exports ──────────────────────────────────────────────────────

pub use batch::{BatchEnv, execute_batch};
pub use consistency::check_consistency_status;
pub use interpreter::execute;
pub use overlay::{Overlay, OverlayResult};
pub use precompile::{PrecompileHandler, PrecompileRegistry};
pub use property::{CommittedStateProvider, PropertyOpeningRegistry, PropertyOpeningResolver};
