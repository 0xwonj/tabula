//! tabula-daemon library.
//!
//! Client-neutral local API control plane for Tabula engine operations.

mod api;
mod kernel;
mod protocol;
pub mod runtime;

pub use protocol::error::{ApiError, ApiResult};
pub use protocol::types::*;
pub use runtime::config::{Cli, ServerConfig};

use std::sync::Arc;

use anyhow::Context;
use tracing::info;

use crate::kernel::engine::TabulaEngine;
use crate::kernel::io::FileAccessPolicy;
use crate::runtime::state::AppState;

/// Run the daemon server until shutdown.
pub async fn run(config: ServerConfig) -> anyhow::Result<()> {
    let file_policy = FileAccessPolicy::new(config.allowed_roots.clone())
        .context("failed to build file access policy")?;
    let engine = Arc::new(TabulaEngine::new(file_policy));
    let state = Arc::new(AppState::new(config, engine));

    let app = api::router::build_router(state.clone());
    let listener = tokio::net::TcpListener::bind(state.bind_addr())
        .await
        .with_context(|| format!("failed to bind {}", state.bind_addr()))?;

    info!(address = %state.bind_addr(), "tabula-daemon listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(runtime::shutdown::shutdown_signal())
        .await
        .context("server failed")
}
