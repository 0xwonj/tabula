//! Integration tests for seal-time computation of `relation_policy` and
//! `uses_ir_hash` bits on `SealedArtifact`.
//!
//! These bits are computed once during `register_program_from_source` (i.e.
//! at `compile_and_register_program_source` time) and stored on the sealed
//! artifact.  This file pins that the bits are set correctly for programs
//! with / without relations and hash ops.
//!
//! The `poseidon_hash` capability is pre-registered in `standard_catalogs()`
//! inside `tabula_testing`, so sources can declare `use capability
//! poseidon_hash;` and call it normally.

#![allow(missing_docs)]

use tabula_contract::SealedRelationPolicy;
use tabula_testing::exec::register_program_from_source;

// ─── helpers ─────────────────────────────────────────────────────────────────

/// A minimal program with no relation and no hash op.
fn no_rel_no_hash_source() -> &'static str {
    r#"
program NoRelOrHash
context { x: u64; }
state { table t(key id: u64) { v: u64 @ssmc; } }
tx noop() { return; }
"#
}

/// A program that uses `assert relation`, triggering `RequireArtifactRoot`.
fn relation_only_source() -> &'static str {
    r#"
program RelationOnly
relation R(x: u64) = enum { 0, 1 };
tx check(x: u64) {
  assert relation R(x);
  return;
}
"#
}

/// A program that calls the `poseidon_hash` capability, which lowers to
/// `ir::Op::Hash` and should set `uses_ir_hash = true`.
///
/// The hash result is emitted in an event so dead-code elimination does not
/// prune the `Hash` op before the IR scan.  Events accept `bytes32` without
/// requiring a storage scheme.
fn hash_only_source() -> &'static str {
    r#"
use capability poseidon_hash;

program HashOnly
event Hashed(d: bytes32);
state {
  table t(key id: u64) { v: u64 @ssmc; }
}
tx store(id: u64, v: u64) {
  let d = poseidon_hash(v);
  emit Hashed(d);
  t[id].v = v;
  return;
}
"#
}

/// A program that uses both a relation and the hash capability.
///
/// Hash result is emitted in an event to prevent dead-code elimination.
fn rel_and_hash_source() -> &'static str {
    r#"
use capability poseidon_hash;

program RelAndHash
event Hashed(d: bytes32);
relation R(x: u64) = enum { 0, 1 };
state {
  table t(key id: u64) { v: u64 @ssmc; }
}
tx store(id: u64, v: u64) {
  assert relation R(v);
  let d = poseidon_hash(v);
  emit Hashed(d);
  t[id].v = v;
  return;
}
"#
}

// ─── test cases ──────────────────────────────────────────────────────────────

#[test]
fn no_relation_no_hash_seals_disabled_and_no_hash() {
    let registered = register_program_from_source(no_rel_no_hash_source());
    let sealed = registered.sealed();

    assert_eq!(
        sealed.relation_policy(),
        SealedRelationPolicy::Disabled,
        "program with no relations must seal Disabled relation policy"
    );
    assert!(
        !sealed.uses_ir_hash(),
        "program with no hash ops must seal uses_ir_hash = false"
    );
}

#[test]
fn assert_relation_seals_require_artifact_root() {
    let registered = register_program_from_source(relation_only_source());
    let sealed = registered.sealed();

    assert_eq!(
        sealed.relation_policy(),
        SealedRelationPolicy::RequireArtifactRoot,
        "program with assert relation must seal RequireArtifactRoot policy"
    );
    assert!(
        !sealed.uses_ir_hash(),
        "relation-only program must seal uses_ir_hash = false"
    );
}

#[test]
fn hash_capability_seals_uses_ir_hash_true() {
    let registered = register_program_from_source(hash_only_source());
    let sealed = registered.sealed();

    assert_eq!(
        sealed.relation_policy(),
        SealedRelationPolicy::Disabled,
        "hash-only program must seal Disabled relation policy"
    );
    assert!(
        sealed.uses_ir_hash(),
        "program using poseidon_hash capability must seal uses_ir_hash = true"
    );
}

#[test]
fn relation_and_hash_seals_both_bits() {
    let registered = register_program_from_source(rel_and_hash_source());
    let sealed = registered.sealed();

    assert_eq!(
        sealed.relation_policy(),
        SealedRelationPolicy::RequireArtifactRoot,
        "program with both relation and hash must seal RequireArtifactRoot"
    );
    assert!(
        sealed.uses_ir_hash(),
        "program with both relation and hash must seal uses_ir_hash = true"
    );
}
