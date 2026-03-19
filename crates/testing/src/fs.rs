//! Shared filesystem helpers for tests that need isolated JSON fixtures.

use std::path::PathBuf;

pub use tempfile::TempDir;

use tabula_artifact::{ProgramArtifact, StateSnapshot, TransactionBatch, write_json};

pub fn tempdir() -> TempDir {
    tempfile::tempdir().expect("create test tempdir")
}

pub fn write_program_artifact_json(
    dir: &TempDir,
    file_name: &str,
    artifact: &ProgramArtifact,
) -> PathBuf {
    let path = dir.path().join(file_name);
    write_json(&path, artifact).expect("write program artifact json");
    path
}

pub fn write_state_json(dir: &TempDir, file_name: &str, state: &StateSnapshot) -> PathBuf {
    let path = dir.path().join(file_name);
    write_json(&path, state).expect("write state json");
    path
}

pub fn write_batch_json(dir: &TempDir, file_name: &str, batch: &TransactionBatch) -> PathBuf {
    let path = dir.path().join(file_name);
    write_json(&path, batch).expect("write batch json");
    path
}
