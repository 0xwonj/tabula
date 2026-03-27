//! Shared runtime context for command handlers.

use std::path::Path;

use tabula_sdk::Sdk;

use crate::config::{OutputFormat, ResolvedConfig};
use crate::environment::PreparedEnvironment;

/// Shared command execution context.
#[derive(Debug, Clone)]
pub(crate) struct AppContext {
    config: ResolvedConfig,
    environment: PreparedEnvironment,
}

impl AppContext {
    /// Build one application context from the current working directory.
    pub(crate) fn load(cwd: &Path, config_override: Option<&Path>) -> anyhow::Result<Self> {
        let config = ResolvedConfig::load(cwd, config_override)?;
        let environment = PreparedEnvironment::prepare(&config)?;
        Ok(Self {
            config,
            environment,
        })
    }

    /// Borrow the configured SDK or fail if the environment is unusable.
    pub(crate) fn sdk(&self) -> anyhow::Result<&Sdk> {
        self.environment.sdk.as_ref().ok_or_else(|| {
            let detail = self
                .environment
                .status
                .build_error
                .clone()
                .unwrap_or_else(|| "SDK environment is not available".to_string());
            anyhow::anyhow!(detail)
        })
    }

    /// Borrow the prepared environment status.
    pub(crate) fn environment_status(&self) -> &crate::environment::EnvironmentStatus {
        &self.environment.status
    }

    /// Resolve whether a command should print JSON.
    pub(crate) fn wants_json(&self, explicit_json: bool) -> bool {
        explicit_json || self.config.output.format == Some(OutputFormat::Json)
    }
}
