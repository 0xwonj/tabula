//! Clap surface for the CLI.

mod authoring;
mod workflow;

use clap::{Parser, Subcommand};
#[cfg(feature = "prove")]
pub(crate) use workflow::ProveArgs;
#[cfg(feature = "verify")]
pub(crate) use workflow::VerifyArgs;
pub(crate) use workflow::{
    CheckArgs, CompileArgs, EnvDoctorArgs, ExampleArgs, ExampleName, ExecuteArgs, QueryArgs,
    SchemaArgs,
};

pub(crate) use authoring::{
    BatchCallArgs, BatchCommand, BatchInitArgs, ContextCommand, ContextInitArgs, ContextSetArgs,
    StateCommand, StateInitArgs, StateInspectArgs, StateSetArgs,
};

/// Top-level CLI arguments.
#[derive(Debug, Parser)]
#[command(name = "tabula", about = "Tabula CLI")]
pub struct Cli {
    /// Path to an explicit `tabula.toml` config file.
    #[arg(long, global = true)]
    pub config: Option<std::path::PathBuf>,

    /// Requested top-level command.
    #[command(subcommand)]
    pub command: Command,
}

/// Top-level command surface for the external beta CLI.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Compile a .tab program to a sealed artifact JSON file.
    Compile(CompileArgs),
    /// Validate a program and print a schema-aware summary.
    Check(CheckArgs),
    /// Print full static schema details for a source or artifact.
    Schema(SchemaArgs),
    /// Execute one read-only query against a state snapshot.
    Query(QueryArgs),
    /// Execute a batch of transactions.
    Execute(ExecuteArgs),
    /// Generate one proof from a receipt bridge.
    #[cfg(feature = "prove")]
    Prove(ProveArgs),
    /// Verify one proof against a selected program.
    #[cfg(feature = "verify")]
    Verify(VerifyArgs),
    /// Create and edit state files.
    State {
        #[command(subcommand)]
        command: StateCommand,
    },
    /// Create and edit public context files.
    Context {
        #[command(subcommand)]
        command: ContextCommand,
    },
    /// Create and edit transaction batch files.
    Batch {
        #[command(subcommand)]
        command: BatchCommand,
    },
    /// Generate example files in the specified directory.
    Example(ExampleArgs),
    /// Inspect the resolved CLI environment.
    Env {
        #[command(subcommand)]
        command: workflow::EnvCommand,
    },
}

pub(crate) use workflow::EnvCommand;
