//! Official extension authoring surface for Tabula.
//!
//! Package consumers use `tabula-sdk`, while extension authors implement
//! custom schemes and semantic precompiles against `tabula-ext`.

/// Advanced backend-facing support for extension authors.
pub mod backend;
mod error;
/// Semantic precompile authoring contracts and backend bundle types.
pub mod precompile;
/// Root backend authoring and root witness preparation contracts.
pub mod root;
/// Column commitment scheme authoring contracts and bundle types.
pub mod scheme;

#[cfg(feature = "verify")]
pub use backend::precompile::PrecompileBackendFactory;
pub use error::{ExtError, ExtResult};
#[cfg(feature = "verify")]
pub use precompile::PrecompileBackendFactoryBundle;
pub use precompile::{
    PrecompileDescriptor, PrecompileId, PrecompileSignature, PrecompileValueProfile,
};
#[cfg(feature = "verify")]
pub use scheme::{
    ColumnBackendFactory, ColumnBackendFactoryBundle, ColumnBackendSetup, ColumnVerifierContract,
    MaterializedColumnBackend, RootBindingContract,
};
pub use scheme::{ColumnLayoutKind, PropertyQueryKind, RootProfileId, RuntimeColumn, SchemeId};
