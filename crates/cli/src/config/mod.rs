//! Project-local config discovery and parsing.

mod file;
mod resolve;

pub(crate) use file::{OutputFormat, ResolvedConfig};
