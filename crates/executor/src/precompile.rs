//! Precompile handler registration and dispatch.
//!
//! Precompiles are custom instructions that applications register at machine
//! setup time. The executor resolves them by [`PrecompileId`] during batch
//! execution, without any ZK dependencies.

use tabula_core::error::TabulaError;
use tabula_ir::{PrecompileId, PrecompileSignature};
use tabula_types::TypedValue;

/// Application-defined handler for a single precompile instruction.
///
/// Implementations execute deterministic pure-function logic. They must not
/// access state or produce side effects — only transform input values to
/// output values.
///
/// # Example
///
/// ```ignore
/// struct Sha256Handler;
/// impl PrecompileHandler for Sha256Handler {
///     fn id(&self) -> PrecompileId { PrecompileId(0x0004) }
///     fn execute(&self, inputs: &[TypedValue]) -> Result<Vec<TypedValue>, TabulaError> {
///         let _ = inputs;
///         todo!()
///     }
/// }
/// ```
pub trait PrecompileHandler: Send + Sync {
    /// Unique identifier for this precompile.
    fn id(&self) -> PrecompileId;

    /// Exact typed I/O contract implemented by this handler.
    fn signature(&self) -> &PrecompileSignature;

    /// Execute the precompile on concrete values.
    ///
    /// Must be deterministic and side-effect-free.
    fn execute(&self, inputs: &[TypedValue]) -> Result<Vec<TypedValue>, TabulaError>;
}

/// Registry of precompile handlers, keyed by [`PrecompileId`].
pub struct PrecompileRegistry {
    handlers: Vec<Box<dyn PrecompileHandler>>,
}

impl PrecompileRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            handlers: Vec::new(),
        }
    }

    /// Register a handler.
    ///
    /// Returns an error if another handler with the same [`PrecompileId`]
    /// is already present.
    pub fn register(
        &mut self,
        handler: impl PrecompileHandler + 'static,
    ) -> Result<(), TabulaError> {
        self.register_boxed(Box::new(handler))
    }

    /// Register a boxed handler.
    pub fn register_boxed(
        &mut self,
        handler: Box<dyn PrecompileHandler>,
    ) -> Result<(), TabulaError> {
        let id = handler.id();
        if self.contains(id) {
            return Err(TabulaError::InvalidIr(format!(
                "duplicate precompile ID: 0x{:04x}",
                id.0
            )));
        }
        self.handlers.push(handler);
        Ok(())
    }

    /// Look up a handler by ID.
    pub fn get(&self, id: PrecompileId) -> Result<&dyn PrecompileHandler, TabulaError> {
        self.handlers
            .iter()
            .find(|h| h.id() == id)
            .map(AsRef::as_ref)
            .ok_or_else(|| TabulaError::InvalidIr(format!("unknown precompile ID: 0x{:04x}", id.0)))
    }

    /// Returns `true` if the registry has no handlers.
    pub fn is_empty(&self) -> bool {
        self.handlers.is_empty()
    }

    /// Returns `true` if a handler with this ID is present.
    pub fn contains(&self, id: PrecompileId) -> bool {
        self.handlers.iter().any(|handler| handler.id() == id)
    }
}

impl Default for PrecompileRegistry {
    fn default() -> Self {
        Self::new()
    }
}
