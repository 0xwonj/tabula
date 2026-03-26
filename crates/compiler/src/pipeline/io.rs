use std::path::Path;

use crate::error::{CompilerError, CompilerResult};
use crate::pipeline::types::RegisteredProgram;

/// Parse one native registered program from JSON.
pub fn parse_registered_program(
    content: &str,
    logical_path: &str,
) -> CompilerResult<RegisteredProgram> {
    serde_json::from_str(content).map_err(|source| CompilerError::ParseJson {
        path: logical_path.to_string(),
        source,
    })
}

/// Load one native registered program from disk.
pub fn load_registered_program(path: &Path) -> CompilerResult<RegisteredProgram> {
    let content = std::fs::read_to_string(path).map_err(|source| CompilerError::ReadFile {
        path: path.display().to_string(),
        source,
    })?;
    parse_registered_program(&content, &path.display().to_string())
}
