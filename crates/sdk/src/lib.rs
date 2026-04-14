//! Public host SDK for Tabula.
//!
//! The SDK is the application-facing surface above the compiler and runtime.
//! Verification is statement-first: applications exchange [`PublicStatement`]
//! and `public_statement.json`, while the runtime reconstructs artifact-bound
//! verifier state internally.

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
    Artifact, ContextBuilder, ContextFieldHandle, FieldHandle, KeyComponentHandle, ParameterHandle,
    Program, QueryHandle, Schema, StateBuilder, TableHandle, TransactionBatchBuilder, TxHandle,
};
#[cfg(feature = "execute")]
pub use program::{ExecutionReceipt, QueryResult, Runner, TxOutcomeSummary};
pub use sdk::Sdk;
#[cfg(feature = "verify")]
pub use tabula_contract::{BoundStatement, PublicStatement};
#[cfg(feature = "verify")]
pub use types::Proof;
pub use types::{Context, State, TransactionBatch};
#[cfg(any(feature = "prove", feature = "verify"))]
pub use types::{PublicStatementFile, PublicStatementFileError};
pub use value::{DecodeValue, EncodeArgs, EncodeValue};
