//! Table and column schema definitions.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::{ColId, TableId, ValueType};

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

    #[test]
    fn borsh_round_trip_table_schema() {
        let schema = TableSchema {
            id: TableId(1),
            name: "users".into(),
            columns: vec![
                ColumnDef {
                    id: ColId(0),
                    name: "balance".into(),
                    value_type: ValueType::U64,
                },
                ColumnDef {
                    id: ColId(1),
                    name: "active".into(),
                    value_type: ValueType::Bool,
                },
            ],
        };
        let bytes = borsh::to_vec(&schema).unwrap();
        let decoded: TableSchema = borsh::from_slice(&bytes).unwrap();
        assert_eq!(schema, decoded);
    }
}
