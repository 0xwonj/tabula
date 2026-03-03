//! tabula-daemon entrypoint.

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    use clap::Parser;

    let cli = tabula_daemon::Cli::parse();
    let config = cli.to_server_config()?;
    tabula_daemon::run(config).await
}
