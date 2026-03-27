//! Native capability handler and executor traits.

use std::collections::BTreeMap;

use tabula_core::error::TabulaError;
use tabula_ir as ir;
use tabula_types::TypedValue;

/// A single registered native capability implementation.
pub trait CapabilityHandler: Send + Sync {
    /// The capability ID this handler services.
    fn id(&self) -> ir::CapabilityId;
    /// Execute the capability with the given inputs and return its outputs.
    fn execute(&self, inputs: &[TypedValue]) -> Result<Vec<TypedValue>, TabulaError>;
}

/// Dispatch a capability call by ID.
pub trait CapabilityExecutor: Send + Sync {
    /// Execute the named capability with the given inputs and return its outputs.
    fn execute(
        &self,
        capability: ir::CapabilityId,
        inputs: &[TypedValue],
    ) -> Result<Vec<TypedValue>, TabulaError>;
}

/// A registry of [`CapabilityHandler`] implementations, keyed by capability ID.
#[derive(Default)]
pub struct CapabilityRegistry {
    handlers: BTreeMap<ir::CapabilityId, Box<dyn CapabilityHandler>>,
}

impl CapabilityRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a handler; returns an error if the capability ID is already registered.
    pub fn register(
        &mut self,
        handler: impl CapabilityHandler + 'static,
    ) -> Result<(), TabulaError> {
        let id = handler.id();
        if self.contains(id) {
            return Err(TabulaError::InvalidIr(format!(
                "duplicate capability ID {}",
                id.0
            )));
        }
        self.handlers.insert(id, Box::new(handler));
        Ok(())
    }

    /// Borrow the handler for a given capability ID; returns an error if not found.
    pub fn get(&self, id: ir::CapabilityId) -> Result<&dyn CapabilityHandler, TabulaError> {
        self.handlers
            .get(&id)
            .map(AsRef::as_ref)
            .ok_or_else(|| TabulaError::InvalidIr(format!("unknown capability ID {}", id.0)))
    }

    /// Return whether a capability ID has a registered handler.
    pub fn contains(&self, id: ir::CapabilityId) -> bool {
        self.handlers.contains_key(&id)
    }
}

impl CapabilityExecutor for CapabilityRegistry {
    fn execute(
        &self,
        capability: ir::CapabilityId,
        inputs: &[TypedValue],
    ) -> Result<Vec<TypedValue>, TabulaError> {
        self.get(capability)?.execute(inputs)
    }
}
