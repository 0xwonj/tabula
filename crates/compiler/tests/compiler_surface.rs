#![allow(missing_docs)]
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::json;
use tabula_compiler::{self, compile_and_register_program_source, parse_registered_program};
use tabula_profile::{ENCODING_U64_ID, TYPE_BOOL_ID, TYPE_U64_ID};

fn next_source() -> &'static str {
    r#"
program P

state {
  table users(key id: u64) {
    tier: u64 @ssmc;
  }
}

tx register(id: u64, tier: u64) {
  users[id].tier = tier;
  return;
}
"#
}

fn relation_source() -> &'static str {
    r#"
program P

relation Allowed(x: u64) = enum { 1, 2, 3 };

tx check(x: u64) {
  assert relation Allowed(x);
  return;
}
"#
}

fn changed_relation_source() -> &'static str {
    r#"
program P

relation Allowed(x: u64) = enum { 1, 2, 4 };

tx check(x: u64) {
  assert relation Allowed(x);
  return;
}
"#
}

fn composite_key_source() -> &'static str {
    r#"
program P

state {
  table users(key id: u64, shard: u64) {
    tier: u64 @ssmc;
  }
}

tx register(id: u64, shard: u64, tier: u64) {
  users[id, shard].tier = tier;
  return;
}
"#
}

fn bool_key_source() -> &'static str {
    r#"
program P

state {
  table users(key active: bool) {
    tier: u64 @ssmc;
  }
}

tx register(active: bool, tier: u64) {
  users[active].tier = tier;
  return;
}
"#
}

fn bytes32_key_source() -> &'static str {
    r#"
program P

state {
  table users(key id: bytes32) {
    tier: u64 @ssmc;
  }
}

tx register(id: bytes32, tier: u64) {
  users[id].tier = tier;
  return;
}
"#
}

fn expanded_key_surface_source() -> &'static str {
    r#"
program P

state {
  table users(key id: u64) {
    tier: u64 @ssmc;
  }
  table teams(key id: u64) {
    score: u64 @ssmc;
  }
}

tx register(id: u64, tier: u64) {
  users[id].tier = tier;
  return;
}
"#
}

fn registered_artifact_value(source: &str) -> serde_json::Value {
    let catalogs = tabula_compiler::CompilerCatalogs::standard().expect("standard catalogs");
    let registered =
        compile_and_register_program_source(source, &catalogs).expect("compile and register");
    serde_json::to_value(&registered).expect("serialize registered program")
}

fn parse_registered_artifact_value(
    value: &serde_json::Value,
) -> Result<tabula_compiler::RegisteredProgram, tabula_compiler::CompilerError> {
    parse_registered_program(
        &serde_json::to_string(value).expect("serialize artifact json"),
        "<compiler-surface-test>",
    )
}

#[test]
fn root_compile_is_deterministic_for_rewritten_source() {
    let first = tabula_compiler::compile_program_source(next_source()).expect("first compile");
    let second = tabula_compiler::compile_program_source(next_source()).expect("second compile");

    assert_eq!(first.validated_program(), second.validated_program());
}

#[test]
fn root_register_round_trips_rewritten_source() {
    let catalogs = tabula_compiler::CompilerCatalogs::standard().expect("standard catalogs");
    let first =
        compile_and_register_program_source(next_source(), &catalogs).expect("first register");
    let second =
        compile_and_register_program_source(next_source(), &catalogs).expect("second register");

    assert_eq!(first.binding(), second.binding());
    assert_eq!(first.program().program_id, second.program().program_id);
    assert_eq!(
        first.tuple_encoding_defaults(),
        second.tuple_encoding_defaults()
    );
    assert!(
        !first.tuple_encoding_defaults().entries().is_empty(),
        "registered programs should seal tuple encoding defaults",
    );
    assert_eq!(first.execution_contract(), second.execution_contract());
}

#[test]
fn registered_program_seals_table_key_contracts_and_machine_shape() {
    let catalogs = tabula_compiler::CompilerCatalogs::standard().expect("standard catalogs");
    let registered =
        compile_and_register_program_source(next_source(), &catalogs).expect("registered");

    assert_eq!(registered.execution_contract().state.tables.len(), 1);
    let table = &registered.execution_contract().state.tables[0];
    assert_eq!(table.key.components.len(), 1);
    assert_eq!(table.key.components[0].ty, TYPE_U64_ID);
    let contract = &table.key;
    assert_eq!(
        contract.component_encoding_profile_ids,
        vec![ENCODING_U64_ID]
    );
    assert_eq!(contract.committed_layout.byte_width, 8);
    assert_eq!(contract.committed_layout.fe_width, 3);
    assert_eq!(
        registered
            .execution_contract()
            .machine_shape
            .max_key_components,
        1
    );
    assert_eq!(registered.execution_contract().machine_shape.max_key_fes, 3);
}

#[test]
fn registered_static_table_artifact_is_deterministic_for_relations() {
    let catalogs = tabula_compiler::CompilerCatalogs::standard().expect("standard catalogs");
    let first =
        compile_and_register_program_source(relation_source(), &catalogs).expect("first register");
    let second =
        compile_and_register_program_source(relation_source(), &catalogs).expect("second register");

    assert_eq!(
        first.static_table_artifact(),
        second.static_table_artifact()
    );
}

#[test]
fn relation_manifest_changes_static_table_artifact() {
    let catalogs = tabula_compiler::CompilerCatalogs::standard().expect("standard catalogs");
    let first =
        compile_and_register_program_source(relation_source(), &catalogs).expect("first register");
    let second = compile_and_register_program_source(changed_relation_source(), &catalogs)
        .expect("second register");

    assert_ne!(
        first.static_table_artifact(),
        second.static_table_artifact()
    );
}

#[test]
fn registration_rejects_composite_keys_outside_current_native_machine_shape() {
    let catalogs = tabula_compiler::CompilerCatalogs::standard().expect("standard catalogs");
    let err = compile_and_register_program_source(composite_key_source(), &catalogs)
        .expect_err("composite keys should fail closed against the current native unary machine");
    assert!(
        err.to_string()
            .contains("exceeds native machine capabilities"),
        "unexpected error: {err}"
    );
}

#[test]
fn registration_rejects_non_u64_keys_before_runtime() {
    let catalogs = tabula_compiler::CompilerCatalogs::standard().expect("standard catalogs");
    let registered = compile_and_register_program_source(bool_key_source(), &catalogs)
        .expect("bool keys should seal natively through the execution contract");
    assert_eq!(
        registered.execution_contract().state.tables[0]
            .key
            .components[0]
            .ty,
        TYPE_BOOL_ID
    );
}

#[test]
fn registration_rejects_missing_default_key_encoding_cleanly() {
    let catalogs = tabula_compiler::CompilerCatalogs::standard().expect("standard catalogs");
    let err = compile_and_register_program_source(bytes32_key_source(), &catalogs)
        .expect_err("bytes32 keys must fail without a default key encoding");

    let rendered = err.to_string();
    assert!(
        rendered.contains("default key encoding"),
        "actual error: {rendered}"
    );
}

#[test]
fn binding_changes_when_registered_key_surface_changes() {
    let catalogs = tabula_compiler::CompilerCatalogs::standard().expect("standard catalogs");
    let base =
        compile_and_register_program_source(next_source(), &catalogs).expect("base register");
    let expanded = compile_and_register_program_source(expanded_key_surface_source(), &catalogs)
        .expect("expanded register");

    assert_ne!(base.binding(), expanded.binding());
    assert_ne!(base.execution_contract(), expanded.execution_contract());
}

#[test]
fn compiler_parse_accepts_fresh_registered_artifact() {
    let value = registered_artifact_value(relation_source());
    parse_registered_artifact_value(&value).expect("fresh compiler artifact should parse");
}

#[test]
fn compiler_parse_rejects_mutated_artifact_schema_version() {
    let mut value = registered_artifact_value(relation_source());
    value["artifact_schema_version"] = json!(u32::MAX);

    let err = parse_registered_artifact_value(&value)
        .expect_err("mutated artifact schema version must fail closed");
    assert!(
        err.to_string()
            .contains("unsupported registered artifact schema version"),
        "unexpected error: {err}"
    );
}

#[test]
fn compiler_parse_rejects_mutated_metadata_version() {
    let mut value = registered_artifact_value(relation_source());
    value["metadata_envelope"]["statement_schema_version"] = json!(u32::MAX);

    let err = parse_registered_artifact_value(&value)
        .expect_err("mutated statement schema version must fail closed");
    assert!(
        err.to_string().contains("statement schema version"),
        "unexpected error: {err}"
    );
}

#[test]
fn compiler_parse_rejects_mutated_profile_hash() {
    let mut value = registered_artifact_value(relation_source());
    value["metadata_envelope"]["profile_hash"][0] = json!(17);

    let err =
        parse_registered_artifact_value(&value).expect_err("mutated profile hash must fail closed");
    assert!(
        err.to_string().contains("profile hash mismatch"),
        "unexpected error: {err}"
    );
}

#[test]
fn compiler_parse_rejects_mutated_semantic_hash() {
    let mut value = registered_artifact_value(relation_source());
    value["metadata_envelope"]["semantic_hash"][0] = json!(23);

    let err = parse_registered_artifact_value(&value)
        .expect_err("mutated semantic hash must fail closed");
    assert!(
        err.to_string().contains("semantic hash"),
        "unexpected error: {err}"
    );
}

#[test]
fn internal_workspace_code_does_not_use_removed_compat_namespace_in_production_crates() {
    let workspace_root = workspace_root();
    let roots = [
        workspace_root.join("crates/compiler/src"),
        workspace_root.join("crates/sdk/src"),
        workspace_root.join("crates/cli/src"),
    ];
    let mut violations = Vec::new();
    for root in roots {
        collect_violations(&root, &workspace_root, &[], &mut violations);
    }
    assert!(
        violations.is_empty(),
        "legacy compiler namespace usage found:\n{}",
        violations.join("\n")
    );
}

#[test]
fn compiler_root_does_not_reexport_internal_field_scheme_sidecars() {
    let workspace_root = workspace_root();
    let compiler_root = fs::read_to_string(workspace_root.join("crates/compiler/src/lib.rs"))
        .expect("read compiler root");
    let compiler_pipeline =
        fs::read_to_string(workspace_root.join("crates/compiler/src/pipeline/mod.rs"))
            .expect("read compiler pipeline");
    let compiler_types =
        fs::read_to_string(workspace_root.join("crates/compiler/src/pipeline/types.rs"))
            .expect("read compiler types");

    assert!(
        !compiler_root.contains("StateFieldSchemeBinding"),
        "compiler root must not re-export internal field-scheme sidecars"
    );
    assert!(
        !compiler_pipeline.contains("pub use types::{CompiledProgram, REGISTERED_PROGRAM_SCHEMA_VERSION, RegisteredProgram, StateFieldSchemeBinding,"),
        "compiler pipeline root must not publicly re-export internal field-scheme sidecars"
    );
    assert!(
        !compiler_types.contains("pub fn field_schemes("),
        "compiled program must not expose field-scheme sidecars publicly"
    );
    assert!(
        !compiler_types.contains("pub fn into_parts("),
        "compiled program must not expose compiler-internal registration parts publicly"
    );
}

#[test]
fn contract_metadata_surface_does_not_reintroduce_binding_registry_version() {
    let workspace_root = workspace_root();
    for rel in [
        "crates/contract/src/compatibility.rs",
        "crates/contract/src/metadata_envelope.rs",
        "crates/contract/src/proof_envelope.rs",
        "crates/contract/src/lib.rs",
    ] {
        let source = fs::read_to_string(workspace_root.join(rel)).expect("read contract source");
        assert!(
            !source.contains("binding_registry_version"),
            "{rel} must not reintroduce binding_registry_version into the public contract surface"
        );
    }
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("workspace root")
        .to_path_buf()
}

fn collect_violations(
    dir: &Path,
    workspace_root: &Path,
    allowlist: &[PathBuf],
    violations: &mut Vec<String>,
) {
    let entries = fs::read_dir(dir).expect("read dir");
    for entry in entries {
        let entry = entry.expect("dir entry");
        let path = entry.path();
        if path.is_dir() {
            collect_violations(&path, workspace_root, allowlist, violations);
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) != Some("rs") {
            continue;
        }
        let relative = path
            .strip_prefix(workspace_root)
            .expect("relative path")
            .to_path_buf();
        if allowlist.iter().any(|allowed| allowed == &relative) {
            continue;
        }
        let contents = fs::read_to_string(&path).expect("read source");
        find_ambiguous_root_compat_usage(&relative, &contents, violations);
    }
}

fn find_ambiguous_root_compat_usage(relative: &Path, contents: &str, violations: &mut Vec<String>) {
    let removed_namespace = ["tabula_compiler::", "legacy"].concat();
    for (index, line) in contents.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("//") {
            continue;
        }
        if trimmed.contains(&removed_namespace)
            || trimmed.contains("use tabula_compiler::{") && trimmed.contains("legacy")
        {
            violations.push(format!(
                "{}:{} uses removed compiler alias surface",
                relative.display(),
                index + 1
            ));
        }
    }
}
