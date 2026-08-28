#![allow(missing_docs)]

use std::fs;
use std::path::{Path, PathBuf};

fn leaked(parts: &[&str]) -> &'static str {
    Box::leak(parts.concat().into_boxed_str())
}

#[test]
fn production_surfaces_do_not_depend_on_removed_compiler_or_artifact_paths() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("workspace root")
        .to_path_buf();

    let roots = [
        workspace_root.join("crates/sdk/src"),
        workspace_root.join("crates/cli/src"),
        workspace_root.join("crates/testing/src"),
    ];

    let denylist = [
        leaked(&["tabula_", "artifact", "::"]),
        leaked(&["tabula_compiler::", "legacy"]),
        leaked(&["tabula_compiler::", "Sealed", "Program"]),
        leaked(&["tabula_runtime::", "next"]),
        leaked(&["compile_", "next_", "program_source"]),
        "StateFieldSchemeBinding",
        "RowKey",
    ];

    let mut violations = Vec::new();
    for root in roots {
        collect_violations(&workspace_root, &root, &denylist, &mut violations);
    }

    assert!(
        violations.is_empty(),
        "removed compiler/artifact or row-era surface dependencies found:\n{}",
        violations.join("\n")
    );
}

#[test]
fn active_cli_docs_describe_logical_key_authoring() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("workspace root")
        .to_path_buf();

    let docs = [workspace_root.join("crates/cli/README.md")];
    let forbidden = [
        "tabula state set --program <PROGRAM> --state <STATE> <TABLE> <ROW> <FIELD> <VALUE>",
        "tabula state set --program program.artifact.json --state state.json balances 1 amount 100",
    ];

    let mut violations = Vec::new();
    for doc in docs {
        let relative = doc.strip_prefix(&workspace_root).expect("relative path");
        let source = fs::read_to_string(&doc).expect("read doc");
        for (index, line) in source.lines().enumerate() {
            for marker in &forbidden {
                if line.contains(marker) {
                    violations.push(format!(
                        "{}:{} contains forbidden row-era authoring syntax `{}`",
                        relative.display(),
                        index + 1,
                        marker
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "active CLI docs still describe row-era state authoring:\n{}",
        violations.join("\n")
    );
}

#[test]
fn sdk_examples_use_shared_public_statement_surface() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("workspace root")
        .to_path_buf();

    let examples_root = workspace_root.join("crates/sdk/examples");
    let denylist = ["interop::PublicStatement", "struct PublicStatementFile"];
    let mut violations = Vec::new();
    collect_violations(&workspace_root, &examples_root, &denylist, &mut violations);

    assert!(
        violations.is_empty(),
        "sdk examples still use non-canonical public-statement surface:\n{}",
        violations.join("\n")
    );
}

#[test]
fn active_docs_do_not_point_to_archived_canonical_vocabulary_note() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("workspace root")
        .to_path_buf();

    let docs = [
        workspace_root.join("README.md"),
        workspace_root.join("docs/README.md"),
        workspace_root.join("docs/design/architecture.md"),
    ];
    let mut violations = Vec::new();
    for doc in docs {
        let relative = doc.strip_prefix(&workspace_root).expect("relative path");
        let source = fs::read_to_string(&doc).expect("read doc");
        find_violations(
            relative,
            &source,
            &["canonical-vocabulary.md"],
            &mut violations,
        );
    }

    assert!(
        violations.is_empty(),
        "active docs still point at archived canonical-vocabulary note:\n{}",
        violations.join("\n")
    );
}

fn collect_violations(
    workspace_root: &Path,
    dir: &Path,
    denylist: &[&str],
    violations: &mut Vec<String>,
) {
    for entry in fs::read_dir(dir).expect("read dir") {
        let entry = entry.expect("dir entry");
        let path = entry.path();
        if path.is_dir() {
            collect_violations(workspace_root, &path, denylist, violations);
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) != Some("rs") {
            continue;
        }

        let relative = path
            .strip_prefix(workspace_root)
            .expect("relative path")
            .to_path_buf();
        let source = fs::read_to_string(&path).expect("read source");
        find_violations(&relative, &source, denylist, violations);
    }
}

fn find_violations(relative: &Path, source: &str, denylist: &[&str], violations: &mut Vec<String>) {
    let mut pending_test_mod = false;
    let mut test_mod_depth: Option<i32> = None;

    for (index, line) in source.lines().enumerate() {
        let trimmed = line.trim();

        if test_mod_depth.is_some() {
            let opens = line.chars().filter(|c| *c == '{').count() as i32;
            let closes = line.chars().filter(|c| *c == '}').count() as i32;
            let depth = test_mod_depth.expect("depth present") + opens - closes;
            if depth <= 0 {
                test_mod_depth = None;
            } else {
                test_mod_depth = Some(depth);
            }
            continue;
        }

        if trimmed == "#[cfg(test)]" {
            pending_test_mod = true;
            continue;
        }

        if pending_test_mod && trimmed.starts_with("mod tests") && trimmed.contains('{') {
            let opens = line.chars().filter(|c| *c == '{').count() as i32;
            let closes = line.chars().filter(|c| *c == '}').count() as i32;
            let depth = opens - closes;
            test_mod_depth = if depth > 0 { Some(depth) } else { None };
            pending_test_mod = false;
            continue;
        }

        pending_test_mod = false;

        if trimmed.starts_with("//") {
            continue;
        }

        for forbidden in denylist {
            if trimmed.contains(forbidden) {
                violations.push(format!(
                    "{}:{} contains forbidden dependency marker `{}`",
                    relative.display(),
                    index + 1,
                    forbidden
                ));
            }
        }
    }
}
