//! Public host SDK for Tabula.
//!
//! The SDK is the application-facing surface above the compiler and runtime.

mod builder;
mod environment;
mod error;
pub mod interop;
mod program;
mod sdk;
mod types;
mod value;

pub use builder::SdkBuilder;
pub use environment::Environment;
pub use error::{InstallError, SdkError};
#[cfg(feature = "verify")]
pub use program::Verifier;
pub use program::{
    Artifact, ContextBuilder, ContextFieldHandle, FieldHandle, ParameterHandle, Program,
    QueryHandle, Schema, StateBuilder, TableHandle, TransactionBatchBuilder, TxHandle,
};
#[cfg(feature = "execute")]
pub use program::{ExecutionReceipt, QueryResult, Runner, TxOutcomeSummary};
pub use sdk::Sdk;
#[cfg(feature = "verify")]
pub use tabula_runtime::ProofStatement as Statement;
#[cfg(feature = "verify")]
pub use types::Proof;
pub use types::{Context, State, TransactionBatch};
pub use value::{DecodeValue, EncodeArgs, EncodeValue};
