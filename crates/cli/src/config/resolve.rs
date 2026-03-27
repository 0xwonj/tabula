//! Config file parsing and path resolution.

use std::path::{Path, PathBuf};

use anyhow::{Context as _, anyhow};

use super::file::{EnvironmentConfig, OutputConfig, OutputFormat, ResolvedConfig};

#[derive(Debug, Clone, Default, serde::Deserialize)]
struct FileConfig {
    #[serde(default)]
    environment: FileEnvironmentConfig,
    #[serde(default)]
    output: FileOutputConfig,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
struct FileEnvironmentConfig {
    #[serde(default)]
    extensions: Vec<PathBuf>,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
struct FileOutputConfig {
    format: Option<FileOutputFormat>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum FileOutputFormat {
    Human,
    Json,
}

impl ResolvedConfig {
    /// Load the nearest project-local config, or return an empty config.
    pub(crate) fn load(cwd: &Path, override_path: Option<&Path>) -> anyhow::Result<Self> {
        let config_path = match override_path {
            Some(path) => Some(absolutize(cwd, path)),
            None => find_upward(cwd, "tabula.toml")?,
        };

        let Some(path) = config_path else {
            return Ok(Self::default());
        };

        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read config file {}", path.display()))?;
        let parsed: FileConfig = toml::from_str(&content)
            .with_context(|| format!("failed to parse config file {}", path.display()))?;
        let base_dir = path
            .parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| anyhow!("config path {} has no parent directory", path.display()))?;

        Ok(Self {
            path: Some(path),
            environment: EnvironmentConfig {
                extensions: parsed
                    .environment
                    .extensions
                    .iter()
                    .map(|extension| absolutize(&base_dir, extension))
                    .collect(),
            },
            output: OutputConfig {
                format: parsed.output.format.map(|format| match format {
                    FileOutputFormat::Human => OutputFormat::Human,
                    FileOutputFormat::Json => OutputFormat::Json,
                }),
            },
        })
    }
}

fn find_upward(start: &Path, filename: &str) -> anyhow::Result<Option<PathBuf>> {
    let mut current = absolutize(start, Path::new("."));
    loop {
        let candidate = current.join(filename);
        if candidate.is_file() {
            return Ok(Some(candidate));
        }
        let Some(parent) = current.parent() else {
            return Ok(None);
        };
        if parent == current {
            return Ok(None);
        }
        current = parent.to_path_buf();
    }
}

fn absolutize(base: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    }
}

#[cfg(test)]
mod tests {
    use super::ResolvedConfig;

    #[test]
    fn missing_config_falls_back_to_empty() {
        let temp = temp_path("missing");
        std::fs::create_dir_all(&temp).unwrap();
        let config = ResolvedConfig::load(&temp, None).unwrap();
        assert!(config.path.is_none());
        assert!(config.environment.extensions.is_empty());
    }

    #[test]
    fn upward_search_finds_nearest_file() {
        let root = temp_path("upward");
        let nested = root.join("a/b/c");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(root.join("tabula.toml"), "[environment]\nextensions=[]\n").unwrap();

        let config = ResolvedConfig::load(&nested, None).unwrap();
        assert_eq!(config.path, Some(root.join("tabula.toml")));
    }

    #[test]
    fn explicit_override_wins() {
        let root = temp_path("override");
        let nested = root.join("a/b");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(root.join("tabula.toml"), "[environment]\nextensions=[]\n").unwrap();
        let other = root.join("other.toml");
        std::fs::write(&other, "[environment]\nextensions=[\"./bundle.toml\"]\n").unwrap();

        let config = ResolvedConfig::load(&nested, Some(&other)).unwrap();
        assert_eq!(config.path, Some(other.clone()));
        assert_eq!(
            config.environment.extensions,
            vec![root.join("bundle.toml")]
        );
    }

    fn temp_path(label: &str) -> std::path::PathBuf {
        let base = std::env::temp_dir().join(format!(
            "tabula-cli-config-tests-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        if base.exists() {
            std::fs::remove_dir_all(&base).unwrap();
        }
        base
    }
}
