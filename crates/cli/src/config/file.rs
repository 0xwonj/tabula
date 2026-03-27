//! Resolved config model.

/// Resolved CLI configuration with fully qualified extension paths.
#[derive(Debug, Clone, Default)]
pub(crate) struct ResolvedConfig {
    pub(crate) path: Option<std::path::PathBuf>,
    pub(crate) environment: EnvironmentConfig,
    pub(crate) output: OutputConfig,
}

/// Resolved environment-related configuration.
#[derive(Debug, Clone, Default)]
pub(crate) struct EnvironmentConfig {
    pub(crate) extensions: Vec<std::path::PathBuf>,
}

/// Resolved output defaults.
#[derive(Debug, Clone, Default)]
pub(crate) struct OutputConfig {
    pub(crate) format: Option<OutputFormat>,
}

/// Output format override from config.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OutputFormat {
    Human,
    Json,
}
