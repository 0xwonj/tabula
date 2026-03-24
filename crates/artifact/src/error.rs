//! Artifact-layer error types.

use thiserror::Error;

/// Artifact-layer error.
#[derive(Debug, Error)]
pub enum ArtifactError {
    /// Missing state value for a required cell.
    #[error("state cell is missing value (table={table}, row={row}, col={col})")]
    MissingStateValue {
        /// Table id.
        table: u32,
        /// Row key.
        row: u64,
        /// Column id.
        col: u16,
    },
    /// JSON file read error.
    #[cfg(not(target_arch = "wasm32"))]
    #[error("failed to read {path}: {source}")]
    ReadJson {
        /// File path.
        path: String,
        /// Source error.
        source: std::io::Error,
    },
    /// JSON file parse error.
    #[cfg(not(target_arch = "wasm32"))]
    #[error("failed to parse {path}: {source}")]
    ParseJson {
        /// File path.
        path: String,
        /// Source error.
        source: serde_json::Error,
    },
    /// JSON file write error.
    #[cfg(not(target_arch = "wasm32"))]
    #[error("failed to write {path}: {source}")]
    WriteJson {
        /// File path.
        path: String,
        /// Source error.
        source: std::io::Error,
    },
    /// JSON serialization error.
    #[error("failed to encode JSON: {0}")]
    EncodeJson(#[from] serde_json::Error),
    /// A profile-backed compatibility projection failed.
    #[error("failed to project legacy artifact compatibility shape: {detail}")]
    InvalidProfileProjection {
        /// Human-readable projection failure detail.
        detail: String,
    },
    /// Invalid portable value at an artifact/runtime boundary.
    #[error("invalid portable value: {detail}")]
    InvalidPortableValue {
        /// Human-readable decode or validation failure detail.
        detail: String,
    },
}
