//! Config-driven SDK environment installation.

use tabula_sdk::Sdk;
use tabula_sdk::interop::SdkBuilderExt;

use crate::config::ResolvedConfig;

use super::bundle::load_bundle;
use super::status::EnvironmentStatus;

/// Prepared CLI environment plus installation status.
#[derive(Debug, Clone)]
pub(crate) struct PreparedEnvironment {
    pub(crate) sdk: Option<Sdk>,
    pub(crate) status: EnvironmentStatus,
}

pub(crate) fn prepare_environment(config: &ResolvedConfig) -> anyhow::Result<PreparedEnvironment> {
    let mut builder = Sdk::builder();
    let mut extensions = Vec::new();
    let mut build_error = None;

    for path in &config.environment.extensions {
        let parsed = load_bundle(path)?;
        if !parsed.status.unsupported_entries.is_empty() && build_error.is_none() {
            build_error = Some(format!(
                "extension bundle {} uses unsupported declarative sections: {}",
                path.display(),
                parsed.status.unsupported_entries.join(", ")
            ));
        }
        if build_error.is_none() {
            for descriptor in &parsed.capabilities {
                builder = builder.with_capability_descriptor(descriptor.clone())?;
            }
        }
        extensions.push(parsed.status);
    }

    let sdk = if build_error.is_some() {
        None
    } else {
        match builder.build() {
            Ok(sdk) => Some(sdk),
            Err(error) => {
                build_error = Some(error.to_string());
                None
            }
        }
    };

    let sdk_ready = sdk.is_some();
    Ok(PreparedEnvironment {
        sdk,
        status: EnvironmentStatus {
            config_path: config
                .path
                .as_ref()
                .map(|path: &std::path::PathBuf| path.display().to_string()),
            sdk_ready,
            build_error,
            extensions,
            verify_feature_enabled: cfg!(feature = "verify"),
            prove_feature_enabled: cfg!(feature = "prove"),
        },
    })
}

impl PreparedEnvironment {
    /// Build one CLI environment from resolved config.
    pub(crate) fn prepare(config: &ResolvedConfig) -> anyhow::Result<Self> {
        prepare_environment(config)
    }
}
