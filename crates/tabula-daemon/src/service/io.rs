//! Filesystem access policy for `kind=file` inputs.

use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use serde::de::DeserializeOwned;

use tabula_artifact::InputRef;

use super::error::{ServiceError, ServiceResult};
use crate::protocol::error::ErrorCode;

/// Filesystem access policy for `kind=file` inputs.
#[derive(Debug, Clone)]
pub struct FileAccessPolicy {
    allowed_roots: Vec<PathBuf>,
}

impl FileAccessPolicy {
    /// Resolve default local roots (`cwd`, temp dir, and `/tmp` on unix).
    pub fn local_default_roots() -> anyhow::Result<Vec<PathBuf>> {
        let mut roots = vec![
            std::env::current_dir().context("failed to resolve current dir")?,
            std::env::temp_dir(),
        ];
        #[cfg(unix)]
        roots.push(PathBuf::from("/tmp"));

        Self::canonicalize_roots(roots)
    }

    /// Canonicalize, validate, and deduplicate allowed roots.
    pub fn canonicalize_roots<I>(roots: I) -> anyhow::Result<Vec<PathBuf>>
    where
        I: IntoIterator<Item = PathBuf>,
    {
        let mut allowed_roots = Vec::new();
        for root in roots {
            let canonical = root.canonicalize().with_context(|| {
                format!(
                    "allowed path does not exist or is invalid: {}",
                    root.display()
                )
            })?;

            if !canonical.is_dir() {
                bail!("allowed path is not a directory: {}", canonical.display());
            }

            if !allowed_roots.iter().any(|r: &PathBuf| r == &canonical) {
                allowed_roots.push(canonical);
            }
        }

        if allowed_roots.is_empty() {
            bail!("allowed roots must not be empty");
        }

        Ok(allowed_roots)
    }

    /// Build a file access policy.
    pub fn new(allowed_roots: Vec<PathBuf>) -> anyhow::Result<Self> {
        let allowed_roots = Self::canonicalize_roots(allowed_roots)?;
        Ok(Self { allowed_roots })
    }

    /// Load JSON input from `inline` or `file` modes.
    pub fn load_json_input<T>(&self, input: &InputRef<T>, label: &str) -> ServiceResult<T>
    where
        T: Clone + DeserializeOwned,
    {
        match input {
            InputRef::Inline { inline } => Ok(inline.clone()),
            InputRef::File { file_path } => self.read_json_file(file_path, label),
            InputRef::Artifact { artifact_id } => Err(ServiceError::not_implemented(
                ErrorCode::ArtifactInputNotAvailable,
                format!("artifact input is not available yet: {artifact_id}"),
            )),
        }
    }

    /// Read UTF-8 file within allow-list.
    pub fn read_utf8_file(&self, path: &Path, label: &str) -> ServiceResult<String> {
        let resolved = self.resolve_read_path(path)?;
        std::fs::read_to_string(&resolved).map_err(|e| {
            ServiceError::bad_request(
                ErrorCode::FileIoError,
                format!(
                    "failed to read {label} file {}: {e}",
                    resolved.to_string_lossy()
                ),
            )
        })
    }

    /// Read JSON file within allow-list.
    pub fn read_json_file<T: DeserializeOwned>(
        &self,
        path: &Path,
        label: &str,
    ) -> ServiceResult<T> {
        let content = self.read_utf8_file(path, label)?;
        serde_json::from_str(&content).map_err(|e| {
            ServiceError::bad_request(
                ErrorCode::ParseError,
                format!("failed to parse {label} file {}: {e}", path.display()),
            )
        })
    }

    fn resolve_read_path(&self, path: &Path) -> ServiceResult<PathBuf> {
        let candidate = path.canonicalize().map_err(|e| {
            ServiceError::bad_request(
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
            Err(ServiceError::forbidden(
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
