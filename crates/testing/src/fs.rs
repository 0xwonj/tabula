//! Shared filesystem helpers for tests that need isolated JSON fixtures.

use std::path::PathBuf;

use serde::Serialize;
pub use tempfile::TempDir;

pub fn tempdir() -> TempDir {
    tempfile::tempdir().expect("create test tempdir")
}

pub fn write_json<T: Serialize>(dir: &TempDir, file_name: &str, value: &T) -> PathBuf {
    let path = dir.path().join(file_name);
    let body = serde_json::to_vec_pretty(value).expect("serialize json fixture");
    std::fs::write(&path, body).expect("write json fixture");
    path
}
