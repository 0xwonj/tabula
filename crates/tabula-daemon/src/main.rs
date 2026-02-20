//! tabula-daemon: local HTTP daemon for the Tabula Web IDE.

mod app;
mod error;
mod model;
mod service;

use std::net::{IpAddr, SocketAddr};

use clap::Parser;
use tracing::info;

use crate::app::{AppState, build_router};

#[derive(Debug, Parser)]
#[command(name = "tabula-daemon", about = "Local API daemon for Tabula Web IDE")]
struct Cli {
    /// Host to bind the daemon to.
    #[arg(long, default_value = "127.0.0.1")]
    host: IpAddr,

    /// Port to bind the daemon to.
    #[arg(long, default_value_t = 4317)]
    port: u16,

    /// Optional bearer token required for protected endpoints.
    /// If omitted, auth is disabled.
    #[arg(long, env = "TABULA_DAEMON_TOKEN")]
    token: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();
    let app_state = AppState::new(cli.token);
    let app = build_router(app_state);

    let addr = SocketAddr::new(cli.host, cli.port);
    let listener = tokio::net::TcpListener::bind(addr).await?;

    info!(address = %addr, "tabula-daemon listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    info!("shutdown signal received");
}
