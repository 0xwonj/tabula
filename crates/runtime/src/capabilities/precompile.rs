use std::sync::Arc;

use tabula_executor::precompile::PrecompileHandler;
use tabula_ext::{PrecompileBundle, PrecompileDescriptor};
use tabula_ir::PrecompileId;

use crate::error::RuntimeError;
use crate::precompile_proofs::PrecompileProofFactory;

/// Unified runtime registration for one precompile capability.
pub(crate) struct PrecompileRegistration {
    descriptor: PrecompileDescriptor,
    handler: Arc<dyn PrecompileHandler>,
    proof_factory: Arc<dyn PrecompileProofFactory>,
}

impl PrecompileRegistration {
    pub(crate) fn from_bundle(bundle: PrecompileBundle) -> Result<Self, RuntimeError> {
        let (descriptor, handler, proof_factory) = bundle.into_parts();
        let handler = handler.ok_or_else(|| RuntimeError::ValidationFailed {
            detail: format!(
                "precompile 0x{:04x} is registered for verification only; execution/proving requires a handler",
                descriptor.precompile_id.0,
            ),
        })?;

        Ok(Self {
            descriptor,
            handler,
            proof_factory,
        })
    }

    /// Declared precompile descriptor.
    pub fn descriptor(&self) -> &PrecompileDescriptor {
        &self.descriptor
    }

    /// Declared precompile ID.
    pub fn id(&self) -> PrecompileId {
        self.descriptor.precompile_id
    }

    pub(crate) fn into_parts(
        self,
    ) -> (
        PrecompileDescriptor,
        Box<dyn PrecompileHandler>,
        Arc<dyn PrecompileProofFactory>,
    ) {
        (
            self.descriptor,
            Box::new(SharedPrecompileHandler(self.handler)),
            self.proof_factory,
        )
    }

    pub(crate) fn proof_factory(&self) -> Arc<dyn PrecompileProofFactory> {
        Arc::clone(&self.proof_factory)
    }
}

struct SharedPrecompileHandler(Arc<dyn PrecompileHandler>);

impl PrecompileHandler for SharedPrecompileHandler {
    fn id(&self) -> PrecompileId {
        self.0.id()
    }

    fn execute(
        &self,
        inputs: &[tabula_core::Value],
    ) -> Result<Vec<tabula_core::Value>, tabula_core::error::TabulaError> {
        self.0.execute(inputs)
    }
}

impl std::fmt::Debug for PrecompileRegistration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PrecompileRegistration")
            .field("descriptor", &self.descriptor)
            .finish_non_exhaustive()
    }
}
