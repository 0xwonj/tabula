use tabula_compiler::{SealedProgram, register_program};
use tabula_core::{ColId, TableId, TableSchema, TxTypeId, ValueType};
use tabula_ir::{AggregateKind, Instruction, PropertyQuery, TxTypeDef};
use tabula_testing::exec::compiled_property_successor_program;

pub(crate) fn compiled_program_with_property_query() -> SealedProgram {
    compiled_property_successor_program()
}

#[cfg(feature = "prove")]
pub(crate) fn compiled_program_with_unsupported_property_query() -> SealedProgram {
    let schema = TableSchema {
        id: TableId(1),
        name: "accounts".to_string(),
        columns: vec![tabula_core::ColumnDef {
            id: ColId(0),
            name: "balance".to_string(),
            value_type: ValueType::U64,
        }],
    };
    let tx = TxTypeDef {
        id: TxTypeId(1),
        name: "scan".to_string(),
        param_schema: vec![],
        body: vec![Instruction::PropertyRead {
            dst_val: 0,
            dst_key: 1,
            dst_is_null: 2,
            table: TableId(1),
            col: ColId(0),
            query: PropertyQuery::Aggregate {
                kind: AggregateKind::Count,
            },
        }],
    };

    register_program(&[schema], &[tx]).expect("register program")
}
