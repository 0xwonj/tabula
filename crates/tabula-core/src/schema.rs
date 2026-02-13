//! Table and column schema definitions.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::types::{ColId, TableId, ValueType};

/// A column definition within a table schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct ColumnDef {
    /// Column identifier.
    pub id: ColId,
    /// Human-readable name.
    pub name: String,
    /// The type of values in this column.
    pub value_type: ValueType,
}

/// Schema definition for a table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct TableSchema {
    /// Table identifier.
    pub id: TableId,
    /// Human-readable name.
    pub name: String,
    /// Ordered column definitions.
    pub columns: Vec<ColumnDef>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_table_schema_construction() {
        let schema = TableSchema {
            id: TableId(1),
            name: "balances".into(),
            columns: vec![ColumnDef {
                id: ColId(0),
                name: "balance".into(),
                value_type: ValueType::U64,
            }],
        };
        assert_eq!(schema.columns.len(), 1);
        assert_eq!(schema.columns[0].value_type, ValueType::U64);
    }
}
