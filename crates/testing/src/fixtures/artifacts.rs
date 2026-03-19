//! Canonical artifact-level fixtures and artifact runtime cases.

use tabula_artifact::ProgramArtifact;
use tabula_compiler::register_program;
use tabula_core::{ColId, TableId, TxTypeId};
use tabula_ir::{Instruction, PrecompileId, TxTypeDef};

use crate::fixtures::batch::no_param_batch;
use crate::fixtures::cases::ArtifactRuntimeCase;
use crate::fixtures::schema::single_u64_column_schema;
use crate::fixtures::state::empty_state;

pub fn precompile_requirement_artifact() -> ProgramArtifact {
    let schema = single_u64_column_schema(TableId(1), ColId(0), "accounts", "balance");
    let tx = TxTypeDef {
        id: TxTypeId(1),
        name: "scan".to_string(),
        param_schema: vec![],
        body: vec![Instruction::Precompile {
            id: PrecompileId(0x0001),
            dst_slots: vec![0],
            inputs: vec![],
        }],
    };

    register_program(&[schema], &[tx])
        .expect("register precompile requirement artifact")
        .into_program_artifact()
}

pub fn precompile_requirement_artifact_case() -> ArtifactRuntimeCase {
    ArtifactRuntimeCase {
        program_artifact: precompile_requirement_artifact(),
        state: empty_state(),
        batch: no_param_batch(1),
    }
}
