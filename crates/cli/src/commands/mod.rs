//! CLI command handlers.

pub mod batch;
pub mod check;
pub mod compile;
pub mod context;
pub mod env;
pub mod example;
pub mod execute;
#[cfg(feature = "prove")]
pub mod prove;
pub mod query;
pub mod schema;
pub mod state;
#[cfg(feature = "verify")]
pub mod verify;
