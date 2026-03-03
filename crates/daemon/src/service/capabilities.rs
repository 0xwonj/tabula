//! Daemon-local capability types.

use serde::{Deserialize, Serialize};

/// Capability input modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityInputMode {
    /// Inline mode.
    Inline,
    /// File mode.
    File,
    /// Artifact mode.
    Artifact,
}

/// Supported client kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityClientKind {
    /// Web IDE client.
    WebIde,
    /// CLI client.
    Cli,
    /// Automation client.
    Automation,
}

/// Service capabilities.
#[derive(Debug, Clone, Serialize)]
pub struct Capabilities {
    /// Service role name.
    pub service_role: &'static str,
    /// Supported clients.
    pub clients: Vec<CapabilityClientKind>,
    /// Program registration support.
    pub register_program: bool,
    /// Stateful instance creation support.
    pub create_instance: bool,
    /// Run submission support.
    pub submit_run: bool,
    /// Proof generation support during run submission.
    pub prove: bool,
    /// Proof verification support for completed runs.
    pub verify: bool,
    /// Program listing/fetch support.
    pub list_programs: bool,
    /// Instance listing/fetch support.
    pub list_instances: bool,
    /// Run listing/fetch support.
    pub run_history: bool,
    /// Supported input modes.
    pub input_modes: Vec<CapabilityInputMode>,
}
