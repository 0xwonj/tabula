use tabula_core::TypeId;
use tabula_core::error::TabulaError;
use tabula_ir as ir;
use tabula_types::{TypeRuntimeRegistry, TypedValue};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PropertyReadQuery {
    Minimum,
    Maximum,
    Successor {
        key: Vec<TypedValue>,
    },
    Predecessor {
        key: Vec<TypedValue>,
    },
    Aggregate {
        kind: ir::AggregateKind,
    },
    NonExistenceRange {
        lower: Vec<TypedValue>,
        upper: Vec<TypedValue>,
    },
}

#[derive(Debug, Clone)]
pub struct PropertyReadRequest {
    pub table: ir::TableId,
    pub field: ir::FieldId,
    pub key_type: TypeId,
    pub key_arity: usize,
    pub field_type: TypeId,
    pub query: PropertyReadQuery,
    pub output_arity: usize,
}

pub trait PropertyReadExecutor: Send + Sync {
    fn execute(
        &self,
        request: &PropertyReadRequest,
        type_runtimes: &TypeRuntimeRegistry,
    ) -> Result<Vec<TypedValue>, TabulaError>;
}
