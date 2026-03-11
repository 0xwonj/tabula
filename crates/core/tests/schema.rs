#![allow(missing_docs)]
use tabula_core::{ColId, ColumnDef, TableId, TableSchema, ValueType};

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
