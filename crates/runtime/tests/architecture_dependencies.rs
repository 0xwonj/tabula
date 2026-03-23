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

fn read_workspace_files(rels: &[&str]) -> String {
    rels.iter()
        .map(|rel| read_workspace_file(rel))
        .collect::<Vec<_>>()
        .join("\n")
}

fn rust_sources_under(rel: &str) -> Vec<String> {
    fn walk(dir: &Path, files: &mut Vec<String>) {
        for entry in fs::read_dir(dir).expect("read dir") {
            let entry = entry.expect("dir entry");
            let path = entry.path();
            if path.is_dir() {
                walk(&path, files);
            } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
                files.push(path.to_string_lossy().into_owned());
            }
        }
    }

    let mut files = Vec::new();
    walk(&workspace_root().join(rel), &mut files);
    files.sort();
    files
}

fn assert_source_omits(rel: &str, forbidden: &[&str]) {
    let source = read_workspace_file(rel);
    for needle in forbidden {
        assert!(
            !source.contains(needle),
            "{rel} must not contain legacy execution-carrier pattern '{needle}'"
        );
    }
}

fn joined(parts: &[&str]) -> String {
    parts.concat()
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
    assert_forbidden(&metadata, "tabula-profile", &["tabula-artifact"]);
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
fn shared_prove_path_does_not_depend_on_old_witness_or_layout_dispatch() {
    let proving_journal_rs = read_workspace_files(&[
        "crates/runtime/src/proving/journal/mod.rs",
        "crates/runtime/src/proving/journal/types.rs",
        "crates/runtime/src/proving/journal/state.rs",
        "crates/runtime/src/proving/journal/tx.rs",
        "crates/runtime/src/proving/journal/reduce.rs",
        "crates/runtime/src/proving/journal/digest.rs",
    ]);
    let proving_artifacts_rs = read_workspace_file("crates/runtime/src/proving/artifacts.rs");
    let traces_rs = read_workspace_file("crates/runtime/src/proving/traces.rs");
    let materialize_rs = read_workspace_file("crates/runtime/src/bootstrap/materialize.rs");
    let runtime_program_rs = read_workspace_file("crates/runtime/src/program/contract.rs");

    for (name, source) in [
        ("proving/journal/", proving_journal_rs.as_str()),
        ("proving/artifacts.rs", proving_artifacts_rs.as_str()),
        ("proving/traces.rs", traces_rs.as_str()),
        ("bootstrap/materialize.rs", materialize_rs.as_str()),
        ("program/contract.rs", runtime_program_rs.as_str()),
    ] {
        assert!(
            !source.contains("tabula_witness::legacy::ColumnWitness")
                && !source.contains("use tabula_witness::legacy::ColumnWitness")
                && !source.contains("pub type ColumnWitness"),
            "{name} must not depend on removed witness column shape"
        );
        assert!(
            !source.contains("tabula_witness::legacy::BatchWitness")
                && !source.contains("use tabula_witness::legacy::BatchWitness")
                && !source.contains("pub type BatchWitness"),
            "{name} must not depend on removed witness batch shape"
        );
        assert!(
            !source.contains("ProofInputBuilder"),
            "{name} must not reference removed ProofInputBuilder"
        );
        assert!(
            !source.contains(&joined(&["Column", "State", "Backend"])),
            "{name} must not reference removed plan-backed commitment backend"
        );
        assert!(
            !source.contains(&joined(&["Plan", "Column", "State", "Backend"])),
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
            && !lib_rs.contains("pub use schemes::{ColumnSchemeFactory")
            && !lib_rs.contains("pub use schemes::{ResolvedColumnPlan")
            && !lib_rs.contains("pub use schemes::{RuntimeColumn"),
        "runtime root must not own extension authoring contracts"
    );
}

#[test]
fn execution_carrier_cutover_keeps_migrated_paths_old_carrier_free() {
    let migrated_surface = [
        "crates/core/src/tx.rs",
        "crates/core/src/event.rs",
        "crates/core/src/traits/state.rs",
        "crates/core/src/state/in_memory.rs",
        "crates/artifact/src/batch.rs",
        "crates/artifact/src/state.rs",
        "crates/executor/src/batch.rs",
        "crates/executor/src/execution_state.rs",
        "crates/executor/src/interpreter.rs",
        "crates/executor/src/precompile.rs",
        "crates/executor/src/property.rs",
        "crates/executor/src/resolve.rs",
        "crates/runtime/src/execute/snapshot_view.rs",
        "crates/ext/src/scheme.rs",
        "crates/ir/src/instruction.rs",
    ];

    for rel in migrated_surface {
        assert_source_omits(
            rel,
            &[
                "Vec<Value>",
                "Option<Value>",
                "&[Value]",
                "Result<Value",
                "ValueType",
                &joined(&["zero", "_typed("]),
                &joined(&["lega", "cy_", "runtime_value_type("]),
            ],
        );
    }
}

#[test]
fn phase_two_proof_paths_stay_old_carrier_free() {
    assert!(
        !workspace_root()
            .join("crates/runtime/src/proving/legacy.rs")
            .exists(),
        "runtime proving legacy helper module must be deleted"
    );
    assert!(
        !workspace_root()
            .join("crates/witness/src/legacy.rs")
            .exists(),
        "witness legacy helper module must be deleted"
    );

    let production_paths = [
        "crates/runtime/src/proving",
        "crates/runtime/src/host",
        "crates/witness/src",
    ];
    let forbidden = [
        joined(&["use tabula_core::state::", "va", "lue::", "Va", "lue"]),
        joined(&["use tabula_core::state::", "va", "lue::{", "Va", "lue"]),
        joined(&["use tabula_core::state::", "va", "lue::", "Value", "Type"]),
        joined(&["use tabula_core::state::", "va", "lue::{", "Value", "Type"]),
        joined(&["use tabula_core::state::", "va", "lue::", "zero", "_typed"]),
        joined(&["use tabula_core::state::", "va", "lue::{", "zero", "_typed"]),
        joined(&["use tabula_core::", "Va", "lue"]),
        joined(&["use tabula_core::", "Value", "Type"]),
        joined(&["lega", "cy_", "runtime_value_type("]),
        joined(&["mod ", "lega", "cy;"]),
        joined(&["lega", "cy::"]),
    ];

    for rel_dir in production_paths {
        for path in rust_sources_under(rel_dir) {
            let source = fs::read_to_string(&path).expect("read phase two source");
            for needle in &forbidden {
                assert!(
                    !source.contains(needle),
                    "{path} must not contain phase-two legacy proof-path pattern '{needle}'",
                );
            }
        }
    }
}

#[test]
fn phase_one_contract_freeze_keeps_root_surfaces_quarantined() {
    let core_lib = read_workspace_file("crates/core/src/lib.rs");
    let types_lib = read_workspace_file("crates/types/src/lib.rs");
    let sdk_rs = read_workspace_file("crates/sdk/src/sdk.rs");

    assert!(
        core_lib.contains("pub use state::portable::PortableValue;")
            && !core_lib.contains("pub use state::value::{PortableValue, Value")
            && !core_lib.contains("pub use state::value::{PortableValue, ValueType")
            && !core_lib
                .contains("pub use state::value::{PortableValue, Value, ValueType, zero_typed}")
            && !core_lib.contains("pub use state::value::{Value")
            && !core_lib.contains("pub use state::value::{ValueType")
            && !core_lib.contains("pub use state::value::{zero_typed"),
        "tabula-core root must not re-export legacy value carriers"
    );

    let root_forbidden = [
        "PortableValueExt".to_string(),
        joined(&["lega", "cy_", "value_from_typed"]),
        joined(&["lega", "cy_", "value_from_portable"]),
        joined(&["portable_from_", "lega", "cy_", "value"]),
        joined(&["typed_from_", "lega", "cy_", "value"]),
        joined(&["builtin_type_id_for_", "lega", "cy_", "value_type"]),
        joined(&["builtin_encode_field_elements_for_", "lega", "cy_", "value"]),
        joined(&["builtin_decode_field_elements_to_", "lega", "cy_", "value"]),
    ];
    for forbidden in &root_forbidden {
        assert!(
            !types_lib.contains(forbidden),
            "tabula-types root must not export legacy helper '{forbidden}'"
        );
    }

    assert!(
        sdk_rs.contains("host_environment: HostEnvironment"),
        "SdkBuilder must keep HostEnvironment as its bootstrap owner"
    );
    assert!(
        !sdk_rs.contains("type_runtimes: TypeRuntimeRegistry")
            && !sdk_rs.contains("encoding_runtimes: EncodingRuntimeRegistry"),
        "SdkBuilder must not keep parallel runtime registry ownership"
    );
}

#[test]
fn phase_one_non_proof_paths_do_not_import_old_helper_surface() {
    let frozen_paths = [
        "crates/compiler/src/example.rs",
        "crates/core/src/lib.rs",
        "crates/executor/src/batch.rs",
        "crates/executor/src/execution_state.rs",
        "crates/executor/src/interpreter.rs",
        "crates/executor/src/precompile.rs",
        "crates/executor/src/property.rs",
        "crates/executor/src/resolve.rs",
        "crates/runtime/src/host/environment.rs",
        "crates/runtime/src/host/installed.rs",
        "crates/runtime/src/host/registries.rs",
        "crates/runtime/src/bootstrap/builder.rs",
        "crates/runtime/src/verifier.rs",
        "crates/sdk/src/sdk.rs",
        "crates/types/src/lib.rs",
    ];

    let frozen_needles = [
        joined(&["portable_from_", "lega", "cy_", "value"]),
        joined(&["typed_from_", "lega", "cy_", "value"]),
        joined(&["lega", "cy_", "value_from_typed"]),
        joined(&["lega", "cy_", "value_from_portable"]),
        joined(&["builtin_type_id_for_", "lega", "cy_", "value_type"]),
        "PortableValueExt".to_string(),
    ];
    for rel in &frozen_paths {
        let source = read_workspace_file(rel);
        for needle in &frozen_needles {
            assert!(
                !source.contains(needle),
                "{rel} must not contain removed helper surface '{needle}'"
            );
        }
    }
}

#[test]
fn phase_three_precompile_contract_is_explicit_and_signature_driven() {
    let artifact_program = read_workspace_file("crates/artifact/src/program.rs");
    let compiler_register = read_workspace_file("crates/compiler/src/register.rs");
    let ext_backend_precompile = read_workspace_file("crates/ext/src/backend/precompile.rs");
    let lang_lower_expr = read_workspace_file("crates/lang/src/lower/expr.rs");
    let lang_lower_mod = read_workspace_file("crates/lang/src/lower/mod.rs");
    let lang_lower_stmt = read_workspace_file("crates/lang/src/lower/stmt.rs");
    let runtime_journal = read_workspace_files(&[
        "crates/runtime/src/proving/journal/mod.rs",
        "crates/runtime/src/proving/journal/types.rs",
        "crates/runtime/src/proving/journal/state.rs",
        "crates/runtime/src/proving/journal/tx.rs",
        "crates/runtime/src/proving/journal/reduce.rs",
    ]);
    let runtime_artifacts = read_workspace_file("crates/runtime/src/proving/artifacts.rs");
    let witness_lower_precompile =
        read_workspace_file("crates/witness/src/stark/lowering/precompile.rs");
    let transcript_chip = read_workspace_file("crates/chips/src/precompile_transcript.rs");
    let executor_interpreter = read_workspace_file("crates/executor/src/interpreter.rs");

    assert!(
        artifact_program.contains("pub signature: PrecompileSignature")
            && !artifact_program.contains("params_hash"),
        "sealed artifact precompile descriptors must carry explicit signatures and no params_hash",
    );
    assert!(
        ext_backend_precompile.contains("if descriptor != self.descriptor()"),
        "precompile backend validation must default to exact descriptor equality",
    );
    assert!(
        compiler_register.contains("validate_precompile_descriptor(")
            && compiler_register.contains("GENERIC_EXECUTION_VALUE_WIDTH"),
        "compiler precompile descriptor registration must validate encoding/type compatibility and execution width",
    );
    for (name, source) in [
        ("runtime proving journal", runtime_journal.as_str()),
        ("runtime proof artifacts", runtime_artifacts.as_str()),
        (
            "witness precompile lowering",
            witness_lower_precompile.as_str(),
        ),
        ("precompile transcript chip", transcript_chip.as_str()),
    ] {
        for forbidden in [
            "builtin_encoding_profile_id_for_type",
            "builtin_encode_field_elements_for_portable",
        ] {
            assert!(
                !source.contains(forbidden),
                "{name} must not use built-in-only precompile transcript helpers ({forbidden})",
            );
        }
    }
    for source in [&lang_lower_mod, &lang_lower_expr, &lang_lower_stmt] {
        assert!(
            !source.contains(&joined(&["builtin_", "zero", "_typed("])),
            "source lowering must not use built-in-only zero synthesis",
        );
    }
    for forbidden in [
        "KoalaBear::new(profile.type_id.0)",
        "KoalaBear::new(profile.encoding_profile_id.0)",
        "payload.push(KoalaBear::new(u32::try_from(atoms.len())",
    ] {
        assert!(
            !transcript_chip.contains(forbidden),
            "precompile transcript prefixes must encode ids and atom counts bytewise LE32 ({forbidden})",
        );
    }
    assert!(
        executor_interpreter.contains("let signature = handler.signature();"),
        "executor precompile dispatch must validate against handler signatures",
    );
}

#[test]
fn phase_three_production_code_does_not_use_label_only_precompile_descriptors() {
    for rel in [
        "crates/artifact/src/program.rs",
        "crates/ext/src/backend/precompile.rs",
        "crates/runtime/src/bootstrap/materialize.rs",
        "crates/runtime/src/proving/journal/mod.rs",
        "crates/runtime/src/proving/journal/types.rs",
        "crates/runtime/src/proving/journal/state.rs",
        "crates/runtime/src/proving/journal/tx.rs",
        "crates/runtime/src/proving/journal/reduce.rs",
        "crates/runtime/src/proving/artifacts.rs",
        "crates/witness/src/stark/lowering/precompile.rs",
        "crates/sdk/src/sdk.rs",
    ] {
        assert_source_omits(rel, &["PrecompileDescriptor::from_labels("]);
    }
}

#[test]
fn phase_four_deleted_files_and_modules_stay_deleted() {
    for rel in [
        "crates/core/src/state/value.rs",
        "crates/testing/src/legacy.rs",
    ] {
        assert!(
            !workspace_root().join(rel).exists(),
            "{rel} must remain deleted after phase-four cleanup",
        );
    }

    let compat_dirs: Vec<_> = fs::read_dir(workspace_root().join("crates/commitment/src"))
        .expect("read commitment src")
        .filter_map(|entry| {
            let entry = entry.expect("dir entry");
            let path = entry.path();
            path.is_dir().then_some(
                path.file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or_default()
                    .to_string(),
            )
        })
        .filter(|name| name == "compat")
        .collect();
    assert!(
        compat_dirs.is_empty(),
        "commitment compat module must stay deleted: {compat_dirs:?}"
    );
}

#[test]
fn phase_four_workspace_source_denylist_stays_clean() {
    let forbidden = [
        joined(&["tabula_core::state::", "va", "lue::", "Va", "lue"]),
        joined(&["tabula_core::state::", "va", "lue::", "Value", "Type"]),
        joined(&["tabula_core::", "Va", "lue"]),
        joined(&["tabula_core::", "Value", "Type"]),
        joined(&["lega", "cy_"]),
        joined(&["commitment::", "compat"]),
        joined(&["Column", "Meta"]),
        joined(&["Column", "State"]),
        joined(&["compute_", "lega", "cy_", "column_meta_binding_digest"]),
        joined(&["compute_state_roots_from_", "metas"]),
        joined(&["Unsupported", "Legacy", "Type"]),
    ];

    for path in rust_sources_under("crates") {
        let source = fs::read_to_string(&path).expect("read rust source");
        for needle in &forbidden {
            assert!(
                !source.contains(needle),
                "{path} must not contain final deleted legacy symbol '{needle}'",
            );
        }
    }
}

#[test]
fn raw_backend_extension_surface_stays_backend_only() {
    let machine_lib = read_workspace_file("crates/machine/src/lib.rs");
    let machine_backend_extension = read_workspace_file("crates/machine/src/backend/extension.rs");
    let machine_backend_mod = read_workspace_file("crates/machine/src/backend/mod.rs");
    let machine_backend_prelude = read_workspace_file("crates/machine/src/backend/prelude.rs");
    let runtime_builder = read_workspace_file("crates/runtime/src/bootstrap/builder.rs");
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
    assert!(
        ext_lib.contains("pub use backend::precompile::PrecompileBackendFactory;"),
        "tabula-ext root must expose the curated precompile backend factory trait"
    );
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
fn precompile_redesign_removes_old_id_only_and_digest_slot_paths() {
    for rel in [
        "crates/compiler/src/register.rs",
        "crates/compiler/src/program.rs",
        "crates/artifact/src/program.rs",
        "crates/runtime/src/proving/journal/mod.rs",
        "crates/runtime/src/proving/journal/types.rs",
        "crates/runtime/src/proving/journal/state.rs",
        "crates/runtime/src/proving/journal/tx.rs",
        "crates/runtime/src/proving/journal/reduce.rs",
        "crates/runtime/src/proving/artifacts.rs",
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
fn stable_scheme_surface_fixture_uses_canonical_backend_only_registration() {
    let stable_fixture = read_workspace_file("crates/runtime/tests/stable_scheme_surface.rs");

    assert!(
        stable_fixture.contains(".with_column_backend_bundle("),
        "stable scheme surface fixture must register custom schemes through canonical backend bundles"
    );
    assert!(
        stable_fixture.contains(".with_host_environment(")
            && stable_fixture.contains("HostEnvironment::empty()"),
        "stable scheme surface fixture must install custom schemes through HostEnvironment"
    );
    assert!(
        !stable_fixture.contains(".with_scheme(")
            && !stable_fixture.contains(".with_scheme_bundle(")
            && !stable_fixture.contains(".with_proof_scheme("),
        "stable scheme surface fixture must not use removed legacy registration APIs"
    );
}

#[test]
fn old_runtime_installation_modules_are_removed() {
    for rel in [
        "crates/runtime/src/extension_contracts.rs",
        "crates/runtime/src/setup/environment.rs",
        "crates/runtime/src/setup/precompile.rs",
    ] {
        assert!(
            !workspace_root().join(rel).exists(),
            "{rel} must be removed after the host/machine refactor"
        );
    }
}

#[test]
fn canonical_renamed_phase_objects_do_not_regress() {
    assert!(
        !workspace_root()
            .join("crates/witness/src/prepare.rs")
            .exists(),
        "crates/witness/src/prepare.rs must stay deleted after Stage 4"
    );
    for rel in [
        "crates/witness/src/lib.rs",
        "crates/runtime/src/bootstrap/materialize.rs",
        "crates/runtime/src/runtime.rs",
        "crates/runtime/src/proving/journal/mod.rs",
        "crates/runtime/src/proving/journal/types.rs",
        "crates/runtime/src/proving/journal/state.rs",
        "crates/runtime/src/proving/journal/tx.rs",
        "crates/runtime/src/proving/journal/reduce.rs",
        "crates/runtime/src/proving/artifacts.rs",
        "crates/runtime/src/proving/traces.rs",
        "crates/runtime/src/bootstrap/builder.rs",
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
    assert!(
        !workspace_root()
            .join("crates/witness/src/prepare.rs")
            .exists(),
        "crates/witness/src/prepare.rs must stay deleted after Stage 4"
    );
    for rel in [
        "crates/core/src/traits/state.rs",
        "crates/runtime/src/program/contract.rs",
        "crates/runtime/src/bootstrap/materialize.rs",
        "crates/runtime/src/proving/journal/mod.rs",
        "crates/runtime/src/proving/journal/types.rs",
        "crates/runtime/src/proving/journal/reduce.rs",
        "crates/runtime/src/proving/artifacts.rs",
    ] {
        let source = read_workspace_file(rel);
        for forbidden in [
            "trait StateSnapshot",
            "struct ResolvedRuntimeProgram",
            "pub struct ResolvedRuntimeProgram",
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
            "struct PreparedBatchJournal",
            "pub(crate) struct PreparedBatchJournal",
            "struct PreparedProofArtifacts",
            "pub(crate) struct PreparedProofArtifacts",
            "struct PreparedColumnSlot",
            "pub(crate) struct PreparedColumnSlot",
            "struct ProofJournalInput",
            "pub(crate) struct ProofJournalInput",
            "struct TxProofShard",
            "pub(crate) struct TxProofShard",
        ] {
            assert!(
                !source.contains(forbidden),
                "{rel} must not reintroduce renamed internal symbol '{forbidden}'"
            );
        }
    }
}

#[test]
fn stage1_runtime_contract_split_is_enforced() {
    let runtime_rs = read_workspace_file("crates/runtime/src/runtime.rs");
    let runtime_lib_rs = read_workspace_file("crates/runtime/src/lib.rs");
    let builder_rs = read_workspace_file("crates/runtime/src/bootstrap/builder.rs");
    let proving_journal_rs = read_workspace_files(&[
        "crates/runtime/src/proving/journal/mod.rs",
        "crates/runtime/src/proving/journal/types.rs",
        "crates/runtime/src/proving/journal/reduce.rs",
    ]);
    let proving_artifacts_rs = read_workspace_file("crates/runtime/src/proving/artifacts.rs");
    let executor_lib_rs = read_workspace_file("crates/executor/src/lib.rs");
    let executor_resolved_rs = read_workspace_file("crates/executor/src/resolved_program.rs");
    let executor_journal_rs = read_workspace_file("crates/executor/src/journal.rs");

    assert!(
        !runtime_lib_rs.contains("pub use program::ResolvedProgram;"),
        "runtime root must not re-export ResolvedProgram after Stage 1"
    );
    assert!(
        runtime_lib_rs.contains("RuntimeProgram")
            && runtime_lib_rs.contains("ResolvedProofProgram")
            && runtime_lib_rs.contains("ProofPlan"),
        "runtime root must export the canonical Stage 1 runtime/proof contract nouns"
    );
    assert!(
        runtime_rs.contains("runtime_program: RuntimeProgram"),
        "TabulaRuntime must own RuntimeProgram as its root contract"
    );
    for forbidden in ["proof_recipes:", "precompile_recipes:"] {
        assert!(
            !runtime_rs.contains(forbidden),
            "TabulaRuntime must not own standalone recipe vectors ({forbidden})"
        );
    }
    assert!(
        runtime_rs.contains("pub fn runtime_program(&self)")
            && runtime_rs.contains("pub fn execution_program(&self)")
            && runtime_rs.contains("pub fn proof_program(&self)"),
        "TabulaRuntime must expose split contract accessors"
    );
    assert!(
        builder_rs.contains("RuntimeProgram::from_compiled_program")
            && builder_rs.contains("ProofPlan::new("),
        "runtime builder must materialize the split runtime/proof contracts"
    );
    assert!(
        proving_journal_rs.contains("resolved_program: &'a ResolvedProofProgram")
            && proving_artifacts_rs.contains("resolved_program: &ResolvedProofProgram"),
        "runtime proving stages must consume ResolvedProofProgram directly"
    );
    assert!(
        !executor_lib_rs.contains("execute_batch_resolved")
            && executor_lib_rs.contains("execute_batch")
            && executor_lib_rs.contains("ResolvedExecutionProgram")
            && executor_lib_rs.contains("ExecutionJournal")
            && executor_lib_rs.contains("SuccessfulTxExecution"),
        "executor root must expose the canonical Stage 1 execution nouns"
    );
    assert!(
        executor_resolved_rs.contains("pub struct ResolvedExecutionProgram"),
        "executor must own the canonical resolved execution contract"
    );
    assert!(
        executor_journal_rs.contains("pub struct ExecutionJournal")
            && executor_journal_rs.contains("pub struct SuccessfulTxExecution"),
        "executor must define the canonical execution journal anchors"
    );
}

#[test]
fn stage2_executor_journal_cutover_is_enforced() {
    let executor_batch_rs = read_workspace_file("crates/executor/src/batch.rs");
    let executor_journal_rs = read_workspace_file("crates/executor/src/journal.rs");
    let executor_overlay_rs = read_workspace_file("crates/executor/src/overlay.rs");
    let executor_lib_rs = read_workspace_file("crates/executor/src/lib.rs");
    let runtime_execute_rs = read_workspace_files(&[
        "crates/runtime/src/execute/envelope.rs",
        "crates/runtime/src/execute/pipeline.rs",
    ]);

    assert!(
        !workspace_root()
            .join("crates/executor/src/trace_recorder.rs")
            .exists(),
        "TraceRecorder must stay deleted after Stage 2"
    );
    assert!(
        executor_batch_rs.contains("-> Result<ExecutionJournal, TabulaError>")
            && !executor_batch_rs.contains("Result<BatchReport"),
        "executor batch API must return ExecutionJournal, not BatchReport"
    );
    assert!(
        !executor_batch_rs.contains("execute_batch_resolved")
            && !executor_batch_rs.contains("&Program"),
        "executor canonical path must not reintroduce raw-program or resolved-wrapper entrypoints"
    );
    for forbidden in ["TraceRecorder", "events_since", "set_tx_index", "fn time("] {
        assert!(
            !executor_overlay_rs.contains(forbidden),
            "Overlay must stay state-only after Stage 2 ({forbidden})"
        );
    }
    assert!(
        executor_lib_rs.contains("ExecutionJournal")
            && executor_lib_rs.contains("ExecutionStateSummary")
            && executor_lib_rs.contains("FailedAccessObservation")
            && executor_lib_rs.contains("derive_batch_report")
            && executor_lib_rs.contains("derive_portable_state_summary")
            && executor_lib_rs.contains("derive_consistency_status"),
        "executor root must expose the journal-first Stage 2 surface"
    );
    assert!(
        executor_journal_rs.contains("pub struct ExecutionJournal")
            && executor_journal_rs.contains("pub state_summary: ExecutionStateSummary")
            && executor_journal_rs.contains("pub struct ExecutionStateSummary")
            && executor_journal_rs.contains("pub partial_accesses: Vec<FailedAccessObservation>")
            && !executor_journal_rs.contains("partial_access_effects"),
        "ExecutionJournal must nest state_summary and keep failed diagnostics distinct from canonical access effects"
    );
    assert!(
        runtime_execute_rs.contains("execution_journal: ExecutionJournal")
            && runtime_execute_rs.contains("batch_report: BatchReport")
            && runtime_execute_rs.contains("pub fn execution_journal(&self) -> &ExecutionJournal")
            && runtime_execute_rs.contains("derive_portable_state_summary(")
            && !runtime_execute_rs.contains(
                "merge_output_state_entries(&normalized.cells, &batch_report.write_set_final)"
            ),
        "runtime execution envelope must store ExecutionJournal as primary and BatchReport as derived view"
    );
}

#[test]
fn stage3_runtime_proving_is_journal_first() {
    let proving_mod_rs = read_workspace_file("crates/runtime/src/proving/mod.rs");
    let proving_journal_rs = read_workspace_files(&[
        "crates/runtime/src/proving/journal/mod.rs",
        "crates/runtime/src/proving/journal/types.rs",
        "crates/runtime/src/proving/journal/state.rs",
        "crates/runtime/src/proving/journal/tx.rs",
        "crates/runtime/src/proving/journal/reduce.rs",
    ]);
    let proving_artifacts_rs = read_workspace_file("crates/runtime/src/proving/artifacts.rs");
    let proving_traces_rs = read_workspace_file("crates/runtime/src/proving/traces.rs");
    let runtime_rs = read_workspace_file("crates/runtime/src/runtime.rs");

    for (name, source) in [
        ("proving/mod.rs", proving_mod_rs.as_str()),
        ("proving/journal/", proving_journal_rs.as_str()),
        ("proving/artifacts.rs", proving_artifacts_rs.as_str()),
        ("proving/traces.rs", proving_traces_rs.as_str()),
    ] {
        for forbidden in [
            "BatchReport",
            "ExecutionInputPreparer",
            "lower_program_batch",
            "LowerProgramBatchInput",
        ] {
            assert!(
                !source.contains(forbidden),
                "{name} must not depend on removed Stage 2 prove-path helper '{forbidden}'"
            );
        }
    }
    for forbidden in [
        "AccessEvent as PortableAccessEvent",
        "PropertyReadResult",
        "portable_access_event(",
        "portable_property_read(",
        "use tabula_core::{Batch, ColId, OpKind, PortableValue, PrecompileEvent",
    ] {
        assert!(
            !proving_journal_rs.contains(forbidden),
            "proving/journal/ must not reintroduce a portable lowering seam '{forbidden}'"
        );
    }
    assert!(
        !proving_journal_rs.contains("compute_precompile_call_header("),
        "proving/journal/ must not reconstruct precompile transcript headers through a duplicate encoding path"
    );

    assert!(
        proving_mod_rs.contains("prepare_proof_artifacts")
            && proving_mod_rs.contains("build_proof_journal"),
        "proving module must expose the Stage 3 journal/artifact split"
    );
    assert!(
        proving_journal_rs.contains("pub(crate) struct ProofJournal")
            && proving_journal_rs.contains("pub(crate) struct ProofColumnSlot")
            && !proving_journal_rs.contains("shared_store: WitnessStore")
            && !proving_journal_rs.contains("air_statement: PublicStatement"),
        "ProofJournal must stay a proof-input journal rather than a machine-ready artifact bundle"
    );
    assert!(
        proving_artifacts_rs.contains("pub(crate) struct ProofArtifacts")
            && proving_artifacts_rs.contains("shared_store: WitnessStore")
            && proving_artifacts_rs.contains("air_statement: PublicStatement"),
        "ProofArtifacts must own the machine-ready store and AIR statement"
    );
    assert!(
        runtime_rs.contains("build_proof_journal(proving::JournalInput")
            && runtime_rs.contains("prepare_proof_artifacts(self.proof_program(), journal)")
            && !runtime_rs.contains("prepare_proof_batch("),
        "runtime proving entrypoints must reduce ExecutionJournal into a prepared proof journal before backend artifact preparation"
    );
}

#[test]
fn runtime_state_surface_validation_is_shared_and_fail_closed() {
    let execute_rs = read_workspace_file("crates/runtime/src/execute/pipeline.rs");
    let runtime_rs = read_workspace_file("crates/runtime/src/runtime.rs");
    let state_validation_rs = read_workspace_file("crates/runtime/src/policy/surface.rs");
    let proving_journal_rs = read_workspace_file("crates/runtime/src/proving/journal/reduce.rs");
    let runtime_lib_rs = read_workspace_file("crates/runtime/src/lib.rs");

    assert!(
        state_validation_rs.contains("validate_execution_state_surface")
            && state_validation_rs.contains("validate_proof_state_surface")
            && state_validation_rs.contains("validate_prove_input_prestate"),
        "runtime policy surface module must own shared execution/proof state-surface validators"
    );
    assert!(
        execute_rs.contains("validate_execution_state_surface(program, &normalized)?"),
        "execute pipeline must validate normalized state against the execution surface"
    );
    assert!(
        !execute_rs.contains("crate::setup::validation"),
        "execute pipeline must not depend on prove/verify-gated setup validation"
    );
    assert!(
        runtime_rs
            .contains("validate_execution_state_surface(self.execution_program(), &normalized)?")
            && runtime_rs
                .contains("validate_proof_state_surface(self.proof_program(), &normalized)?")
            && runtime_rs.contains("validate_prove_input_prestate("),
        "runtime prove entrypoints must fail closed on state-surface mismatch and prove-input pre-state mismatch"
    );
    assert!(
        proving_journal_rs
            .contains("validate_proof_state_surface(input.resolved_program, &normalized_state)?"),
        "direct proof-journal reduction must validate state against the proof surface"
    );
    assert!(
        runtime_lib_rs.contains("mod policy;"),
        "runtime root must include an ungated policy module"
    );
}

#[test]
fn runtime_host_surface_is_gated_to_verify_and_prove() {
    let runtime_lib_rs = read_workspace_file("crates/runtime/src/lib.rs");

    assert!(
        runtime_lib_rs
            .contains("#[cfg(any(feature = \"prove\", feature = \"verify\"))]\nmod host;")
            && runtime_lib_rs.contains("pub use host::{")
            && runtime_lib_rs.contains("HostEnvironment")
            && runtime_lib_rs.contains("RuntimeRegistries")
            && runtime_lib_rs.contains("InstalledPrecompiles")
            && runtime_lib_rs.contains("InstalledSchemes"),
        "runtime host surface must be gated to verify/prove builds"
    );
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
fn sealed_profile_surface_stays_old_surface_free() {
    let artifact_program = read_workspace_file("crates/artifact/src/program.rs");
    let core_schema = read_workspace_file("crates/core/src/state/schema.rs");
    let ir_tx = read_workspace_file("crates/ir/src/tx.rs");
    let compiler_lib = read_workspace_file("crates/compiler/src/lib.rs");
    let profile_lib = read_workspace_file("crates/profile/src/lib.rs");
    let runtime_materialize = read_workspace_file("crates/runtime/src/bootstrap/materialize.rs");

    assert!(
        !artifact_program.contains("pub column_proof_plan:"),
        "sealed artifact surface must not store legacy column_proof_plan fields"
    );
    assert!(
        !core_schema.contains("pub value_type:"),
        "sealed column schema must not store legacy value_type"
    );
    assert!(
        !ir_tx.contains("pub value_type:"),
        "sealed IR param definitions must not store legacy value_type"
    );
    assert!(
        !compiler_lib.contains(" register_program,")
            && !compiler_lib.contains("pub fn register_program("),
        "compiler root must not re-export the removed register_program API"
    );
    assert!(
        !profile_lib.contains("pub use compat::"),
        "profile root must not re-export compat helpers as canonical surface"
    );
    assert!(
        !runtime_materialize.contains("projected_column_proof_plan")
            && !runtime_materialize.contains("column_proof_plan"),
        "runtime planning must derive compat plans from resolved profiles, not artifact-stored proof plans"
    );
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
        "crates/runtime/src/bootstrap/builder.rs",
        "crates/runtime/src/testing/schemes.rs",
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

    for forbidden in [
        "pub mod prepare;",
        "ExecutionInputPreparer",
        "PreparedExecutionColumn",
        "PreparedExecutionColumns",
    ] {
        assert!(
            !lib_rs.contains(forbidden),
            "witness root must not expose removed proof-orchestration surface '{forbidden}'"
        );
    }
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
            lib_rs.contains(required),
            "witness root must expose logical preparation row type '{required}'"
        );
    }
    assert!(
        lib_rs.contains("pub mod stark;"),
        "witness crate must expose its STARK-specific helpers under a namespaced module"
    );
}

#[test]
fn runtime_builtin_schemes_do_not_own_low_level_stark_witness_assembly() {
    for rel in [
        "crates/runtime/src/host/builtins/ssmc.rs",
        "crates/runtime/src/host/builtins/smt.rs",
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
        "lower_program_batch",
        "LowerProgramBatchInput",
    ] {
        assert!(
            !stark_mod.contains(forbidden),
            "witness::stark must not re-export logical prep type '{forbidden}'"
        );
    }
}

#[test]
fn materialize_helpers_consume_prederived_column_plans() {
    let materialize_rs = read_workspace_file("crates/runtime/src/bootstrap/materialize.rs");
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
