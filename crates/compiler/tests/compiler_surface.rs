#![allow(missing_docs)]
use std::fs;
use std::path::{Path, PathBuf};

use tabula_compiler::{self, compile_and_register_program_source};

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

#[test]
fn root_compile_is_deterministic_for_rewritten_source() {
    let first = tabula_compiler::compile_program_source(next_source()).expect("first compile");
    let second = tabula_compiler::compile_program_source(next_source()).expect("second compile");

    assert_eq!(first.program().program_id, second.program().program_id);
    assert_eq!(
        first.program().entries.len(),
        second.program().entries.len()
    );
    assert_eq!(first.field_schemes(), second.field_schemes());
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
