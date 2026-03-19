//! Architecture guardrails for the witness crate public surface.

use std::fs;
use std::path::Path;

fn crate_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn root_surface_does_not_reexport_legacy_witness_types() {
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
}

#[test]
fn builtin_memory_surface_exports_from_parts_only() {
    let builtin = fs::read_to_string(crate_root().join("src/trace/builtin.rs"))
        .expect("read trace/builtin.rs");

    assert!(
        !builtin.contains("pub mod legacy_memory"),
        "legacy memory helper namespace must be removed"
    );
    assert!(
        builtin.contains(
            "pub mod memory {\n    pub use super::memory_impl::{\n        prepare_memory_shard_rows_from_parts, prepare_meta_shard_row_from_parts,\n        prepare_ssmc_column_witness_from_parts,\n    };\n}"
        ),
        "builtin memory surface must expose only part-based helpers"
    );
}
