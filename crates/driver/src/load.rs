//! Program source loading from file system.

use std::path::Path;

use crate::ProgramSourceFile;
use crate::compile::compile_program_source;
use crate::error::{DriverError, DriverResult};
use crate::register::{MetadataPolicy, RegisteredProgram, register_program_sources};

/// Program source format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgramSourceFormat {
    /// `.tab` source program.
    TabSource,
    /// JSON artifact program.
    JsonArtifact,
}

/// Load program sources from `.tab` or `.json`.
pub fn load_program_sources(path: &Path) -> anyhow::Result<ProgramSourceFile> {
    load_program_sources_strict(path).map_err(anyhow::Error::new)
}

/// Strict variant of [`load_program_sources`] that returns typed driver errors.
pub fn load_program_sources_strict(path: &Path) -> DriverResult<ProgramSourceFile> {
    let source = std::fs::read_to_string(path).map_err(|source| DriverError::ReadFile {
        path: path.display().to_string(),
        source,
    })?;

    let format = if path.extension().and_then(|e| e.to_str()) == Some("tab") {
        ProgramSourceFormat::TabSource
    } else {
        ProgramSourceFormat::JsonArtifact
    };
    parse_program_sources(&source, format, &path.display().to_string())
}

/// Parse/compile program sources from in-memory text using the given source format.
pub fn parse_program_sources(
    content: &str,
    format: ProgramSourceFormat,
    source_label: &str,
) -> DriverResult<ProgramSourceFile> {
    match format {
        ProgramSourceFormat::TabSource => compile_program_source(content),
        ProgramSourceFormat::JsonArtifact => {
            serde_json::from_str(content).map_err(|source| DriverError::ParseJson {
                path: source_label.to_string(),
                source,
            })
        }
    }
}

/// Convenience helper: load sources from a path and register in one step.
pub fn load_and_register_program(path: &Path) -> anyhow::Result<RegisteredProgram> {
    let sources = load_program_sources_strict(path).map_err(anyhow::Error::new)?;
    let metadata_policy = if path.extension().and_then(|e| e.to_str()) == Some("tab") {
        MetadataPolicy::Optional
    } else {
        MetadataPolicy::Required
    };
    register_program_sources(&sources, metadata_policy).map_err(anyhow::Error::new)
}
