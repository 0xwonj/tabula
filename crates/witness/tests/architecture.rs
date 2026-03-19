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
    for forbidden in [
        "BuiltinTraceBuilder",
        "BuiltinTraceContext",
        "BuiltinWitnessInputs",
        "AllTraceInputs",
        "proof_column_commitment",
        "ExecutionInputPreparer",
    ] {
        assert!(
            !lib.contains(forbidden),
            "broad convenience re-export '{forbidden}' must not remain at crate root"
        );
    }
    assert!(
        lib.contains("pub use prepare::{BatchInputPreparer, PreparedExecutionInputs};"),
        "root must expose the minimal preparation seam"
    );
    assert!(
        lib.contains("pub use witness::{AccessRow, InitRow};"),
        "root must expose shared execution row types"
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
        builtin.contains("prepare_memory_shard_rows_from_parts"),
        "builtin memory surface must expose shared memory helpers"
    );
    assert!(
        builtin.contains("prepare_meta_shard_row_from_parts"),
        "builtin memory surface must expose meta-row helpers"
    );
    assert!(
        builtin.contains("prepare_ssmc_column_witness_from_parts"),
        "builtin memory surface must expose SSMC assembly from explicit parts"
    );
    assert!(
        builtin.contains("SsmcColumnWitnessParts"),
        "builtin memory surface must expose the explicit SSMC witness-parts bundle"
    );
}
