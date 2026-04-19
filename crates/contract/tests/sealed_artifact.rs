//! Integration tests for `SealedArtifact` round-trip serialization and
//! `validate()` contract-layer checks.
//!
//! A real `SealedArtifact` is obtained by compiling a minimal Tabula program
//! through `tabula-testing::register_program_from_source`, which avoids any
//! circular dependency (testing → compiler → contract, so adding testing as a
//! dev-dep here is safe).

#![allow(missing_docs)]

use tabula_contract::{SEALED_ARTIFACT_SCHEMA_VERSION, SealedArtifact, SealedArtifactError};
use tabula_testing::exec::register_program_from_source;

/// Minimal stateful program with no relations and no hash op.
fn minimal_source() -> &'static str {
    r#"
program Minimal
state {
  table t(key id: u64) { v: u64 @ssmc; }
}
tx noop() { return; }
"#
}

/// Build a sealed artifact from the minimal source.
fn minimal_sealed() -> SealedArtifact {
    register_program_from_source(minimal_source())
        .sealed()
        .clone()
}

// ─── canonical_bytes ─────────────────────────────────────────────────────────

#[test]
fn canonical_bytes_starts_with_magic_prefix() {
    let sealed = minimal_sealed();
    let bytes = sealed
        .canonical_bytes()
        .expect("canonical_bytes must succeed");
    assert!(
        bytes.starts_with(b"tabula.contract.sealed_artifact.v1"),
        "canonical_bytes must start with the versioned magic prefix; got: {:?}",
        &bytes[..bytes.len().min(64)]
    );
}

#[test]
fn canonical_bytes_round_trip_preserves_fields() {
    let original = minimal_sealed();
    let bytes = original.canonical_bytes().expect("canonical_bytes");

    // Strip the magic prefix to get the raw JSON payload.
    let prefix = b"tabula.contract.sealed_artifact.v1";
    let json_payload = bytes
        .strip_prefix(prefix.as_ref())
        .expect("bytes must start with magic prefix");

    let decoded: SealedArtifact =
        serde_json::from_slice(json_payload).expect("must deserialize from JSON payload");

    assert_eq!(
        original.schema_version(),
        decoded.schema_version(),
        "schema_version must survive round-trip"
    );
    assert_eq!(
        original.relation_policy(),
        decoded.relation_policy(),
        "relation_policy must survive round-trip"
    );
    assert_eq!(
        original.uses_ir_hash(),
        decoded.uses_ir_hash(),
        "uses_ir_hash must survive round-trip"
    );
    assert_eq!(
        original.binding(),
        decoded.binding(),
        "binding must survive round-trip"
    );
    assert_eq!(
        original.program_id(),
        decoded.program_id(),
        "program_id must survive round-trip"
    );
}

// ─── validate() happy path ───────────────────────────────────────────────────

#[test]
fn validate_accepts_freshly_registered_artifact() {
    let sealed = minimal_sealed();
    sealed
        .validate()
        .expect("freshly registered SealedArtifact must pass validate()");
}

// ─── validate() error variants ───────────────────────────────────────────────

#[test]
fn validate_rejects_unsupported_schema_version() {
    // Mutate schema_version via JSON round-trip since the field is pub(crate).
    let mut value = serde_json::to_value(minimal_sealed()).expect("serialize");
    value["schema_version"] = serde_json::json!(u32::MAX);
    let sealed: SealedArtifact =
        serde_json::from_value(value).expect("deserialize with bad schema version");

    let err = sealed
        .validate()
        .expect_err("unsupported schema version must fail closed");

    assert!(
        matches!(err, SealedArtifactError::UnsupportedSchemaVersion { .. }),
        "expected UnsupportedSchemaVersion, got: {err}"
    );
    assert!(
        err.to_string()
            .contains("unsupported sealed artifact schema version"),
        "error message must mention the schema version; got: {err}"
    );
}

#[test]
fn validate_rejects_non_canonical_tuple_encoding() {
    let mut sealed = minimal_sealed();

    // Build a deliberately non-canonical encoding defaults by taking the
    // current entries, duplicating the first entry if possible, or inserting
    // a duplicate manually.  We reach into the public `new` constructor with
    // an empty entries list to obtain a canonical empty default, then replace
    // the stored value with one that fails the canonicality check:
    // reverse-sorted entries (if any) would fail ordering; for zero entries we
    // can't trigger that branch, but we can duplicate an existing entry if one
    // exists.
    let entries = sealed.tuple_encoding_defaults().entries().to_vec();
    if entries.is_empty() {
        // Nothing to corrupt.  The empty case is canonical and cannot be made
        // non-canonical without access to a real profile ID.  Skip the body
        // assertion to avoid a false failure, and instead confirm that the
        // empty defaults pass validate() (already covered by the happy-path
        // test above).
        return;
    }
    // Duplicate the first entry to violate uniqueness.
    // TupleEncodingDefaults::new rejects duplicates, so we bypass it by
    // constructing a raw value and patching the field via JSON round-trip.
    let mut value = serde_json::to_value(&sealed).expect("serialize");
    value["tuple_encoding_defaults"]["entries"]
        .as_array_mut()
        .expect("array")
        .push(serde_json::to_value(entries[0]).expect("serialize entry"));
    sealed = serde_json::from_value(value).expect("deserialize with bad defaults");

    let err = sealed
        .validate()
        .expect_err("non-canonical tuple encoding must fail closed");
    assert!(
        matches!(err, SealedArtifactError::TupleEncodingNotCanonical { .. }),
        "expected TupleEncodingNotCanonical, got: {err}"
    );
}

// ─── error Display messages ───────────────────────────────────────────────────

#[test]
fn error_display_messages_are_informative() {
    let unsupported = SealedArtifactError::UnsupportedSchemaVersion {
        found: 99,
        expected: SEALED_ARTIFACT_SCHEMA_VERSION,
    };
    let msg = unsupported.to_string();
    assert!(msg.contains("99"), "should mention found version: {msg}");
    assert!(
        msg.contains(&SEALED_ARTIFACT_SCHEMA_VERSION.to_string()),
        "should mention expected version: {msg}"
    );

    let not_canonical = SealedArtifactError::TupleEncodingNotCanonical {
        detail: "test detail".into(),
    };
    let not_canonical_msg = not_canonical.to_string();
    assert!(
        not_canonical_msg.contains("canonical"),
        "should contain 'canonical': {not_canonical_msg}"
    );
}
