//! Authoring namespace commands.

use std::path::PathBuf;

use clap::Subcommand;

/// `tabula state ...`
#[derive(Debug, Clone, Subcommand)]
pub enum StateCommand {
    /// Create an empty state snapshot for a program.
    Init(StateInitArgs),
    /// Set one state cell value by source symbol.
    Set(StateSetArgs),
    /// Inspect a state snapshot.
    Inspect(StateInspectArgs),
}

/// `tabula state init`
#[derive(Debug, Clone, clap::Args)]
pub struct StateInitArgs {
    /// Program source or artifact.
    #[arg(short = 'p', long)]
    pub program: PathBuf,

    /// Output path for the state file.
    #[arg(long)]
    pub out: PathBuf,
}

/// `tabula state set`
#[derive(Debug, Clone, clap::Args)]
pub struct StateSetArgs {
    /// Program source or artifact.
    #[arg(short = 'p', long)]
    pub program: PathBuf,

    /// Existing state file to update.
    #[arg(short = 's', long)]
    pub state: PathBuf,

    /// Source-level table symbol.
    pub table: String,

    /// Row key.
    pub row: u64,

    /// Source-level field symbol.
    pub field: String,

    /// JSON literal carrying the new value.
    pub value: String,

    /// Optional alternate output path. Defaults to in-place update.
    #[arg(long)]
    pub out: Option<PathBuf>,
}

/// `tabula state inspect`
#[derive(Debug, Clone, clap::Args)]
pub struct StateInspectArgs {
    /// State file to inspect.
    #[arg(short = 's', long)]
    pub state: PathBuf,

    /// Program source or artifact for symbolic rendering.
    #[arg(short = 'p', long)]
    pub program: Option<PathBuf>,

    /// Optional table symbol filter when `--program` is supplied.
    #[arg(long)]
    pub table: Option<String>,

    /// Emit the versioned JSON contract.
    #[arg(long)]
    pub json: bool,

    /// Show raw ids rather than symbolic names when possible.
    #[arg(long)]
    pub raw: bool,
}

/// `tabula context ...`
#[derive(Debug, Clone, Subcommand)]
pub enum ContextCommand {
    /// Create an empty public context input for a program.
    Init(ContextInitArgs),
    /// Set one public context field by source symbol.
    Set(ContextSetArgs),
}

/// `tabula context init`
#[derive(Debug, Clone, clap::Args)]
pub struct ContextInitArgs {
    /// Program source or artifact.
    #[arg(short = 'p', long)]
    pub program: PathBuf,

    /// Output path for the context file.
    #[arg(long)]
    pub out: PathBuf,
}

/// `tabula context set`
#[derive(Debug, Clone, clap::Args)]
pub struct ContextSetArgs {
    /// Program source or artifact.
    #[arg(short = 'p', long)]
    pub program: PathBuf,

    /// Existing context file to update.
    #[arg(short = 'c', long)]
    pub context: PathBuf,

    /// Source-level context field symbol.
    pub field: String,

    /// JSON literal carrying the new value.
    pub value: String,

    /// Optional alternate output path. Defaults to in-place update.
    #[arg(long)]
    pub out: Option<PathBuf>,
}

/// `tabula batch ...`
#[derive(Debug, Clone, Subcommand)]
pub enum BatchCommand {
    /// Create an empty transaction batch.
    Init(BatchInitArgs),
    /// Append one transaction call to an existing batch.
    Call(BatchCallArgs),
}

/// `tabula batch init`
#[derive(Debug, Clone, clap::Args)]
pub struct BatchInitArgs {
    /// Output path for the batch file.
    #[arg(long)]
    pub out: PathBuf,
}

/// `tabula batch call`
#[derive(Debug, Clone, clap::Args)]
pub struct BatchCallArgs {
    /// Program source or artifact.
    #[arg(short = 'p', long)]
    pub program: PathBuf,

    /// Existing batch file to update.
    #[arg(short = 'b', long)]
    pub batch: PathBuf,

    /// Source-level transaction symbol.
    pub tx: String,

    /// Arguments as a JSON array.
    #[arg(long)]
    pub args: String,

    /// Optional alternate output path. Defaults to in-place update.
    #[arg(long)]
    pub out: Option<PathBuf>,
}
