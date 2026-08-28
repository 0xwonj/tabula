//! SP-5 §12 guardrail: `Send + Sync + 'static` bounds on prepared handles.
//!
//! Each prepared handle is designed to be cheap to share via `Arc`.
//! These tests assert the compile-time `Send + Sync + 'static` contract
//! for every handle so the invariant is visible in CI output and greppable
//! by name (complementing the inline `const _: fn() = ...` assertions in
//! each handle module).

#![allow(missing_docs)]

/// Asserts that `T: Send + Sync + 'static`.
fn assert_bound<T: Send + Sync + 'static>() {}

#[cfg(feature = "prove")]
#[test]
fn prepared_prover_is_send_sync_static() {
    assert_bound::<tabula_runtime::PreparedProver>();
}

#[cfg(feature = "verify")]
#[test]
fn prepared_verifier_is_send_sync_static() {
    assert_bound::<tabula_runtime::PreparedVerifier>();
}

#[cfg(feature = "verify")]
#[test]
fn prepared_executor_is_send_sync_static() {
    assert_bound::<tabula_runtime::PreparedExecutor>();
}
