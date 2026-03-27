//! Shared loading helpers for source/artifact/state/context/batch inputs.

use std::path::{Path, PathBuf};

use anyhow::Context as _;
use tabula_sdk::{Artifact, Context, Program, Sdk, State, TransactionBatch};

/// Whether a program input came from source or an artifact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProgramInputKind {
    Source,
    Artifact,
}

/// Opened program plus its loaded artifact and input classification.
#[derive(Debug, Clone)]
pub(crate) struct LoadedProgram {
    pub(crate) artifact: Artifact,
    pub(crate) program: Program,
}

/// Load and compile or deserialize a program input.
pub(crate) fn load_program(sdk: &Sdk, program_path: &Path) -> anyhow::Result<LoadedProgram> {
    let (artifact, _) = load_artifact(sdk, program_path)?;
    let program = sdk
        .open(artifact.clone())
        .with_context(|| format!("failed to open program {}", program_path.display()))?;
    Ok(LoadedProgram { artifact, program })
}

/// Load an artifact from either a `.tab` source file or a serialized artifact.
pub(crate) fn load_artifact(
    sdk: &Sdk,
    program_path: &Path,
) -> anyhow::Result<(Artifact, ProgramInputKind)> {
    if program_path.extension().and_then(|ext| ext.to_str()) == Some("tab") {
        let source = std::fs::read_to_string(program_path)
            .with_context(|| format!("failed to read program source {}", program_path.display()))?;
        let artifact = sdk.compile(&source).with_context(|| {
            format!(
                "failed to compile source program {}",
                program_path.display()
            )
        })?;
        return Ok((artifact, ProgramInputKind::Source));
    }

    let artifact: Artifact = load_json(program_path)?;
    Ok((artifact, ProgramInputKind::Artifact))
}

/// Deserialize one JSON file with contextual diagnostics.
pub(crate) fn load_json<T: serde::de::DeserializeOwned>(path: &Path) -> anyhow::Result<T> {
    let bytes = std::fs::read(path)
        .with_context(|| format!("failed to read JSON file {}", path.display()))?;
    serde_json::from_slice(&bytes)
        .with_context(|| format!("failed to parse JSON file {}", path.display()))
}

/// Load one state snapshot from JSON.
pub(crate) fn load_state(path: &Path) -> anyhow::Result<State> {
    load_json(path)
}

/// Load one public context input from JSON or default to an empty object.
pub(crate) fn load_context(path: Option<&Path>) -> anyhow::Result<Context> {
    match path {
        Some(path) => load_json(path),
        None => Ok(Context::default()),
    }
}

/// Load one transaction batch from JSON.
pub(crate) fn load_batch(path: &Path) -> anyhow::Result<TransactionBatch> {
    load_json(path)
}

/// Build the default artifact output path for one source file.
pub(crate) fn default_artifact_output(program_path: &Path) -> PathBuf {
    program_path.with_extension("json")
}
