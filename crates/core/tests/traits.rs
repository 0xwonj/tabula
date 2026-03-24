#![allow(missing_docs)]
use tabula_core::traits::{
    BatchDigester, Hasher, MembershipScheme, StateView, StaticTableProvider,
};

fn _assert_hasher_bounds<T: Hasher + Send + Sync>() {}
fn _assert_state_view_bounds<T: StateView + Send + Sync>() {}
fn _assert_membership_scheme_bounds<T: MembershipScheme + Send + Sync>() {}
fn _assert_static_table_provider_bounds<T: StaticTableProvider + Send + Sync>() {}
fn _assert_batch_digester_bounds<T: BatchDigester + Send + Sync>() {}

#[test]
fn trait_bounds_compile() {
    // This test simply verifies the trait definitions compile.
    // The actual bound-check functions above are compile-only.
}
