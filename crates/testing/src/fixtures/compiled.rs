//! Canonical compiled-program fixtures and compiled runtime cases.

use tabula_compiler::{CompiledProgram, register_program};
use tabula_core::{ColId, TableId, TxTypeId, Value};
use tabula_ir::{Instruction, PrecompileId, RowExpr, TxTypeDef, ValueExpr};

use crate::exec::compiled_property_successor_program;
use crate::fixtures::batch::{empty_batch, no_param_batch};
use crate::fixtures::cases::CompiledRuntimeCase;
use crate::fixtures::schema::single_u64_column_schema;
use crate::fixtures::state::{empty_state, single_cell_u64};

pub fn compiled_single_write_program() -> CompiledProgram {
    let schema = single_u64_column_schema(TableId(1), ColId(0), "accounts", "balance");
    let tx_def = TxTypeDef {
        id: TxTypeId(1),
        name: "set_balance".to_string(),
        param_schema: vec![],
        body: vec![Instruction::Write {
            table: TableId(1),
            row: RowExpr::Literal(tabula_core::RowKey(0)),
            col: ColId(0),
            src_val: ValueExpr::Literal(Value::U64(7)),
            src_is_null: ValueExpr::Literal(Value::Bool(false)),
        }],
    };

    register_program(&[schema], &[tx_def]).expect("register single-write program")
}

pub fn compiled_precompile_requirement_program() -> CompiledProgram {
    register_program(
        &[],
        &[TxTypeDef {
            id: TxTypeId(1),
            name: "call".to_string(),
            param_schema: vec![],
            body: vec![Instruction::Precompile {
                id: PrecompileId(7),
                dst_slots: vec![0],
                inputs: vec![ValueExpr::Literal(Value::U64(1))],
            }],
        }],
    )
    .expect("register precompile requirement program")
}

pub fn compiled_property_successor_program_fixture() -> CompiledProgram {
    compiled_property_successor_program()
}

pub fn compiled_single_write_case() -> CompiledRuntimeCase {
    CompiledRuntimeCase {
        compiled_program: compiled_single_write_program(),
        state: single_cell_u64(TableId(1), ColId(0), tabula_core::RowKey(0), 1),
        batch: no_param_batch(1),
    }
}

pub fn compiled_precompile_requirement_case() -> CompiledRuntimeCase {
    CompiledRuntimeCase {
        compiled_program: compiled_precompile_requirement_program(),
        state: empty_state(),
        batch: no_param_batch(1),
    }
}

pub fn compiled_property_successor_case() -> CompiledRuntimeCase {
    CompiledRuntimeCase {
        compiled_program: compiled_property_successor_program_fixture(),
        state: empty_state(),
        batch: no_param_batch(1),
    }
}

pub fn compiled_empty_batch_case() -> CompiledRuntimeCase {
    CompiledRuntimeCase {
        compiled_program: compiled_single_write_program(),
        state: single_cell_u64(TableId(1), ColId(0), tabula_core::RowKey(0), 1),
        batch: empty_batch(),
    }
}
