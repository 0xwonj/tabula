//! Internal environment status model.

/// One parsed extension bundle status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExtensionBundleStatus {
    pub(crate) path: String,
    pub(crate) name: String,
    pub(crate) capability_paths: Vec<String>,
    pub(crate) unsupported_entries: Vec<String>,
}

/// Internal environment status used by projection/rendering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EnvironmentStatus {
    pub(crate) config_path: Option<String>,
    pub(crate) sdk_ready: bool,
    pub(crate) build_error: Option<String>,
    pub(crate) extensions: Vec<ExtensionBundleStatus>,
    pub(crate) verify_feature_enabled: bool,
    pub(crate) prove_feature_enabled: bool,
}
