use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::sync::Arc;

use tabula_core::error::TabulaError;
use tabula_core::{PortableValue, TypeId};
use tabula_profile::TypeDescriptor;

use crate::TypedValue;

/// Closed arithmetic vocabulary supported by generic IR.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArithmeticOp {
    /// Addition.
    Add,
    /// Subtraction.
    Sub,
    /// Multiplication.
    Mul,
}

/// Runtime behavior for one registered semantic type.
pub trait TypeRuntime: Send + Sync {
    /// Runtime type identifier.
    fn type_id(&self) -> TypeId;

    /// Semantic descriptor backing this runtime.
    fn descriptor(&self) -> &TypeDescriptor;

    /// Canonical zero value for this type.
    fn zero_typed(&self) -> TypedValue;

    /// Encode one runtime value into the canonical portable boundary carrier.
    fn encode_portable(&self, value: &TypedValue) -> Result<PortableValue, TabulaError>;

    /// Decode one canonical portable boundary value into the runtime carrier.
    fn decode_portable(&self, value: &PortableValue) -> Result<TypedValue, TabulaError>;

    /// Validate that this typed value belongs to this runtime and has a
    /// canonical payload.
    fn validate(&self, value: &TypedValue) -> Result<(), TabulaError>;

    /// Equality over two values of this type.
    fn eq_value(&self, lhs: &TypedValue, rhs: &TypedValue) -> Result<bool, TabulaError>;

    /// Ordering comparison when the type supports ordering.
    fn cmp_value(&self, lhs: &TypedValue, rhs: &TypedValue) -> Result<Ordering, TabulaError>;

    /// Closed generic arithmetic over two values of this type.
    fn apply_arithmetic(
        &self,
        op: ArithmeticOp,
        lhs: &TypedValue,
        rhs: &TypedValue,
    ) -> Result<TypedValue, TabulaError>;

    /// Integer division and modulus when the type supports it.
    fn divmod(
        &self,
        lhs: &TypedValue,
        rhs: &TypedValue,
    ) -> Result<(TypedValue, TypedValue), TabulaError>;

    /// Human-readable display for diagnostics.
    fn debug_display(&self, value: &TypedValue) -> Result<String, TabulaError>;
}

/// Process-local registry of runtime type behavior.
#[derive(Clone, Default)]
pub struct TypeRuntimeRegistry {
    runtimes: BTreeMap<TypeId, Arc<dyn TypeRuntime>>,
}

impl TypeRuntimeRegistry {
    /// Create an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Seed the registry with standard built-in runtimes.
    pub fn seeded() -> Result<Self, TabulaError> {
        let mut registry = Self::new();
        for runtime in crate::builtins::builtin_type_runtimes()? {
            registry.register(runtime)?;
        }
        Ok(registry)
    }

    /// Register one runtime.
    pub fn register(&mut self, runtime: Arc<dyn TypeRuntime>) -> Result<(), TabulaError> {
        let type_id = runtime.type_id();
        if self.runtimes.insert(type_id, runtime).is_some() {
            return Err(TabulaError::Custom(format!(
                "duplicate type runtime registration for type {}",
                type_id.0
            )));
        }
        Ok(())
    }

    /// Resolve one runtime or fail closed.
    pub fn resolve(&self, type_id: TypeId) -> Result<&Arc<dyn TypeRuntime>, TabulaError> {
        self.runtimes.get(&type_id).ok_or_else(|| {
            TabulaError::Custom(format!(
                "missing runtime type implementation for type {}",
                type_id.0
            ))
        })
    }

    /// Decode one portable boundary value into the typed runtime carrier.
    pub fn decode_portable(&self, value: &PortableValue) -> Result<TypedValue, TabulaError> {
        self.resolve(value.type_id())?.decode_portable(value)
    }

    /// Encode one typed runtime value into the portable boundary carrier.
    pub fn encode_typed(&self, value: &TypedValue) -> Result<PortableValue, TabulaError> {
        self.resolve(value.type_id())?.encode_portable(value)
    }

    /// Return the canonical zero value for one registered type.
    pub fn zero_of(&self, type_id: TypeId) -> Result<TypedValue, TabulaError> {
        Ok(self.resolve(type_id)?.zero_typed())
    }

    /// Snapshot all registered runtimes.
    #[must_use]
    pub fn entries(&self) -> Vec<Arc<dyn TypeRuntime>> {
        self.runtimes.values().cloned().collect()
    }
}
