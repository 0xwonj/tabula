use std::collections::BTreeMap;

use tabula_core::error::TabulaError;
use tabula_ir as ir;
use tabula_types::TypedValue;

pub trait CapabilityHandler: Send + Sync {
    fn id(&self) -> ir::CapabilityId;
    fn execute(&self, inputs: &[TypedValue]) -> Result<Vec<TypedValue>, TabulaError>;
}

pub trait CapabilityExecutor: Send + Sync {
    fn execute(
        &self,
        capability: ir::CapabilityId,
        inputs: &[TypedValue],
    ) -> Result<Vec<TypedValue>, TabulaError>;
}

#[derive(Default)]
pub struct CapabilityRegistry {
    handlers: BTreeMap<ir::CapabilityId, Box<dyn CapabilityHandler>>,
}

impl CapabilityRegistry {
    pub fn new() -> Self {
        Self::default()
    }

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

    pub fn get(&self, id: ir::CapabilityId) -> Result<&dyn CapabilityHandler, TabulaError> {
        self.handlers
            .get(&id)
            .map(AsRef::as_ref)
            .ok_or_else(|| TabulaError::InvalidIr(format!("unknown capability ID {}", id.0)))
    }

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
