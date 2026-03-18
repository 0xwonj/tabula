//! Source-derived program definitions prior to registration.

use serde::{Deserialize, Serialize};

use tabula_core::{ColId, SchemeId, TableId, TableSchema};
use tabula_ir::TxTypeDef;

/// Source-derived program definitions before metadata sealing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProgramDefinition {
    /// Table schema definitions.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub table_schemas: Vec<TableSchema>,
    /// Transaction type definitions.
    pub tx_types: Vec<TxTypeDef>,
    /// Source-selected non-default commitment schemes for specific columns.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub column_schemes: Vec<ColumnSchemeSelection>,
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
