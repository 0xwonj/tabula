//! Safe extension surface for custom schemes and precompiles.

#[cfg(feature = "verify")]
pub use tabula_ext::{
    ColumnLayoutKind, PropertyQueryKind, RootProfileId, SchemeBundle, SchemeDescriptor, SchemeId,
    Value,
};
#[cfg(feature = "verify")]
pub use tabula_ext::{PrecompileBundle, PrecompileDescriptor, PrecompileId};
