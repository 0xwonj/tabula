//! SP-5 §8 boundary guardrail.
//!
//! Runtime **production** code under `crates/runtime/src/**/*.rs` must
//! not name the chip-layer row types `InstructionRecord` or
//! `RelationTableWitnessRow` through their canonical
//! `tabula_chips::...` paths. Crossings to chip-internal row layout
//! must happen through the logical types in `tabula-stark::witness_kit`
//! (`LogicalExecutionPrelude`, `LogicalRelationTableRow`) plus the
//! chip-side `From` conversions in `tabula-chips`.
//!
//! The §8 intent is *production-code cleanliness* — the runtime prove
//! surface should not couple to chip-row internals. Inline test files
//! are not production code. This walker therefore treats files whose
//! filename ends in `_tests.rs` (a project-wide convention for inline
//! integration tests included via `#[path]`) as test-side and skips
//! them, mirroring the way `crates/runtime/tests/**` sits outside the
//! scan entirely. Tamper-class tests that fundamentally need to
//! construct chip-row values to exercise the prover's rejection
//! semantics live inline at `crates/runtime/src/prover_relation_tests.rs`
//! and are allowed to name the chip row types directly.
//!
//! See SP-5 review findings (`docs/notes/sp5-review-findings.md`) blocker
//! B-1 and the spec §8.1 / §8.2 / §12.

#![allow(missing_docs)]

use std::fs;
use std::path::{Path, PathBuf};

const FORBIDDEN_PATHS: &[&str] = &[
    "tabula_chips::execution::trace::InstructionRecord",
    "tabula_chips::relation_table::RelationTableWitnessRow",
];

fn runtime_src_dir() -> PathBuf {
    // CARGO_MANIFEST_DIR points at crates/runtime; src/ lives next door.
    let crate_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    crate_root.join("src")
}

fn is_inline_test_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with("_tests.rs"))
}

fn walk_rust_files(root: &Path, out: &mut Vec<PathBuf>) {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) => panic!("failed to read {}: {error}", root.display()),
    };
    for entry in entries {
        let entry = entry.expect("directory entry");
        let path = entry.path();
        let file_type = entry.file_type().expect("file type");
        if file_type.is_dir() {
            walk_rust_files(&path, out);
        } else if file_type.is_file()
            && path.extension().is_some_and(|ext| ext == "rs")
            && !is_inline_test_file(&path)
        {
            out.push(path);
        }
    }
}

#[test]
fn runtime_src_does_not_import_chip_row_types() {
    let src = runtime_src_dir();
    let mut files = Vec::new();
    walk_rust_files(&src, &mut files);
    assert!(
        !files.is_empty(),
        "expected at least one .rs file under {}",
        src.display()
    );

    let mut violations = Vec::new();
    for file in &files {
        let contents = fs::read_to_string(file)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", file.display()));
        for (line_index, line) in contents.lines().enumerate() {
            for forbidden in FORBIDDEN_PATHS {
                if line.contains(forbidden) {
                    violations.push(format!(
                        "{}:{}: forbidden reference `{}`",
                        file.display(),
                        line_index + 1,
                        forbidden
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "\nSP-5 §8 chip-row boundary violated:\n  {}\n\n\
         Production runtime code must not name chip-layer row types. \
         Route through `tabula_stark::witness_kit::LogicalExecutionPrelude` \
         / `LogicalRelationTableRow` and the chip-side `From` impls.",
        violations.join("\n  "),
    );
}
