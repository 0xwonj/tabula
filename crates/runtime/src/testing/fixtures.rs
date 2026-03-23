use tabula_compiler::SealedProgram;
use tabula_core::{ColId, TableId, TxTypeId};
use tabula_ir::{AggregateKind, Instruction, PropertyQuery, TxTypeDef};
use tabula_testing::exec::{compiled_program_from_definition, compiled_property_successor_program};
use tabula_testing::fixtures::schema::single_u64_column_schema;

pub(crate) fn compiled_program_with_property_query() -> SealedProgram {
    compiled_property_successor_program()
}

#[cfg(feature = "prove")]
pub(crate) fn compiled_program_with_unsupported_property_query() -> SealedProgram {
    let schema = single_u64_column_schema(TableId(1), ColId(0), "accounts", "balance");
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

    compiled_program_from_definition(vec![schema], vec![tx])
}
