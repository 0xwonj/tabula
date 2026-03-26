//! Public host SDK for Tabula.
//!
//! The SDK is the application-facing surface above the compiler and runtime.

pub mod advanced;
mod artifact;
mod batch;
mod context;
mod environment;
mod error;
mod program;
#[cfg(feature = "verify")]
mod proof;
#[cfg(feature = "execute")]
mod runner;
mod schema;
mod sdk;
mod state;
mod value;
#[cfg(feature = "verify")]
mod verifier;

pub use artifact::Artifact;
pub use batch::TransactionBatch;
pub use context::Context;
pub use environment::Environment;
pub use error::{InstallError, SdkError};
pub use program::{ContextBuilder, Program, StateBuilder, TransactionBatchBuilder};
#[cfg(feature = "verify")]
pub use proof::Proof;
#[cfg(feature = "execute")]
pub use runner::{ExecutionReceipt, QueryResult, Runner, TxOutcomeSummary};
pub use schema::{
    ContextFieldHandle, FieldHandle, ParameterHandle, QueryHandle, Schema, TableHandle, TxHandle,
};
pub use sdk::{Sdk, SdkBuilder};
pub use state::State;
#[cfg(feature = "verify")]
pub use tabula_runtime::ProofStatement as Statement;
#[cfg(feature = "verify")]
pub use verifier::Verifier;
