//! Declarative CLI environment preparation.

mod bundle;
mod install;
mod status;

pub(crate) use install::PreparedEnvironment;
pub(crate) use status::EnvironmentStatus;
