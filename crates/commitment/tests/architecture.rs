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
        lib.contains("pub use primitives::{FieldHasher, NativeDigest, PoseidonHasher};"),
        "commitment root should export only native hash/field primitives"
    );
    assert!(
        lib.contains("pub use binding::{ColumnRootBinding, NormalizedVerifierDigest};"),
        "commitment root should export only canonical root-binding contracts"
    );
    assert!(
        !lib.contains("compat"),
        "commitment root must not expose a compat namespace"
    );
}

#[test]
fn deleted_old_commitment_files_stay_deleted() {
    assert!(
        !crate_root().join("src/column.rs").exists(),
        "legacy column.rs surface should remain deleted"
    );
    assert!(
        !crate_root().join("src/compat.rs").exists(),
        "compat.rs should remain deleted"
    );
}

#[test]
fn binding_surface_stays_canonical_only() {
    let binding = fs::read_to_string(crate_root().join("src/binding.rs")).expect("read binding.rs");

    assert!(
        binding.contains("pub struct ColumnRootBinding"),
        "binding surface must define ColumnRootBinding"
    );
    assert!(
        binding.contains("pub struct NormalizedVerifierDigest"),
        "binding surface must define NormalizedVerifierDigest"
    );
    assert!(
        !binding.contains(&["Column", "State"].concat()),
        "binding surface must not mention removed state wrapper terminology"
    );
    assert!(
        !binding.contains(&["Column", "Meta"].concat()),
        "binding surface must not mention removed meta-leaf terminology"
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
