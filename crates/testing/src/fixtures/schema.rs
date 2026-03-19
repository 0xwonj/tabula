//! Canonical schema fixtures.

use tabula_core::{ColId, ColumnDef, TableId, TableSchema, ValueType};

pub fn single_u64_table(table: TableId, col_name: &str) -> TableSchema {
    single_u64_column_schema(table, ColId(0), "test", col_name)
}

pub fn single_u64_column_schema(
    table_id: TableId,
    col_id: ColId,
    table_name: &str,
    col_name: &str,
) -> TableSchema {
    TableSchema {
        id: table_id,
        name: table_name.to_string(),
        columns: vec![ColumnDef {
            id: col_id,
            name: col_name.to_string(),
            value_type: ValueType::U64,
        }],
    }
}
