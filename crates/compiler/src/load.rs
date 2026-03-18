//! Program source loading from file system.

use std::path::Path;

use tabula_artifact::ProgramArtifact;

use crate::compile::compile_program_source;
use crate::error::{CompilerError, CompilerResult};
use crate::program::CompiledProgram;
use crate::register::{register_program_artifact, register_program_definition};
use crate::sources::ProgramDefinition;

/// Load program definitions from a `.tab` file.
pub fn load_program_definition(path: &Path) -> anyhow::Result<ProgramDefinition> {
    load_program_definition_strict(path).map_err(anyhow::Error::new)
}

/// Strict variant of [`load_program_definition`] that returns typed compiler errors.
pub fn load_program_definition_strict(path: &Path) -> CompilerResult<ProgramDefinition> {
    let source = std::fs::read_to_string(path).map_err(|source| CompilerError::ReadFile {
        path: path.display().to_string(),
        source,
    })?;
    compile_program_source(&source)
}

/// Parse program definitions from in-memory `.tab` text.
pub fn parse_program_definition(content: &str) -> CompilerResult<ProgramDefinition> {
    compile_program_source(content)
}

/// Load a sealed program artifact from JSON.
pub fn load_program_artifact(path: &Path) -> anyhow::Result<ProgramArtifact> {
    load_program_artifact_strict(path).map_err(anyhow::Error::new)
}

/// Strict variant of [`load_program_artifact`] that returns typed compiler errors.
pub fn load_program_artifact_strict(path: &Path) -> CompilerResult<ProgramArtifact> {
    let source = std::fs::read_to_string(path).map_err(|source| CompilerError::ReadFile {
        path: path.display().to_string(),
        source,
    })?;
    parse_program_artifact(&source, &path.display().to_string())
}

/// Parse a sealed program artifact from JSON text.
pub fn parse_program_artifact(
    content: &str,
    source_label: &str,
) -> CompilerResult<ProgramArtifact> {
    serde_json::from_str(content).map_err(|source| CompilerError::ParseJson {
        path: source_label.to_string(),
        source,
    })
}

/// Convenience helper: load sources from a path and register in one step.
pub fn load_and_register_program(path: &Path) -> anyhow::Result<CompiledProgram> {
    if path.extension().and_then(|e| e.to_str()) == Some("tab") {
        let definition = load_program_definition_strict(path).map_err(anyhow::Error::new)?;
        register_program_definition(&definition).map_err(anyhow::Error::new)
    } else {
        let artifact = load_program_artifact_strict(path).map_err(anyhow::Error::new)?;
        register_program_artifact(&artifact).map_err(anyhow::Error::new)
    }
}
