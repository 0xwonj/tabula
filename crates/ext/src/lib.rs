//! Official extension authoring surface for Tabula.
//!
//! Application embedding belongs in `tabula-sdk`. Extension authors package
//! reusable contributions here and install them atomically through the SDK.

/// Expert-only backend-facing support for AIR/witness/backend work.
pub mod backend;
mod error;
mod extension;
/// Root backend authoring and root witness preparation contracts.
pub mod root;
/// Column commitment scheme authoring contracts and bundle types.
pub mod scheme;

pub mod prelude {
    #[cfg(feature = "prove")]
    pub use crate::RootBackend;
    pub use crate::{
        Capability, EncodingContribution, Extension, ExtensionBuilder, SchemeContribution,
        TypeContribution,
    };
}

pub use error::{ExtError, ExtResult};
pub use extension::{
    Capability, EncodingContribution, Extension, ExtensionBuilder, SchemeContribution,
    TypeContribution,
};
#[cfg(feature = "prove")]
pub use root::RootBackend;
