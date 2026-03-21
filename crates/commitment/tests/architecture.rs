//! Architecture guardrails for the commitment crate public surface.

use std::fs;
use std::path::Path;

fn crate_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn root_surface_stays_native_only() {
    let lib = fs::read_to_string(crate_root().join("src/lib.rs")).expect("read lib.rs");

    for forbidden in [
        "pub use schemes::smt::{",
        "pub use schemes::ssmc::{",
        "pub use roots::{compute_column_meta_leaf",
        "pub use roots::{compute_table_root",
        "pub use roots::{compute_state_root",
        "pub use schemes::tags",
        "MockFieldHasher",
        "NativeFieldHasher",
        "encode_trace",
        "decode_trace",
        "trace_width",
        "MergeSource",
        "MergeStep",
        "MergeTrace",
    ] {
        assert!(
            !lib.contains(forbidden),
            "commitment root must not export witness or merge-trace helper '{forbidden}'"
        );
    }

    assert!(
        lib.contains(
            "pub use primitives::{FieldHasher, KoalaBearCodec, NativeDigest, PoseidonHasher};"
        ),
        "commitment root should still export the native KoalaBear codec"
    );
    assert!(
        lib.contains("pub use roots::compute_state_roots_from_metas;"),
        "commitment root should keep the high-level state-root entrypoint"
    );
}

#[test]
fn column_state_public_apply_writes_is_trace_free() {
    let column = fs::read_to_string(crate_root().join("src/column.rs")).expect("read column.rs");

    assert!(
        column.contains("pub fn apply_writes("),
        "ColumnState must keep a public apply_writes API"
    );
    assert!(
        column.contains(") -> Result<(Self, NativeDigest), TabulaError> {"),
        "public apply_writes must return only the new state and commitment, wrapped in Result"
    );
    assert!(
        !column.contains("apply_writes_with_trace"),
        "internal trace-specific apply_writes helper should be removed when unused"
    );
    assert!(
        column.contains("pub fn proof_commitment("),
        "ColumnState should own the proof-visible commitment bridge as a method"
    );
    assert!(
        !column.contains("pub fn proof_column_commitment("),
        "legacy free-function proof_column_commitment should be removed"
    );
}

#[test]
fn ssmc_public_surface_does_not_expose_mutators() {
    let ssmc =
        fs::read_to_string(crate_root().join("src/schemes/ssmc/mod.rs")).expect("read ssmc mod");

    for forbidden in ["pub fn new(", "pub fn insert(", "pub fn remove("] {
        assert!(
            !ssmc.contains(forbidden),
            "SSMC mutator '{forbidden}' should not remain public"
        );
    }
}

#[test]
fn docs_match_current_architecture_language() {
    let readme = fs::read_to_string(crate_root().join("README.md")).expect("read README.md");
    let lib = fs::read_to_string(crate_root().join("src/lib.rs")).expect("read lib.rs");

    for stale in ["BabyBear", "`tabula-proof`"] {
        assert!(
            !readme.contains(stale),
            "README must not contain stale wording: {stale}"
        );
        assert!(
            !lib.contains(stale),
            "crate docs must not contain stale wording: {stale}"
        );
    }

    for required in [
        "KoalaBear",
        "tabula-runtime",
        "tabula-witness",
        "tabula-machine",
        "plain 2-to-1 Poseidon compression",
        "primitives",
        "schemes",
        "roots",
    ] {
        assert!(
            readme.contains(required) || lib.contains(required),
            "docs should describe the current proof-stack boundary using '{required}'"
        );
    }
}
