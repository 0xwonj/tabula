use tabula_executor::precompile::PrecompileHandler;
use tabula_ir::PrecompileId;
use tabula_machine::ChipExtension;

use crate::error::RuntimeError;

/// Unified runtime registration for one precompile capability.
pub struct PrecompileRegistration {
    id: PrecompileId,
    handler: Box<dyn PrecompileHandler>,
    verifier: Box<dyn ChipExtension>,
}

impl PrecompileRegistration {
    /// Create a registration unit for one precompile capability.
    ///
    /// Validates that the declared `id` matches the executor handler's own ID.
    pub fn new(
        id: PrecompileId,
        handler: impl PrecompileHandler + 'static,
        verifier: impl ChipExtension + 'static,
    ) -> Result<Self, RuntimeError> {
        let handler = Box::new(handler);
        if handler.id() != id {
            return Err(RuntimeError::ValidationFailed {
                detail: format!(
                    "precompile registration declared id 0x{:04x} but handler reports 0x{:04x}",
                    id.0,
                    handler.id().0,
                ),
            });
        }

        Ok(Self {
            id,
            handler,
            verifier: Box::new(verifier),
        })
    }

    /// Declared precompile ID.
    pub fn id(&self) -> PrecompileId {
        self.id
    }

    pub(crate) fn into_parts(
        self,
    ) -> (
        PrecompileId,
        Box<dyn PrecompileHandler>,
        Box<dyn ChipExtension>,
    ) {
        (self.id, self.handler, self.verifier)
    }
}

impl std::fmt::Debug for PrecompileRegistration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PrecompileRegistration")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}
