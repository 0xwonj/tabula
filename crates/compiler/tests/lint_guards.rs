#![allow(missing_docs)]

use std::fs;
use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

fn rust_sources_under(rel: &str) -> Vec<PathBuf> {
    fn walk(dir: &Path, files: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(dir).expect("read dir") {
            let entry = entry.expect("dir entry");
            let path = entry.path();
            if path.is_dir() {
                walk(&path, files);
            } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
                files.push(path);
            }
        }
    }

    let mut files = Vec::new();
    walk(&workspace_root().join(rel), &mut files);
    files.sort();
    files
}

fn assert_no_file_wide_allows(path: &Path, forbidden: &[&str]) {
    let source =
        fs::read_to_string(path).unwrap_or_else(|_| panic!("read source {}", path.display()));
    for needle in forbidden {
        assert!(
            !source.contains(needle),
            "{} must not contain forbidden file-wide allow `{needle}`",
            path.display()
        );
    }
}

#[test]
fn strict_core_has_no_file_wide_broad_allows() {
    let forbidden = [
        "#![allow(dead_code)]",
        "#![allow(unused_imports)]",
        "#![allow(clippy::wildcard_imports)]",
    ];

    for rel in [
        "crates/compiler/src",
        "crates/runtime/src",
        "crates/witness/src",
        "crates/chips/src",
    ] {
        for path in rust_sources_under(rel) {
            let source = fs::read_to_string(&path).expect("read source");
            for needle in forbidden {
                assert!(
                    !source.contains(needle),
                    "{} must not contain forbidden file-wide allow `{needle}`",
                    path.display()
                );
            }
        }
    }
}

#[test]
fn lang_frontend_does_not_use_file_wide_broad_allows() {
    for path in rust_sources_under("crates/lang/src") {
        assert_no_file_wide_allows(
            &path,
            &[
                "#![allow(dead_code)]",
                "#![allow(unused_imports)]",
                "#![allow(clippy::wildcard_imports)]",
            ],
        );
    }
}
