//! JSON input/output types for the CLI.

use tabula_artifact::{State as ArtifactState, StateEntry as ArtifactStateEntry};
use tabula_core::{AccessEvent, EmittedEvent, ExecutionConsistencyStatus, TxResult};

/// JSON representation of state.
pub type State = ArtifactState;
/// JSON representation of a state entry.
pub type StateEntry = ArtifactStateEntry;
/// JSON representation of execution results.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct ExecutionOutput {
    /// Per-transaction results.
    pub tx_results: Vec<TxResult>,
    /// Cells read from committed state.
    pub read_set: Vec<StateEntry>,
    /// Final writes to committed state.
    pub write_set: Vec<StateEntry>,
    /// Emitted application events.
    pub emitted: Vec<EmittedEvent>,
    /// Typed consistency check result.
    pub consistency: ExecutionConsistencyStatus,
    /// Full execution trace (only if requested).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace: Option<Vec<AccessEvent>>,
}

/// Deserialize a JSON file from the given path.
pub(crate) fn load_json<T: serde::de::DeserializeOwned>(
    path: &std::path::Path,
) -> anyhow::Result<T> {
    Ok(tabula_artifact::load_json(path)?)
}

/// Serialize a value to a pretty-printed JSON file.
pub(crate) fn write_json<T: serde::Serialize>(
    path: &std::path::Path,
    value: &T,
) -> anyhow::Result<()> {
    Ok(tabula_artifact::write_json(path, value)?)
}
