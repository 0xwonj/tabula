#[cfg(feature = "verify")]
use std::sync::Arc;

pub use tabula_artifact::PrecompileDescriptor;
#[cfg(feature = "prove")]
pub use tabula_executor::precompile::PrecompileHandler;
pub use tabula_ir::{PrecompileId, PrecompileSignature, PrecompileValueProfile};

#[cfg(feature = "verify")]
use crate::backend::precompile::PrecompileBackendFactory;

/// Canonical registration bundle for one installed precompile backend family.
#[cfg(feature = "verify")]
#[derive(Clone)]
pub struct PrecompileBackendFactoryBundle {
    factory: Arc<dyn PrecompileBackendFactory>,
}

#[cfg(feature = "verify")]
impl PrecompileBackendFactoryBundle {
    /// Build a canonical backend bundle from one installed precompile family.
    pub fn new(factory: impl PrecompileBackendFactory + 'static) -> Self {
        Self {
            factory: Arc::new(factory),
        }
    }

    /// Portable precompile identifier implemented by this bundle.
    pub fn precompile_id(&self) -> PrecompileId {
        self.factory.precompile_id()
    }

    /// Clone the installed backend factory.
    pub fn factory(&self) -> Arc<dyn PrecompileBackendFactory> {
        Arc::clone(&self.factory)
    }

    /// Consume the bundle and return the installed backend factory.
    pub fn into_factory(self) -> Arc<dyn PrecompileBackendFactory> {
        self.factory
    }
}
