//! Source-derived program definitions prior to registration.

use serde::{Deserialize, Serialize};

use tabula_core::{ColId, SchemeId, TableId, TypeId};
use tabula_ir::TxTypeDef;

/// Source-derived program definitions before metadata sealing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProgramDefinition {
    /// Table schema definitions.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub table_schemas: Vec<SourceTableSchema>,
    /// Transaction type definitions.
    pub tx_types: Vec<TxTypeDef>,
    /// Source-selected non-default commitment schemes for specific columns.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub column_schemes: Vec<ColumnSchemeSelection>,
}

/// Source-side column definition before compiler sealing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceColumnDef {
    /// Column identifier.
    pub id: ColId,
    /// Human-readable name.
    pub name: String,
    /// Source-resolved semantic type selection.
    pub type_id: TypeId,
}

/// Source-side table schema before compiler sealing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceTableSchema {
    /// Table identifier.
    pub id: TableId,
    /// Human-readable name.
    pub name: String,
    /// Ordered column definitions.
    pub columns: Vec<SourceColumnDef>,
}

/// Source-level scheme override for one specific column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColumnSchemeSelection {
    /// Table identifier.
    pub table_id: TableId,
    /// Column identifier.
    pub col_id: ColId,
    /// Portable commitment scheme identifier.
    pub scheme_id: SchemeId,
}
