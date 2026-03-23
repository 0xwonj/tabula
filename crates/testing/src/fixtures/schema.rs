//! Canonical source-schema fixtures.

use tabula_compiler::{SourceColumnDef, SourceTableSchema};
use tabula_core::{ColId, TableId};
use tabula_profile::TYPE_U64_ID;

pub fn single_u64_table(table: TableId, col_name: &str) -> SourceTableSchema {
    single_u64_column_schema(table, ColId(0), "test", col_name)
}

pub fn single_u64_column_schema(
    table_id: TableId,
    col_id: ColId,
    table_name: &str,
    col_name: &str,
) -> SourceTableSchema {
    SourceTableSchema {
        id: table_id,
        name: table_name.to_string(),
        columns: vec![SourceColumnDef {
            id: col_id,
            name: col_name.to_string(),
            type_id: TYPE_U64_ID,
        }],
    }
}
