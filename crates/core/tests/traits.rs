#![allow(missing_docs)]
use tabula_core::traits::{
    BatchDigester, Hasher, NoncePolicy, SigVerifier, StateSnapshot, StaticTableProvider,
};

fn _assert_hasher_bounds<T: Hasher + Send + Sync>() {}
fn _assert_state_snapshot_bounds<T: StateSnapshot + Send + Sync>() {}
fn _assert_sig_verifier_bounds<T: SigVerifier + Send + Sync>() {}
fn _assert_nonce_policy_bounds<T: NoncePolicy + Send + Sync>() {}
fn _assert_static_table_provider_bounds<T: StaticTableProvider + Send + Sync>() {}
fn _assert_batch_digester_bounds<T: BatchDigester + Send + Sync>() {}

#[test]
fn trait_bounds_compile() {
    // This test simply verifies the trait definitions compile.
    // The actual bound-check functions above are compile-only.
}
