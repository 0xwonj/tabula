#![allow(missing_docs)]
use tabula_core::{ColId, ColumnDef, ColumnProfileId, TableId, TableSchema};

#[test]
fn test_table_schema_construction() {
    let schema = TableSchema {
        id: TableId(1),
        name: "balances".into(),
        columns: vec![ColumnDef {
            id: ColId(0),
            name: "balance".into(),
            column_profile_id: ColumnProfileId(0),
        }],
    };
    assert_eq!(schema.columns.len(), 1);
    assert_eq!(schema.columns[0].column_profile_id, ColumnProfileId(0));
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
                column_profile_id: ColumnProfileId(0),
            },
            ColumnDef {
                id: ColId(1),
                name: "active".into(),
                column_profile_id: ColumnProfileId(1),
            },
        ],
    };
    let bytes = borsh::to_vec(&schema).unwrap();
    let decoded: TableSchema = borsh::from_slice(&bytes).unwrap();
    assert_eq!(schema, decoded);
}
