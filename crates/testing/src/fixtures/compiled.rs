//! Canonical compiled-program fixtures and compiled runtime cases.

use tabula_compiler::SealedProgram;
use tabula_core::{ColId, TableId, TxTypeId};
use tabula_ir::{Instruction, RowExpr, TxTypeDef, ValueExpr};
use tabula_types::{bool_portable, u64_portable};

use crate::exec::{
    compiled_program_from_artifact, compiled_program_from_definition,
    compiled_property_successor_program,
};
use crate::fixtures::artifacts::precompile_requirement_artifact;
use crate::fixtures::batch::{empty_batch, no_param_batch};
use crate::fixtures::cases::CompiledRuntimeCase;
use crate::fixtures::schema::single_u64_column_schema;
use crate::fixtures::state::{empty_state, single_cell_u64};

pub fn compiled_single_write_program() -> SealedProgram {
    let schema = single_u64_column_schema(TableId(1), ColId(0), "accounts", "balance");
    let tx_def = TxTypeDef {
        id: TxTypeId(1),
        name: "set_balance".to_string(),
        param_schema: vec![],
        body: vec![Instruction::Write {
            table: TableId(1),
            row: RowExpr::Literal(tabula_core::RowKey(0)),
            col: ColId(0),
            src_val: ValueExpr::Literal(u64_portable(7)),
            src_is_null: ValueExpr::Literal(bool_portable(false)),
        }],
    };

    compiled_program_from_definition(vec![schema], vec![tx_def])
}

pub fn compiled_precompile_requirement_program() -> SealedProgram {
    compiled_program_from_artifact(&precompile_requirement_artifact())
}

pub fn compiled_property_successor_program_fixture() -> SealedProgram {
    compiled_property_successor_program()
}

pub fn compiled_hash_only_program() -> SealedProgram {
    compiled_program_from_definition(
        vec![],
        vec![TxTypeDef {
            id: TxTypeId(1),
            name: "hash_only".to_string(),
            param_schema: vec![],
            body: vec![Instruction::Hash {
                dst: 0,
                inputs: vec![
                    ValueExpr::Literal(u64_portable(7)),
                    ValueExpr::Literal(u64_portable(11)),
                ],
            }],
        }],
    )
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

pub fn compiled_hash_only_case() -> CompiledRuntimeCase {
    CompiledRuntimeCase {
        compiled_program: compiled_hash_only_program(),
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
