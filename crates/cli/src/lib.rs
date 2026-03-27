//! Product-facing CLI for Tabula.

mod app;
mod cli;
mod commands;
mod config;
mod environment;
mod handoff;
mod io;
mod output;

use clap::Parser as _;

pub use cli::Cli;

/// Run the CLI using process arguments.
pub fn run() -> anyhow::Result<()> {
    run_cli(Cli::parse())
}

/// Run the CLI from one already-parsed clap value.
pub fn run_cli(cli: Cli) -> anyhow::Result<()> {
    let cwd = std::env::current_dir()?;
    let ctx = app::AppContext::load(&cwd, cli.config.as_deref())?;

    match cli.command {
        cli::Command::Compile(args) => commands::compile::run(&ctx, &args),
        cli::Command::Check(args) => commands::check::run(&ctx, &args),
        cli::Command::Schema(args) => commands::schema::run(&ctx, &args),
        cli::Command::Query(args) => commands::query::run(&ctx, &args),
        cli::Command::Execute(args) => commands::execute::run(&ctx, &args),
        #[cfg(feature = "prove")]
        cli::Command::Prove(args) => commands::prove::run(&ctx, &args),
        #[cfg(feature = "verify")]
        cli::Command::Verify(args) => commands::verify::run(&ctx, &args),
        cli::Command::State { command } => commands::state::run(&ctx, &command),
        cli::Command::Context { command } => commands::context::run(&ctx, &command),
        cli::Command::Batch { command } => commands::batch::run(&ctx, &command),
        cli::Command::Example(args) => commands::example::run(&ctx, &args),
        cli::Command::Env { command } => commands::env::run(&ctx, &command),
    }
}
