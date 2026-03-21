use std::sync::Arc;

pub use tabula_artifact::PrecompileDescriptor;
#[cfg(feature = "prove")]
pub use tabula_executor::precompile::PrecompileHandler;
pub use tabula_ir::PrecompileId;

use crate::backend::precompile::PrecompileProofFactory;
use crate::error::{ExtError, ExtResult};

/// Canonical bundle for one custom precompile capability.
#[derive(Clone)]
pub struct PrecompileBundle {
    descriptor: PrecompileDescriptor,
    #[cfg(feature = "prove")]
    handler: Option<Arc<dyn PrecompileHandler>>,
    proof_factory: Arc<dyn PrecompileProofFactory>,
}

impl PrecompileBundle {
    /// Build a verifier-only bundle from one sealed descriptor and one proof factory.
    pub fn verification(
        descriptor: PrecompileDescriptor,
        proof_factory: impl PrecompileProofFactory + 'static,
    ) -> ExtResult<Self> {
        let proof_factory: Arc<dyn PrecompileProofFactory> = Arc::new(proof_factory);
        let factory_descriptor = proof_factory.descriptor();

        if factory_descriptor.precompile_id != descriptor.precompile_id {
            return Err(ExtError::validation(format!(
                "precompile bundle declared id 0x{:04x} but proof factory reports 0x{:04x}",
                descriptor.precompile_id.0, factory_descriptor.precompile_id.0,
            )));
        }
        if factory_descriptor != descriptor {
            return Err(ExtError::validation(format!(
                "precompile bundle requires identical descriptors for id 0x{:04x}",
                descriptor.precompile_id.0,
            )));
        }

        Ok(Self {
            descriptor,
            #[cfg(feature = "prove")]
            handler: None,
            proof_factory,
        })
    }

    /// Upgrade a verifier-only bundle into an execution/proving-capable bundle.
    #[cfg(feature = "prove")]
    pub fn with_handler(mut self, handler: impl PrecompileHandler + 'static) -> ExtResult<Self> {
        let handler: Arc<dyn PrecompileHandler> = Arc::new(handler);
        if handler.id() != self.descriptor.precompile_id {
            return Err(ExtError::validation(format!(
                "precompile bundle declared id 0x{:04x} but handler reports 0x{:04x}",
                self.descriptor.precompile_id.0,
                handler.id().0,
            )));
        }
        self.handler = Some(handler);
        Ok(self)
    }

    /// The compiler-visible descriptor carried by this bundle.
    pub fn descriptor(&self) -> &PrecompileDescriptor {
        &self.descriptor
    }

    /// Portable precompile identifier carried by this bundle.
    pub fn id(&self) -> PrecompileId {
        self.descriptor.precompile_id
    }

    /// Clone the verifier/proof-side factory.
    pub fn proof_factory(&self) -> Arc<dyn PrecompileProofFactory> {
        Arc::clone(&self.proof_factory)
    }

    /// Consume the bundle and return the proof-side factory.
    pub fn into_proof_factory(self) -> Arc<dyn PrecompileProofFactory> {
        self.proof_factory
    }

    /// Clone the execution handler if present.
    #[cfg(feature = "prove")]
    pub fn handler(&self) -> Option<Arc<dyn PrecompileHandler>> {
        self.handler.as_ref().map(Arc::clone)
    }

    /// Consume the bundle and return all owned parts.
    #[cfg(feature = "prove")]
    pub fn into_parts(
        self,
    ) -> (
        PrecompileDescriptor,
        Option<Arc<dyn PrecompileHandler>>,
        Arc<dyn PrecompileProofFactory>,
    ) {
        (self.descriptor, self.handler, self.proof_factory)
    }

    /// Consume the bundle and return verifier-side owned parts.
    #[cfg(not(feature = "prove"))]
    pub fn into_parts(self) -> (PrecompileDescriptor, Arc<dyn PrecompileProofFactory>) {
        (self.descriptor, self.proof_factory)
    }
}
