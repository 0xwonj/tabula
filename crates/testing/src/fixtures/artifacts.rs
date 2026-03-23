//! Canonical artifact-level fixtures and artifact runtime cases.

use tabula_artifact::{Artifact, PrecompileDescriptor};
use tabula_compiler::{
    CompilerCatalogs, ProgramDefinition, register_program_definition_with_catalogs,
};
use tabula_core::TxTypeId;
use tabula_ir::{Instruction, PrecompileId, TxTypeDef};

use crate::extensions::precompile::{
    constant_one_precompile_descriptor, sequence_precompile_descriptor,
};
use crate::fixtures::batch::no_param_batch;
use crate::fixtures::cases::ArtifactRuntimeCase;
use crate::fixtures::state::empty_state;

pub fn precompile_requirement_descriptor() -> PrecompileDescriptor {
    constant_one_precompile_descriptor(PrecompileId(0x0001))
}

pub fn precompile_requirement_artifact() -> Artifact {
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

    let definition = ProgramDefinition {
        table_schemas: vec![],
        tx_types: vec![tx],
        column_schemes: vec![],
    };
    let descriptor = precompile_requirement_descriptor();
    let catalogs = CompilerCatalogs::standard()
        .with_precompile_descriptor(descriptor)
        .expect("register precompile descriptor");

    register_program_definition_with_catalogs(&definition, &catalogs)
        .expect("register precompile requirement artifact")
        .into_artifact()
}

pub fn precompile_requirement_artifact_case() -> ArtifactRuntimeCase {
    ArtifactRuntimeCase {
        artifact: precompile_requirement_artifact(),
        state: empty_state(),
        batch: no_param_batch(1),
    }
}

pub fn sequence_precompile_descriptor_fixture() -> PrecompileDescriptor {
    sequence_precompile_descriptor(PrecompileId(0x0002))
}

pub fn sequence_precompile_artifact() -> Artifact {
    let tx = TxTypeDef {
        id: TxTypeId(1),
        name: "sequence".to_string(),
        param_schema: vec![],
        body: vec![Instruction::Precompile {
            id: PrecompileId(0x0002),
            dst_slots: vec![0, 1, 2, 3],
            inputs: vec![],
        }],
    };

    let definition = ProgramDefinition {
        table_schemas: vec![],
        tx_types: vec![tx],
        column_schemes: vec![],
    };
    let descriptor = sequence_precompile_descriptor_fixture();
    let catalogs = CompilerCatalogs::standard()
        .with_precompile_descriptor(descriptor)
        .expect("register precompile descriptor");

    register_program_definition_with_catalogs(&definition, &catalogs)
        .expect("register sequence precompile artifact")
        .into_artifact()
}

pub fn sequence_precompile_artifact_case() -> ArtifactRuntimeCase {
    ArtifactRuntimeCase {
        artifact: sequence_precompile_artifact(),
        state: empty_state(),
        batch: no_param_batch(1),
    }
}
