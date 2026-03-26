//! Rewritten source frontend for Tabula programs.

mod build;
mod syntax;
mod verify;

pub mod ast;
pub mod error;
pub mod hir;
pub mod span;

pub use build::{CapabilityPreludeEntry, FrontendPrelude, build_hir, compile_to_hir};
pub use error::{FrontendError, FrontendErrorKind};
pub use syntax::parse_program;
pub use verify::verify_hir;
