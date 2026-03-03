//! JSON file I/O helpers (non-wasm only).

use std::path::Path;

use crate::ArtifactError;

/// Read and parse JSON from file.
pub fn load_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, ArtifactError> {
    let path_str = path.display().to_string();
    let content = std::fs::read_to_string(path).map_err(|source| ArtifactError::ReadJson {
        path: path_str.clone(),
        source,
    })?;
    serde_json::from_str(&content).map_err(|source| ArtifactError::ParseJson {
        path: path_str,
        source,
    })
}

/// Serialize and write pretty JSON to file.
pub fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<(), ArtifactError> {
    let path_str = path.display().to_string();
    let content = serde_json::to_string_pretty(value)?;
    std::fs::write(path, content).map_err(|source| ArtifactError::WriteJson {
        path: path_str,
        source,
    })
}
