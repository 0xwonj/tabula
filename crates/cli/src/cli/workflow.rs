//! Top-level workflow commands.

use std::path::PathBuf;

use clap::{Subcommand, ValueEnum};

/// `tabula compile`
#[derive(Debug, Clone, clap::Args)]
pub struct CompileArgs {
    /// Path to `.tab` source.
    pub program: PathBuf,

    /// Output path for the artifact JSON.
    #[arg(short, long)]
    pub output: Option<PathBuf>,
}

/// `tabula check`
#[derive(Debug, Clone, clap::Args)]
pub struct CheckArgs {
    /// Path to `.tab` source or artifact JSON.
    pub program: PathBuf,

    /// Emit the versioned JSON contract.
    #[arg(long)]
    pub json: bool,
}

/// `tabula schema`
#[derive(Debug, Clone, clap::Args)]
pub struct SchemaArgs {
    /// Path to `.tab` source or artifact JSON.
    pub program: PathBuf,

    /// Emit the versioned JSON contract.
    #[arg(long)]
    pub json: bool,
}

/// `tabula query`
#[derive(Debug, Clone, clap::Args)]
pub struct QueryArgs {
    /// Source-level query symbol.
    pub query: String,

    /// Program source or artifact.
    #[arg(short = 'p', long)]
    pub program: PathBuf,

    /// Input state snapshot.
    #[arg(short = 's', long)]
    pub state: PathBuf,

    /// Query arguments as a JSON array.
    #[arg(long)]
    pub args: String,

    /// Public context input.
    #[arg(short = 'c', long)]
    pub context: Option<PathBuf>,

    /// Emit the versioned JSON contract.
    #[arg(long)]
    pub json: bool,
}

/// `tabula execute`
#[derive(Debug, Clone, clap::Args)]
pub struct ExecuteArgs {
    /// Program source or artifact.
    #[arg(short = 'p', long)]
    pub program: PathBuf,

    /// Input state snapshot.
    #[arg(short = 's', long)]
    pub state: PathBuf,

    /// Transaction batch JSON.
    #[arg(short = 'b', long)]
    pub batch: PathBuf,

    /// Public context input.
    #[arg(short = 'c', long)]
    pub context: Option<PathBuf>,

    /// Write the resulting state to this file.
    #[arg(long)]
    pub state_out: Option<PathBuf>,

    /// Write the versioned execution report JSON to this file.
    #[arg(long)]
    pub report_out: Option<PathBuf>,

    /// Write the binary execution receipt bridge to this file.
    #[arg(long)]
    pub receipt_out: Option<PathBuf>,

    /// Emit the versioned JSON report to stdout.
    #[arg(long)]
    pub json: bool,

    /// Show raw ids rather than symbolic names when possible.
    #[arg(long)]
    pub raw: bool,
}

/// `tabula prove`
#[cfg(feature = "prove")]
#[derive(Debug, Clone, clap::Args)]
pub struct ProveArgs {
    /// Program source or artifact.
    #[arg(short = 'p', long)]
    pub program: PathBuf,

    /// Binary execution receipt bridge produced by `execute --receipt-out`.
    #[arg(long)]
    pub receipt: PathBuf,

    /// Output path for canonical `proof.bin`.
    #[arg(long)]
    pub proof_out: PathBuf,

    /// Output path for `statement.json`.
    #[arg(long)]
    pub statement_out: PathBuf,

    /// Output path for `proof_summary.json`.
    #[arg(long)]
    pub summary_out: PathBuf,

    /// Emit the versioned JSON contract.
    #[arg(long)]
    pub json: bool,
}

/// `tabula verify`
#[cfg(feature = "verify")]
#[derive(Debug, Clone, clap::Args)]
pub struct VerifyArgs {
    /// Program source or artifact.
    #[arg(short = 'p', long)]
    pub program: PathBuf,

    /// Canonical `proof.bin` path.
    #[arg(long)]
    pub proof: PathBuf,

    /// Emit the versioned JSON contract.
    #[arg(long)]
    pub json: bool,
}

/// `tabula example`
#[derive(Debug, Clone, clap::Args)]
pub struct ExampleArgs {
    /// Which example to generate.
    #[arg(value_enum, default_value_t = ExampleName::Basic)]
    pub name: ExampleName,

    /// Output directory for the example files.
    #[arg(short, long, default_value = ".")]
    pub dir: PathBuf,
}

/// Built-in example set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ExampleName {
    /// Minimal example with one state table and one transaction.
    Basic,
    /// Membership approval example with queries and relations.
    Membership,
    /// DEX example with declarative capability bundle config.
    Dex,
}

/// `tabula env ...`
#[derive(Debug, Clone, Subcommand)]
pub enum EnvCommand {
    /// Show resolved config, extension bundles, and build feature availability.
    Doctor(EnvDoctorArgs),
}

/// `tabula env doctor`
#[derive(Debug, Clone, clap::Args)]
pub struct EnvDoctorArgs {
    /// Emit the versioned JSON contract.
    #[arg(long)]
    pub json: bool,
}
