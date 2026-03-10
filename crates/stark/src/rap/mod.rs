//! RAP (Randomized Air with Preprocessing) constraint evaluation.
//!
//! Provides prover and verifier constraint folders for the two-phase evaluation
//! pattern. Phase 1 (inner chip constraints) uses p3's standard folders; Phase 2
//! (LogUp RAP constraints) uses these specialized folders that:
//!
//! - Suppress `assert_zero()` (main constraints already folded)
//! - Intercept `send()`/`receive()` to generate phi·f=m and cumsum constraints
//!
//! Also provides shared EF4 arithmetic helpers used by both folders.

pub mod ef4;
pub mod prover;
pub mod verifier;
