use std::path::{Path, PathBuf};

use anyhow::bail;
use serde::de::DeserializeOwned;

use crate::kernel::domain::InputRef;
use crate::protocol::error::{ApiError, ErrorCode};

/// Filesystem access policy for `kind=file` input.
#[derive(Debug, Clone)]
pub struct FileAccessPolicy {
    allowed_roots: Vec<PathBuf>,
}

impl FileAccessPolicy {
    pub fn new(allowed_roots: Vec<PathBuf>) -> anyhow::Result<Self> {
        if allowed_roots.is_empty() {
            bail!("allowed roots must not be empty");
        }
        Ok(Self { allowed_roots })
    }

    pub fn load_json_input<T>(&self, input: &InputRef<T>, label: &str) -> Result<T, ApiError>
    where
        T: Clone + DeserializeOwned,
    {
        match input {
            InputRef::Inline(inline) => Ok(inline.clone()),
            InputRef::File(file_path) => self.read_json_file(file_path, label),
            InputRef::Artifact(artifact_id) => Err(ApiError::not_implemented(
                ErrorCode::ArtifactInputNotAvailable,
                format!("artifact input is not available yet: {artifact_id}"),
            )),
        }
    }

    pub fn read_utf8_file(&self, path: &Path, label: &str) -> Result<String, ApiError> {
        let resolved = self.resolve_read_path(path)?;
        std::fs::read_to_string(&resolved).map_err(|e| {
            ApiError::bad_request(
                ErrorCode::FileIoError,
                format!(
                    "failed to read {label} file {}: {e}",
                    resolved.to_string_lossy()
                ),
            )
        })
    }

    pub fn read_json_file<T: DeserializeOwned>(
        &self,
        path: &Path,
        label: &str,
    ) -> Result<T, ApiError> {
        let content = self.read_utf8_file(path, label)?;
        serde_json::from_str(&content).map_err(|e| {
            ApiError::bad_request(
                ErrorCode::ParseError,
                format!("failed to parse {label} file {}: {e}", path.display()),
            )
        })
    }

    fn resolve_read_path(&self, path: &Path) -> Result<PathBuf, ApiError> {
        let candidate = path.canonicalize().map_err(|e| {
            ApiError::bad_request(
                ErrorCode::FileIoError,
                format!("failed to resolve path {}: {e}", path.display()),
            )
        })?;

        if self
            .allowed_roots
            .iter()
            .any(|root| candidate.starts_with(root))
        {
            Ok(candidate)
        } else {
            Err(ApiError::forbidden(
                ErrorCode::PathNotAllowed,
                format!("path is outside allowed roots: {}", candidate.display()),
            )
            .with_details(serde_json::json!({
                "path": candidate,
                "allowed_roots": self.allowed_roots,
            })))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_requires_non_empty_roots() {
        assert!(FileAccessPolicy::new(Vec::new()).is_err());
    }
}
