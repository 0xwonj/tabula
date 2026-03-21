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
        .unwrap_or_else(|| panic!("package '{package_name}' missing from cargo metadata"))
        ["dependencies"]
        .as_array()
        .expect("dependency array")
        .iter()
        .filter(|dep| dep["kind"].is_null())
        .map(|dep| dep["name"].as_str().expect("dependency name").to_string())
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
    assert_forbidden(&metadata, "tabula-runtime", &["tabula-witness-stark"]);
    assert_forbidden(&metadata, "tabula-testing", &["tabula-witness-stark"]);
    assert_forbidden(
        &metadata,
        "tabula-machine",
        &["tabula-ir", "tabula-witness", "tabula-runtime"],
    );
}

#[test]
fn witness_stark_package_is_removed_from_workspace() {
    let metadata = cargo_metadata();
    let packages = metadata["packages"].as_array().expect("packages array");
    assert!(
        !packages
            .iter()
            .any(|pkg| pkg["name"].as_str() == Some("tabula-witness-stark")),
        "tabula-witness-stark package must be removed from the workspace"
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
    let materialize_rs = read_workspace_file("crates/runtime/src/setup/materialize.rs");
    let runtime_program_rs = read_workspace_file("crates/runtime/src/program/resolved_program.rs");

    for (name, source) in [
        ("proving/prepare.rs", prepare_rs.as_str()),
        ("proving/traces.rs", traces_rs.as_str()),
        ("setup/materialize.rs", materialize_rs.as_str()),
        ("program/resolved_program.rs", runtime_program_rs.as_str()),
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
            !source.contains("ColumnTransitionBackend")
                && !source.contains("ColumnTransitionInput")
                && !source.contains("struct ColumnProofInput")
                && !source.contains("type ColumnProofInput")
                && !source.contains("BatchProofInput"),
            "{name} must not reference removed transition-backend types"
        );
        assert!(
            !source.contains("prepare_witness_artifacts"),
            "{name} must not reference removed witness-artifact helper"
        );
        assert!(
            !source.contains("layout_kind"),
            "{name} must not dispatch on layout_kind in the shared prove path"
        );
    }
}

#[test]
fn runtime_root_does_not_own_extension_authoring_contracts() {
    let lib_rs = read_workspace_file("crates/runtime/src/lib.rs");

    for forbidden in [
        "BatchProofInput",
        "ColumnProofInput",
        "ColumnTransitionBackend",
        "ColumnTransitionInput",
        "ColumnArtifactBuilder",
        "ColumnArtifactInput",
        "ColumnArtifactPayload",
        "ColumnProofArtifact",
        "ColumnTraceInputs",
        "PropertyReadClaim",
        "WitnessStore",
    ] {
        assert!(
            !lib_rs.contains(forbidden),
            "runtime root must not expose removed prove-side symbol '{forbidden}'"
        );
    }

    assert!(
        !lib_rs.contains("pub mod proof_extensions;")
            && !lib_rs.contains("pub mod precompile_proofs;"),
        "runtime root must not expose authoring modules as public surface"
    );
    assert!(
        !lib_rs.contains("pub use proof_extensions::")
            && !lib_rs.contains("pub use precompile_proofs::")
            && !lib_rs.contains("pub use columns::{ColumnSchemeFactory")
            && !lib_rs.contains("pub use columns::{ResolvedColumnPlan")
            && !lib_rs.contains("pub use columns::{RuntimeColumn"),
        "runtime root must not own extension authoring contracts"
    );
}

#[test]
fn raw_backend_extension_surface_stays_backend_only() {
    let machine_lib = read_workspace_file("crates/machine/src/lib.rs");
    let machine_backend_extension = read_workspace_file("crates/machine/src/backend/extension.rs");
    let machine_backend_mod = read_workspace_file("crates/machine/src/backend/mod.rs");
    let machine_backend_prelude = read_workspace_file("crates/machine/src/backend/prelude.rs");
    let runtime_builder = read_workspace_file("crates/runtime/src/builder.rs");
    let runtime_verifier = read_workspace_file("crates/runtime/src/verifier.rs");
    let sdk_ext_mod = read_workspace_file("crates/sdk/src/ext/mod.rs");
    let ext_lib = read_workspace_file("crates/ext/src/lib.rs");
    let ext_backend = read_workspace_file("crates/ext/src/backend/mod.rs");

    assert!(
        machine_lib.contains("pub mod backend;"),
        "machine must expose backend APIs only under the explicit backend module"
    );
    assert!(
        !machine_lib.contains("backend_api"),
        "machine must not keep the removed backend_api shim"
    );
    assert!(
        !machine_lib.contains("pub mod prelude;")
            && !machine_lib.contains("pub use backend::AnyRap;")
            && !machine_lib.contains("pub use backend::{AnyRap")
            && !machine_lib.contains("pub use columns::{ColumnChipSet, ProofColumn};"),
        "machine root must not expose backend authoring types"
    );
    assert!(
        machine_backend_mod.contains("pub mod prelude;"),
        "machine backend-only path must own the chip-authoring prelude"
    );
    assert!(
        !machine_backend_prelude.contains("ExecutionTierExtension")
            && !machine_backend_prelude.contains("ExtensionContext"),
        "machine backend prelude must not expose generic extension seam types"
    );
    assert!(
        !runtime_builder.contains("pub fn with_extension(")
            && !runtime_builder.contains("pub fn with_execution_extension("),
        "runtime builder must not expose raw machine extensions"
    );
    assert!(
        !runtime_verifier.contains("pub fn with_extension(")
            && !runtime_verifier.contains("pub fn with_execution_extension("),
        "runtime verifier builder must not expose raw machine extensions"
    );
    for forbidden in [
        "AnyRap",
        "DynChip",
        "BusConsumer",
        "WitnessStore",
        "ProofColumn",
    ] {
        assert!(
            !sdk_ext_mod.contains(forbidden),
            "sdk extension surface must not re-export backend symbol '{forbidden}'"
        );
    }
    for forbidden in [
        "pub use backend::AnyRap",
        "pub use backend::{AnyRap",
        "pub use backend::DynChip",
        "pub use backend::ProofColumn",
        "pub use precompile::{PrecompileProofContext",
        "pub use precompile::{PrecompileProofFactory",
        "pub use scheme::{ColumnProofContext",
        "pub use scheme::{ProofSchemeFactory",
    ] {
        assert!(
            !ext_lib.contains(forbidden),
            "tabula-ext root must not flatten backend symbol '{forbidden}'"
        );
    }
    for required in [
        "pub use tabula_machine::backend::{AnyRap, ColumnChipSet, ProofColumn};",
        "pub use tabula_stark::trace::{BusConsumer, DynChip, WitnessStore};",
        "pub mod prelude {",
    ] {
        assert!(
            ext_backend.contains(required),
            "tabula-ext::backend must expose curated backend support '{required}'"
        );
    }
    assert!(
        machine_backend_extension.contains("pub trait ExecutionTierExtension"),
        "backend extension module must expose the renamed execution-tier trait"
    );
    assert!(
        !machine_backend_extension.contains("ExtensionContext"),
        "legacy empty ExtensionContext must be deleted"
    );
    assert!(
        !read_workspace_file("crates/machine/src/machine.rs")
            .contains(".with_execution_extension("),
        "machine docs must not advertise generic execution-extension registration as the main model"
    );
    assert!(
        !machine_backend_prelude.contains("extension authors"),
        "machine backend prelude docs must not present generic extension authoring as the primary public model"
    );
}

#[test]
fn precompile_redesign_removes_legacy_id_only_and_digest_slot_paths() {
    for rel in [
        "crates/compiler/src/register.rs",
        "crates/compiler/src/program.rs",
        "crates/artifact/src/program.rs",
        "crates/runtime/src/proving/prepare.rs",
        "crates/witness/src/stark/lowering/precompile.rs",
        "crates/chips/src/execution/buses.rs",
        "crates/chips/src/execution/ops/precompile.rs",
    ] {
        let source = read_workspace_file(rel);
        for forbidden in [
            "required_precompile_ids",
            "PrecompileIo",
            "precompile_ios",
            "digest-as-slot",
        ] {
            assert!(
                !source.contains(forbidden),
                "{rel} must not reference removed precompile legacy symbol '{forbidden}'"
            );
        }
    }

    let execution_buses = read_workspace_file("crates/chips/src/execution/buses.rs");
    assert!(
        !execution_buses.contains("local.op_hash.into() + local.op_precompile.into()"),
        "execution bus wiring must not treat precompiles as hash-permutation rows"
    );
}

#[test]
fn stable_scheme_surface_fixture_uses_bundle_only_public_registration() {
    let stable_fixture = read_workspace_file("crates/runtime/tests/stable_scheme_surface.rs");

    assert!(
        stable_fixture.contains(".with_scheme_bundle("),
        "stable scheme surface fixture must register custom schemes through bundles"
    );
    assert!(
        !stable_fixture.contains(".with_scheme(")
            && !stable_fixture.contains(".with_proof_scheme("),
        "stable scheme surface fixture must not use removed split registration APIs"
    );
}

#[test]
fn canonical_renamed_phase_objects_do_not_regress() {
    for rel in [
        "crates/witness/src/lib.rs",
        "crates/witness/src/prepare.rs",
        "crates/runtime/src/setup/materialize.rs",
        "crates/runtime/src/runtime.rs",
        "crates/runtime/src/proving/prepare.rs",
        "crates/runtime/src/proving/traces.rs",
        "crates/runtime/src/builder.rs",
        "crates/testing/src/witness.rs",
    ] {
        let source = read_workspace_file(rel);
        for forbidden in [
            "PreparedExecutionInputs",
            "ResolvedProofSlot",
            "struct ProofSlot",
            "type ProofSlot",
            "struct PreparedColumnTier ",
            "Vec<PreparedColumnTier>",
            "PreparedColumnTier {",
            "resolve_proof_columns_with_factories",
            ".proof_slots(",
            " proof_slots:",
        ] {
            assert!(
                !source.contains(forbidden),
                "{rel} must not reintroduce stale canonical name '{forbidden}'"
            );
        }
    }
}

#[test]
fn renamed_internal_symbols_do_not_reappear() {
    for rel in [
        "crates/core/src/traits/state.rs",
        "crates/runtime/src/program/resolved_program.rs",
        "crates/runtime/src/columns/resolved_plan.rs",
        "crates/witness/src/prepare.rs",
        "crates/runtime/src/setup/materialize.rs",
        "crates/runtime/src/proving/prepare.rs",
    ] {
        let source = read_workspace_file(rel);
        for forbidden in [
            "trait StateSnapshot",
            "struct RuntimeProgram",
            "pub struct RuntimeProgram",
            "struct ColumnPlan",
            "pub struct ColumnPlan",
            "struct BatchInputPreparer",
            "pub struct BatchInputPreparer",
            "struct RuntimeProofSlot",
            "pub(crate) struct RuntimeProofSlot",
            "struct MaterializedProofSlot",
            "pub(crate) struct MaterializedProofSlot",
            "struct PreparedColumnTierInput",
            "pub(crate) struct PreparedColumnTierInput",
            "struct PlannedColumnProof {",
            "type PlannedColumnProof =",
        ] {
            assert!(
                !source.contains(forbidden),
                "{rel} must not reintroduce renamed internal symbol '{forbidden}'"
            );
        }
    }
}

#[test]
fn root_crates_do_not_export_removed_compat_aliases() {
    let artifact_lib = read_workspace_file("crates/artifact/src/lib.rs");
    let runtime_lib = read_workspace_file("crates/runtime/src/lib.rs");

    for forbidden in [
        "pub type ProgramArtifact",
        "pub type StateSnapshot",
        "pub type ExecutionStatement",
    ] {
        assert!(
            !artifact_lib.contains(forbidden),
            "artifact root must not export removed alias '{forbidden}'"
        );
    }

    for forbidden in [
        "pub type ProgramBinding",
        "pub type RuntimeProgram",
        "pub type ProgramVerifier",
        "pub type ProgramVerifierBuilder",
    ] {
        assert!(
            !runtime_lib.contains(forbidden),
            "runtime root must not export removed alias '{forbidden}'"
        );
    }
}

#[test]
fn program_binding_is_canonicalized_in_contract_crate() {
    let contract_binding = read_workspace_file("crates/contract/src/binding.rs");
    let runtime_binding = read_workspace_file("crates/runtime/src/program/binding.rs");
    let ext_precompile = read_workspace_file("crates/ext/src/precompile.rs");
    let ext_backend_precompile = read_workspace_file("crates/ext/src/backend/precompile.rs");

    assert!(
        contract_binding.contains("pub struct ProgramBinding"),
        "contract crate must own the canonical ProgramBinding type"
    );
    assert!(
        runtime_binding.contains("pub use tabula_contract::ProgramBinding as Binding;"),
        "runtime binding module must be a thin re-export over the contract-owned ProgramBinding"
    );
    assert!(
        !ext_precompile.contains("struct ProgramBinding"),
        "tabula-ext root precompile module must not own ProgramBinding"
    );
    assert!(
        ext_backend_precompile.contains("pub use tabula_contract::ProgramBinding;"),
        "backend precompile authoring surface must re-export the contract-owned ProgramBinding"
    );
}

#[test]
fn workspace_uses_tabula_ext_and_not_backend_ext() {
    let metadata = cargo_metadata();
    let packages = metadata["packages"].as_array().expect("packages array");
    assert!(
        packages
            .iter()
            .any(|pkg| pkg["name"].as_str() == Some("tabula-ext")),
        "tabula-ext must exist as the canonical extension authoring crate"
    );
    assert!(
        !packages
            .iter()
            .any(|pkg| pkg["name"].as_str() == Some("tabula-backend-ext")),
        "tabula-backend-ext must be removed from the workspace"
    );
}

#[test]
fn runtime_code_avoids_stale_artifact_builder_and_replacement_wording() {
    for rel in [
        "crates/runtime/src/builder.rs",
        "crates/runtime/src/testing/prove.rs",
    ] {
        let source = read_workspace_file(rel);
        for forbidden in ["artifact builder", "artifact-builder", "replacement"] {
            assert!(
                !source.contains(forbidden),
                "{rel} must not contain stale wording '{forbidden}'"
            );
        }
    }
}

#[test]
fn witness_root_surface_stays_minimal_and_namespaced() {
    let lib_rs = read_workspace_file("crates/witness/src/lib.rs");

    for forbidden in [
        "pub use stark::{",
        "SharedStoreBuilder",
        "SharedStoreContext",
        "ProgramInfo",
        "TemplateId",
        "LiteralCell",
        "proof_column_commitment",
    ] {
        assert!(
            !lib_rs.contains(forbidden),
            "witness root must not expose broad convenience re-export '{forbidden}'"
        );
    }

    assert!(
        lib_rs.contains(
            "pub use prepare::{ExecutionInputPreparer, PreparedExecutionColumn, PreparedExecutionColumns};"
        ),
        "witness root must expose the minimal preparation seam"
    );
    assert!(
        lib_rs.contains(
            "pub use types::{AccessEvent, ColumnWrite, CommittedEntry, InitCell, PropertyReadClaim};"
        ),
        "witness root must expose logical preparation row types"
    );
    assert!(
        lib_rs.contains("pub mod stark;"),
        "witness crate must expose its STARK-specific helpers under a namespaced module"
    );
}

#[test]
fn runtime_builtin_schemes_do_not_own_low_level_stark_witness_assembly() {
    for rel in [
        "crates/runtime/src/columns/builtins/ssmc.rs",
        "crates/runtime/src/columns/builtins/smt.rs",
    ] {
        let source = read_workspace_file(rel);
        for forbidden in [
            "fn encode_committed_entries(",
            "fn encode_property_record(",
            "fn synthesize_old_init_cells(",
            "fn ssmc_entries(",
            "fn build_smt_state_witness(",
            "fn encode_array(",
            "fn validate_leaf_match(",
            "fn path_bits_from_key(",
            "prepare_ssmc_column_witness_from_parts",
            "prepare_memory_shard_rows_from_parts",
            "prepare_meta_shard_row_from_parts",
            "SsmcColumnWitnessParts",
            "SmtStateWitnessParts",
        ] {
            assert!(
                !source.contains(forbidden),
                "{rel} must not own low-level STARK witness helper '{forbidden}'"
            );
        }
    }
}

#[test]
fn witness_stark_namespace_does_not_reexport_logical_prep_types() {
    let stark_mod = read_workspace_file("crates/witness/src/stark/mod.rs");
    for forbidden in [
        "pub use crate::CommittedEntry;",
        "pub use crate::PropertyReadClaim;",
    ] {
        assert!(
            !stark_mod.contains(forbidden),
            "witness::stark must not re-export logical prep type '{forbidden}'"
        );
    }
}

#[test]
fn materialize_helpers_consume_prederived_column_plans() {
    let materialize_rs = read_workspace_file("crates/runtime/src/setup/materialize.rs");
    assert!(
        !materialize_rs.contains("derive_column_plans("),
        "materialization helpers must consume prederived column plans rather than deriving them internally"
    );
}

#[test]
fn stark_trace_runtime_stays_generic() {
    for rel in [
        "crates/stark/src/trace/mod.rs",
        "crates/stark/src/trace/orchestration.rs",
        "crates/stark/src/trace/validation.rs",
    ] {
        let source = read_workspace_file(rel);
        for forbidden in [
            "tabula_chips::shards",
            "SsmcWitness",
            "SmtStateWitness",
            "PropertyReadRecord",
            "MemoryShardRow",
            "MetaShardRow",
            "lower_program_batch",
            "SharedStoreBuilder",
        ] {
            assert!(
                !source.contains(forbidden),
                "{rel} must not absorb builtin-specific STARK witness logic '{forbidden}'"
            );
        }
    }
}
