//! Architecture guardrails for proof-stack crate dependencies.

use std::fs;
use std::path::Path;
use std::process::Command;

use serde_json::Value;

fn workspace_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("runtime crate lives under workspace root")
        .parent()
        .expect("workspace root")
}

fn direct_normal_deps(metadata: &Value, package_name: &str) -> Vec<String> {
    metadata["packages"]
        .as_array()
        .expect("packages array")
        .iter()
        .find(|pkg| pkg["name"].as_str() == Some(package_name))
        .unwrap_or_else(|| panic!("package '{package_name}' missing from cargo metadata"))["dependencies"]
        .as_array()
        .expect("dependency array")
        .iter()
        .filter(|dep| dep["kind"].is_null())
        .map(|dep| {
            dep["name"]
                .as_str()
                .expect("dependency name")
                .to_string()
        })
        .collect()
}

fn assert_forbidden(metadata: &Value, package_name: &str, forbidden: &[&str]) {
    let deps = direct_normal_deps(metadata, package_name);
    for blocked in forbidden {
        assert!(
            !deps.iter().any(|dep| dep == blocked),
            "{package_name} must not depend on {blocked}: {deps:?}"
        );
    }
}

fn cargo_metadata() -> Value {
    let output = Command::new("cargo")
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .current_dir(workspace_root())
        .output()
        .expect("run cargo metadata");
    assert!(
        output.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    serde_json::from_slice(&output.stdout).expect("parse cargo metadata")
}

fn read_workspace_file(rel: &str) -> String {
    fs::read_to_string(workspace_root().join(rel)).expect("read workspace file")
}

#[test]
fn proof_crate_dependencies_respect_boundary_contract() {
    let metadata = cargo_metadata();

    assert_forbidden(
        &metadata,
        "tabula-stark",
        &[
            "tabula-gadgets",
            "tabula-chips",
            "tabula-witness",
            "tabula-machine",
            "tabula-runtime",
        ],
    );
    assert_forbidden(
        &metadata,
        "tabula-gadgets",
        &[
            "tabula-chips",
            "tabula-witness",
            "tabula-machine",
            "tabula-runtime",
        ],
    );
    assert_forbidden(
        &metadata,
        "tabula-chips",
        &["tabula-witness", "tabula-machine", "tabula-runtime"],
    );
    assert_forbidden(
        &metadata,
        "tabula-witness",
        &["tabula-machine", "tabula-runtime"],
    );
    assert_forbidden(
        &metadata,
        "tabula-machine",
        &["tabula-ir", "tabula-witness", "tabula-runtime"],
    );
}

#[test]
fn stark_root_does_not_export_public_gadgets_module() {
    let source = read_workspace_file("crates/stark/src/lib.rs");

    assert!(
        !source.contains("pub mod gadgets;"),
        "tabula-stark root must not re-export a public gadgets module"
    );
}

#[test]
fn shared_prove_path_does_not_depend_on_legacy_witness_or_layout_dispatch() {
    let prepare_rs = read_workspace_file("crates/runtime/src/proving/prepare.rs");
    let traces_rs = read_workspace_file("crates/runtime/src/proving/traces.rs");
    let materialize_rs = read_workspace_file("crates/runtime/src/assembly/materialize.rs");
    let runtime_program_rs = read_workspace_file("crates/runtime/src/program/runtime_program.rs");

    for (name, source) in [
        ("proving/prepare.rs", prepare_rs.as_str()),
        ("proving/traces.rs", traces_rs.as_str()),
        ("assembly/materialize.rs", materialize_rs.as_str()),
        ("program/runtime_program.rs", runtime_program_rs.as_str()),
    ] {
        assert!(
            !source.contains("tabula_witness::legacy::ColumnWitness")
                && !source.contains("use tabula_witness::legacy::ColumnWitness")
                && !source.contains("pub type ColumnWitness"),
            "{name} must not depend on legacy ColumnWitness"
        );
        assert!(
            !source.contains("tabula_witness::legacy::BatchWitness")
                && !source.contains("use tabula_witness::legacy::BatchWitness")
                && !source.contains("pub type BatchWitness"),
            "{name} must not depend on legacy BatchWitness"
        );
        assert!(
            !source.contains("ProofInputBuilder"),
            "{name} must not reference removed ProofInputBuilder"
        );
        assert!(
            !source.contains("ColumnStateBackend"),
            "{name} must not reference removed ColumnStateBackend"
        );
        assert!(
            !source.contains("PlanColumnStateBackend"),
            "{name} must not reference removed plan-based backend"
        );
        assert!(
            !source.contains("layout_kind"),
            "{name} must not dispatch on layout_kind in the shared prove path"
        );
    }
}

#[test]
fn witness_root_surface_stays_minimal_and_namespaced() {
    let lib_rs = read_workspace_file("crates/witness/src/lib.rs");

    for forbidden in [
        "pub use trace::builtin::{",
        "BuiltinTraceBuilder",
        "BuiltinTraceContext",
        "BuiltinWitnessInputs",
        "AllTraceInputs",
        "proof_column_commitment",
        "ExecutionInputPreparer",
    ] {
        assert!(
            !lib_rs.contains(forbidden),
            "witness root must not expose broad convenience re-export '{forbidden}'"
        );
    }
    assert!(
        !lib_rs.contains("pub use witness::{AccessRow, InitRow,"),
        "witness root must not re-export additional witness internals"
    );

    assert!(
        lib_rs.contains("pub use prepare::{BatchInputPreparer, PreparedExecutionInputs};"),
        "witness root must expose the minimal preparation seam"
    );
    assert!(
        lib_rs.contains("pub use witness::{AccessRow, InitRow};"),
        "witness root must expose shared execution row types"
    );
}
