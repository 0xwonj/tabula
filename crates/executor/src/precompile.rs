//! Precompile handler registration and dispatch.
//!
//! Precompiles are custom instructions that applications register at machine
//! setup time. The executor resolves them by [`PrecompileId`] during batch
//! execution, without any ZK dependencies.

use tabula_core::Value;
use tabula_core::error::TabulaError;
use tabula_ir::PrecompileId;

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
///     fn execute(&self, inputs: &[Value]) -> Result<Vec<Value>, TabulaError> {
///         let data = inputs[0].as_bytes32()?;
///         Ok(vec![Value::Bytes32(sha256::digest(data))])
///     }
/// }
/// ```
pub trait PrecompileHandler: Send + Sync {
    /// Unique identifier for this precompile.
    fn id(&self) -> PrecompileId;

    /// Execute the precompile on concrete values.
    ///
    /// Must be deterministic and side-effect-free.
    fn execute(&self, inputs: &[Value]) -> Result<Vec<Value>, TabulaError>;
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

    /// Register a handler. Panics on duplicate ID (caller should validate).
    pub fn register(&mut self, handler: impl PrecompileHandler + 'static) {
        self.handlers.push(Box::new(handler));
    }

    /// Register a boxed handler.
    pub fn register_boxed(&mut self, handler: Box<dyn PrecompileHandler>) {
        self.handlers.push(handler);
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
}

impl Default for PrecompileRegistry {
    fn default() -> Self {
        Self::new()
    }
}
