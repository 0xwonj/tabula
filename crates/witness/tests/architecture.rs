//! Architecture guardrails for the witness crate public surface.

use std::fs;
use std::path::Path;

fn crate_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn root_surface_does_not_reexport_old_witness_types() {
    let lib = fs::read_to_string(crate_root().join("src/lib.rs")).expect("read lib.rs");

    assert!(
        !lib.contains("pub mod legacy;"),
        "legacy compatibility namespace must not remain at crate root"
    );
    assert!(
        !lib.contains("pub mod witness;"),
        "legacy witness internals must not be a public root module"
    );
    assert!(
        !lib.contains("BatchWitness"),
        "legacy BatchWitness must not be re-exported from crate root"
    );
    assert!(
        !lib.contains("ColumnWitness"),
        "legacy ColumnWitness must not be re-exported from crate root"
    );
    assert!(
        !lib.contains("WitnessGenerator"),
        "legacy WitnessGenerator must not be re-exported from crate root"
    );
    for forbidden in [
        "SharedStoreBuilder",
        "SharedStoreContext",
        "ProgramInfo",
        "TemplateId",
        "LiteralCell",
        "proof_column_commitment",
    ] {
        assert!(
            !lib.contains(forbidden),
            "broad convenience re-export '{forbidden}' must not remain at crate root"
        );
    }
    assert!(
        lib.contains(
            "pub use prepare::{ExecutionInputPreparer, PreparedExecutionColumn, PreparedExecutionColumns};"
        ),
        "root must expose the minimal preparation seam"
    );
    for required in [
        "pub use types::{",
        "AccessEvent",
        "ColumnValueProfile",
        "ColumnWrite",
        "CommittedEntry",
        "InitCell",
        "PropertyReadClaim",
    ] {
        assert!(
            lib.contains(required),
            "root must expose logical preparation type surface '{required}'"
        );
    }
    assert!(
        lib.contains("pub mod stark;"),
        "witness crate must expose STARK-specific helpers under a namespaced module"
    );
}

#[test]
fn witness_manifest_contains_stark_dependencies_after_consolidation() {
    let manifest =
        fs::read_to_string(crate_root().join("Cargo.toml")).expect("read witness Cargo.toml");

    for required in [
        "tabula-commitment",
        "tabula-stark",
        "tabula-gadgets",
        "tabula-chips",
        "tabula-ir",
        "p3-air",
        "p3-koala-bear",
        "p3-field",
        "p3-matrix",
    ] {
        assert!(
            manifest.contains(required),
            "consolidated witness crate must depend on required backend crate '{required}'"
        );
    }
}

#[test]
fn stale_witness_stark_crate_is_removed() {
    assert!(
        !crate_root()
            .parent()
            .expect("witness under crates")
            .join("witness-stark")
            .exists(),
        "witness-stark crate directory must be removed after consolidation"
    );
}

#[test]
fn program_info_metadata_is_removed() {
    assert!(
        !crate_root().join("src/witness").exists(),
        "legacy witness metadata module must be removed from the consolidated witness crate"
    );
}

#[test]
fn stark_module_keeps_low_level_memory_helpers_internal() {
    let stark_mod =
        fs::read_to_string(crate_root().join("src/stark/mod.rs")).expect("read stark mod.rs");

    assert!(
        stark_mod.contains("pub mod schemes;"),
        "family-specific STARK witness helpers should be grouped under stark::schemes"
    );
    assert!(
        !stark_mod.contains("pub mod ssmc;"),
        "SSMC helpers should live under stark::schemes rather than the stark root"
    );
    assert!(
        !stark_mod.contains("pub mod smt_state;"),
        "SMT helpers should live under stark::schemes rather than the stark root"
    );
    assert!(
        !stark_mod.contains("pub mod memory;"),
        "memory row assembly helpers must stay internal to the STARK witness module"
    );
    assert!(
        !stark_mod.contains("pub use memory::{"),
        "low-level memory witness helpers must not be re-exported broadly"
    );
    for forbidden in [
        "pub use crate::CommittedEntry;",
        "pub use crate::PropertyReadClaim;",
    ] {
        assert!(
            !stark_mod.contains(forbidden),
            "logical proof-prep types must not be re-exported from the stark namespace: {forbidden}"
        );
    }
}

#[test]
fn trace_encoding_helpers_do_not_live_in_witness() {
    assert!(
        !crate_root().join("src/stark/encoding.rs").exists(),
        "trace/null encoding behavior should live in tabula-types, not witness"
    );

    let stark_mod =
        fs::read_to_string(crate_root().join("src/stark/mod.rs")).expect("read stark mod.rs");
    assert!(
        !stark_mod.contains("mod encoding;"),
        "witness stark module must not own a separate encoding behavior layer"
    );

    let lib = fs::read_to_string(crate_root().join("src/lib.rs")).expect("read lib.rs");
    for forbidden in ["encode_trace", "decode_trace", "trace_width"] {
        assert!(
            !lib.contains(forbidden),
            "trace encoding helpers must not leak into the witness root surface: {forbidden}"
        );
    }
}
