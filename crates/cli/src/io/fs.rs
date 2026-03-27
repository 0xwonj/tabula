//! File writing helpers.

use std::path::Path;

use anyhow::{Context as _, bail};

/// Ensure the containing directory exists before writing a file.
pub(crate) fn ensure_parent_dir(path: &Path) -> anyhow::Result<()> {
    let Some(parent) = path.parent() else {
        bail!("path {} has no parent directory", path.display());
    };
    std::fs::create_dir_all(parent)
        .with_context(|| format!("failed to create directory {}", parent.display()))
}

/// Serialize one JSON file with pretty formatting.
pub(crate) fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> anyhow::Result<()> {
    let bytes = serde_json::to_vec_pretty(value)
        .with_context(|| format!("failed to encode JSON for {}", path.display()))?;
    std::fs::write(path, bytes).with_context(|| format!("failed to write {}", path.display()))
}

/// Write one binary file.
pub(crate) fn write_bytes(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    std::fs::write(path, bytes).with_context(|| format!("failed to write {}", path.display()))
}

/// Write one UTF-8 text file.
pub(crate) fn write_text(path: &Path, text: &str) -> anyhow::Result<()> {
    std::fs::write(path, text).with_context(|| format!("failed to write {}", path.display()))
}
