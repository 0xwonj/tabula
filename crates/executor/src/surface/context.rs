use std::collections::BTreeMap;

use tabula_core::traits::Hasher;
use tabula_ir as ir;
use tabula_types::{TypeRuntimeRegistry, TypedValue};

use crate::host::{CapabilityExecutor, PropertyReadExecutor};

pub struct ExecContext<'a> {
    pub hasher: &'a dyn Hasher,
    pub type_runtimes: &'a TypeRuntimeRegistry,
    pub capability_executor: Option<&'a dyn CapabilityExecutor>,
    pub property_reads: Option<&'a dyn PropertyReadExecutor>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ContextValues {
    pub fields: BTreeMap<ir::ContextFieldId, TypedValue>,
}

impl ContextValues {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, id: ir::ContextFieldId, value: TypedValue) {
        self.fields.insert(id, value);
    }
}
