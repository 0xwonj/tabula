//! Official extension authoring surface for Tabula.
//!
//! Package consumers use `tabula-sdk`, while extension authors implement
//! custom schemes and semantic precompiles against `tabula-ext`.

/// Advanced backend-facing support for extension authors.
pub mod backend;
mod error;
/// Semantic precompile authoring contracts and bundle types.
pub mod precompile;
/// Column commitment scheme authoring contracts and bundle types.
pub mod scheme;

pub use error::{ExtError, ExtResult};
pub use precompile::{PrecompileBundle, PrecompileDescriptor, PrecompileId};
pub use scheme::{
    ColumnLayoutKind, ColumnSchemeFactory, PropertyQueryKind, ResolvedColumnPlan, RootProfileId,
    RuntimeColumn, SchemeBundle, SchemeDescriptor, SchemeId, Value, ValueType,
};
