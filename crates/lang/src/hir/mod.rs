//! High-level Intermediate Representation for verified Tabula programs.
//!
//! The HIR is produced by the build phase from the AST and carries resolved
//! identifiers, semantic IDs, and type information.

/// Stable numeric identifiers used throughout the HIR.
pub mod ids;
/// HIR model types (compiler-internal; resolved program representation).
pub mod model;

pub use ids::*;
pub use model::*;
