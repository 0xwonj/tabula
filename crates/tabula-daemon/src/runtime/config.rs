use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, bail};
use clap::Parser;
use http::HeaderValue;

/// CLI options for `tabula-daemon`.
#[derive(Debug, Clone, Parser)]
#[command(name = "tabula-daemon", about = "Local API daemon for Tabula clients")]
pub struct Cli {
    /// Host to bind the daemon to.
    #[arg(long, default_value = "127.0.0.1")]
    pub host: IpAddr,

    /// Port to bind the daemon to.
    #[arg(long, default_value_t = 4317)]
    pub port: u16,

    /// Optional bearer token required for protected endpoints.
    /// If omitted, auth is disabled.
    #[arg(long, env = "TABULA_DAEMON_TOKEN")]
    pub token: Option<String>,

    /// Allowed file root for `kind=file` input.
    /// Can be repeated.
    #[arg(long = "allow-path")]
    pub allow_paths: Vec<PathBuf>,

    /// Allowed CORS origins.
    /// Can be repeated, example: --allow-origin https://play.tabula.dev
    #[arg(long = "allow-origin")]
    pub allow_origins: Vec<String>,

    /// Maximum accepted HTTP request body size in bytes.
    #[arg(long, default_value_t = 4 * 1024 * 1024)]
    pub max_body_bytes: usize,

    /// Maximum number of concurrent blocking engine jobs.
    #[arg(long, default_value_t = 8)]
    pub max_concurrent_jobs: usize,

    /// Maximum wait time to acquire a job slot.
    #[arg(long, default_value_t = 2_000)]
    pub queue_timeout_ms: u64,

    /// Maximum execution time for a single request.
    #[arg(long, default_value_t = 30_000)]
    pub request_timeout_ms: u64,
}

/// Runtime server configuration.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub bind_addr: SocketAddr,
    pub auth_token: Option<String>,
    pub allowed_roots: Vec<PathBuf>,
    pub allow_origins: Vec<HeaderValue>,
    pub max_body_bytes: usize,
    pub max_concurrent_jobs: usize,
    pub queue_timeout: Duration,
    pub request_timeout: Duration,
}

impl Cli {
    /// Convert CLI options into validated runtime config.
    pub fn to_server_config(self) -> anyhow::Result<ServerConfig> {
        let bind_addr = SocketAddr::new(self.host, self.port);

        if self.max_body_bytes == 0 {
            bail!("max_body_bytes must be greater than 0");
        }
        if self.max_concurrent_jobs == 0 {
            bail!("max_concurrent_jobs must be greater than 0");
        }
        if self.queue_timeout_ms == 0 {
            bail!("queue_timeout_ms must be greater than 0");
        }
        if self.request_timeout_ms == 0 {
            bail!("request_timeout_ms must be greater than 0");
        }

        let roots = if self.allow_paths.is_empty() {
            let mut defaults = vec![
                std::env::current_dir().context("failed to resolve current dir")?,
                std::env::temp_dir(),
            ];
            #[cfg(unix)]
            defaults.push(PathBuf::from("/tmp"));
            defaults
        } else {
            self.allow_paths
        };

        let mut allowed_roots = Vec::new();
        for root in roots {
            let canon = root.canonicalize().with_context(|| {
                format!(
                    "allowed path does not exist or is invalid: {}",
                    root.display()
                )
            })?;
            if !canon.is_dir() {
                bail!("allowed path is not a directory: {}", canon.display());
            }
            if !allowed_roots.iter().any(|r: &PathBuf| r == &canon) {
                allowed_roots.push(canon);
            }
        }

        let default_origins = vec![
            "http://127.0.0.1:3000",
            "http://localhost:3000",
            "http://127.0.0.1:5173",
            "http://localhost:5173",
        ];
        let origin_strings: Vec<String> = if self.allow_origins.is_empty() {
            default_origins
                .into_iter()
                .map(ToString::to_string)
                .collect()
        } else {
            self.allow_origins
        };

        let mut allow_origins = Vec::new();
        for origin in origin_strings {
            let hv = HeaderValue::from_str(&origin)
                .with_context(|| format!("invalid allow-origin value: {origin}"))?;
            allow_origins.push(hv);
        }

        Ok(ServerConfig {
            bind_addr,
            auth_token: self.token,
            allowed_roots,
            allow_origins,
            max_body_bytes: self.max_body_bytes,
            max_concurrent_jobs: self.max_concurrent_jobs,
            queue_timeout: Duration::from_millis(self.queue_timeout_ms),
            request_timeout: Duration::from_millis(self.request_timeout_ms),
        })
    }
}
