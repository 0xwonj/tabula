//! Property read executor trait for state structural queries.

use tabula_core::TypeId;
use tabula_core::error::TabulaError;
use tabula_ir as ir;
use tabula_types::{TypeRuntimeRegistry, TypedValue};

/// A structural property query over an ordered state column.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PropertyReadQuery {
    /// The minimum key in the column.
    Minimum,
    /// The maximum key in the column.
    Maximum,
    /// The successor of a given key (next larger key).
    Successor {
        /// The reference key tuple.
        key: Vec<TypedValue>,
    },
    /// The predecessor of a given key (next smaller key).
    Predecessor {
        /// The reference key tuple.
        key: Vec<TypedValue>,
    },
    /// An aggregate (sum or count) over the column.
    Aggregate {
        /// The aggregate function to apply.
        kind: ir::AggregateKind,
    },
    /// Prove that a key range contains no rows.
    NonExistenceRange {
        /// Inclusive lower bound of the empty range.
        lower: Vec<TypedValue>,
        /// Exclusive upper bound of the empty range.
        upper: Vec<TypedValue>,
    },
}

/// A fully resolved request for a state property read.
#[derive(Debug, Clone)]
pub struct PropertyReadRequest {
    /// Target state table.
    pub table: ir::TableId,
    /// Target column field within the table.
    pub field: ir::FieldId,
    /// Type identifier for the key column.
    pub key_type: TypeId,
    /// Number of key columns in the table's primary key.
    pub key_arity: usize,
    /// Type identifier for the target field.
    pub field_type: TypeId,
    /// The structural query to evaluate.
    pub query: PropertyReadQuery,
    /// Expected number of output values.
    pub output_arity: usize,
}

/// Execute a state property read query and return its typed outputs.
pub trait PropertyReadExecutor: Send + Sync {
    /// Execute the property read and return the query output values.
    fn execute(
        &self,
        request: &PropertyReadRequest,
        type_runtimes: &TypeRuntimeRegistry,
    ) -> Result<Vec<TypedValue>, TabulaError>;
}
