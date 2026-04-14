//! Program schema types: state tables, context, and constant pool.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use tabula_core::{KeyComponentSchema, PortableValue};

use super::{ConstId, ContextFieldId, FieldId, TableId, TypeRef};

/// Schema for all state tables in a program.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct StateSchema {
    /// All tables declared by the program.
    pub tables: Vec<TableSchema>,
}

/// Schema for a single state table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct TableSchema {
    /// Unique table identifier.
    pub id: TableId,
    /// Source-level table name.
    pub symbol: String,
    /// Named key components of the composite primary key (in order).
    pub keys: Vec<KeyComponentSchema>,
    /// Non-key value column definitions.
    pub fields: Vec<FieldSchema>,
}

/// Schema for a single column field in a state table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct FieldSchema {
    /// Unique field identifier within its table.
    pub id: FieldId,
    /// Source-level column name.
    pub symbol: String,
    /// Value type stored in this column.
    pub ty: TypeRef,
}

/// Schema for the program's public context (caller-supplied fields).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct ContextSchema {
    /// All context fields declared by the program.
    pub fields: Vec<ContextField>,
}

/// A single field in the context schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct ContextField {
    /// Unique field identifier.
    pub id: ContextFieldId,
    /// Source-level field name.
    pub symbol: String,
    /// Field type.
    pub ty: TypeRef,
}

/// Pool of compile-time constant values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct ConstantPool {
    /// All constant entries in the pool.
    pub entries: Vec<ConstantEntry>,
}

/// A single entry in the compile-time constant pool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct ConstantEntry {
    /// Unique constant identifier.
    pub id: ConstId,
    /// Type of the constant.
    pub ty: TypeRef,
    /// Serialized constant value.
    pub value: PortableValue,
}
