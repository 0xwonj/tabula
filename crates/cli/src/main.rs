//! Tabula CLI: compile, check, and execute Tabula programs.

mod commands;
mod io;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "tabula", about = "Tabula kernel CLI")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Compile a .tab program to IR JSON.
    Compile {
        /// Path to .tab source file.
        program: PathBuf,

        /// Output path (default: replace .tab with .json).
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Validate a program (syntax, types, normal form).
    Check {
        /// Path to program (.tab or .json).
        program: PathBuf,
    },

    /// Execute a batch of transactions.
    Execute {
        /// Path to program definition (JSON or .tab).
        #[arg(short, long)]
        program: PathBuf,

        /// Path to initial state (JSON).
        #[arg(short, long)]
        state: PathBuf,

        /// Path to transaction batch (JSON).
        #[arg(short, long)]
        batch: PathBuf,

        /// Path to public context input (JSON).
        #[arg(short = 'c', long)]
        context: Option<PathBuf>,

        /// Write the resulting state to this file.
        #[arg(short, long)]
        output_state: Option<PathBuf>,

        /// Output results as JSON instead of human-readable text.
        #[arg(long)]
        json: bool,
    },

    /// Inspect a state file.
    Inspect {
        /// Path to state file (JSON).
        #[arg(short, long)]
        state: PathBuf,

        /// Filter by table ID.
        #[arg(short, long)]
        table: Option<u32>,
    },

    /// Generate example files in the specified directory.
    Example {
        /// Output directory for example files (default: current directory).
        #[arg(short, long, default_value = ".")]
        dir: PathBuf,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Compile { program, output } => {
            commands::compile::cmd_compile(&program, output.as_deref())
        }
        Command::Check { program } => commands::check::cmd_check(&program),
        Command::Execute {
            program,
            state,
            batch,
            context,
            output_state,
            json,
        } => commands::execute::cmd_execute(
            &program,
            &state,
            &batch,
            context.as_deref(),
            output_state.as_deref(),
            json,
        ),
        Command::Inspect { state, table } => commands::inspect::cmd_inspect(&state, table),
        Command::Example { dir } => commands::example::cmd_example(&dir),
    }
}
