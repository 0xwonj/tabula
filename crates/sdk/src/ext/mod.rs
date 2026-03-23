//! Safe extension surface for custom schemes and precompiles.
//!
//! Compiler-facing precompile descriptor types are always available. Host-side
//! backend installation bundles remain gated to `verify`.

pub use tabula_artifact::PrecompileDescriptor;
pub use tabula_ir::PrecompileId;

#[cfg(feature = "verify")]
pub use tabula_ext::PrecompileBackendFactoryBundle;
#[cfg(feature = "verify")]
pub use tabula_ext::{
    ColumnBackendFactoryBundle, ColumnLayoutKind, PropertyQueryKind, RootProfileId, SchemeId,
};
