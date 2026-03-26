//! JSON input/output types for the CLI.

use tabula_sdk::State;

/// JSON representation of execution results.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct ExecutionOutput {
    /// Per-entry outcomes in batch order.
    pub tx_outcomes: Vec<serde_json::Value>,
    /// Number of committed cells read from pre-state.
    pub read_count: usize,
    /// Number of committed cells written into post-state.
    pub write_count: usize,
    /// Final committed state snapshot.
    pub state_after: State,
}

/// Deserialize a JSON file from the given path.
pub(crate) fn load_json<T: serde::de::DeserializeOwned>(
    path: &std::path::Path,
) -> anyhow::Result<T> {
    let content = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&content)?)
}

/// Serialize a value to a pretty-printed JSON file.
pub(crate) fn write_json<T: serde::Serialize>(
    path: &std::path::Path,
    value: &T,
) -> anyhow::Result<()> {
    std::fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}
