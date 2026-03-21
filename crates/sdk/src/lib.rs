//! Public host SDK for Tabula.
//!
//! The SDK is the intended application-facing surface above the compiler,
//! artifact models, and runtime engine.

mod error;
mod execution;
mod program;
#[cfg(feature = "verify")]
mod proof;
mod sdk;
#[cfg(feature = "verify")]
mod verifier;

/// Safe extension surface for custom schemes and precompiles.
pub mod ext;

pub use error::SdkError;
pub use execution::Execution;
pub use program::Program;
#[cfg(feature = "verify")]
pub use proof::Proof;
pub use sdk::{Sdk, SdkBuilder};
#[cfg(feature = "verify")]
pub use verifier::Verifier;

pub use tabula_artifact::{Artifact, State, Statement, TransactionBatch, TransactionInput};
pub use tabula_compiler::{CompileDiagnostic, ProgramDefinition};
