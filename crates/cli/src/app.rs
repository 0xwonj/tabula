//! Shared runtime context for command handlers.

use tabula_sdk::Sdk;

/// Shared command execution context.
#[derive(Debug, Clone)]
pub(crate) struct AppContext {
    sdk: Sdk,
}

impl AppContext {
    /// Build one application context using the standard SDK environment.
    pub(crate) fn standard() -> anyhow::Result<Self> {
        Ok(Self {
            sdk: Sdk::standard()?,
        })
    }

    /// Borrow the configured SDK.
    pub(crate) const fn sdk(&self) -> &Sdk {
        &self.sdk
    }

    /// Resolve whether a command should print JSON.
    pub(crate) const fn wants_json(&self, explicit_json: bool) -> bool {
        explicit_json
    }
}
