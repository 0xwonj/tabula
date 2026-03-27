//! Official extension authoring surface for Tabula.
//!
//! Application embedding belongs in `tabula-sdk`. Extension authors package
//! reusable contributions here and install them atomically through the SDK.

/// Expert-only backend-facing support for AIR/witness/backend work.
pub mod backend;
pub mod contribution;
mod error;
mod extension;
/// Root backend authoring and root witness preparation contracts.
pub mod root;
/// Column commitment scheme authoring contracts and bundle types.
pub mod scheme;

/// Re-exports of the most commonly needed extension authoring types.
pub mod prelude {
    #[cfg(feature = "prove")]
    pub use crate::RootBackend;
    pub use crate::{
        Capability, EncodingContribution, Extension, ExtensionBuilder, SchemeContribution,
        TypeContribution,
    };
}

pub use contribution::{Capability, EncodingContribution, SchemeContribution, TypeContribution};
pub use error::{ExtError, ExtResult};
pub use extension::{Extension, ExtensionBuilder};
#[cfg(feature = "prove")]
pub use root::RootBackend;
