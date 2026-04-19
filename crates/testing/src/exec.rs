//! Shared execution helpers built on canonical public seams.

use std::sync::Arc;
use tabula_compiler::SourceCapabilityDescriptor;
use tabula_compiler::{
    CompiledProgram, CompilerCatalogs, RegisteredProgram, compile_and_register_program_source,
    compile_program_source_with_catalogs,
};
use tabula_core::PortableValue;
use tabula_ir as ir;
use tabula_profile::{TYPE_BYTES32_ID, TYPE_U64_ID};

use tabula_runtime::{CommittedStateSnapshot, PreparedOptions, prepare_executor};

pub fn compile_program_from_source(source: &str) -> CompiledProgram {
    compile_program_from_source_with_catalogs(source, &standard_catalogs())
}

pub fn compile_program_from_source_with_catalogs(
    source: &str,
    catalogs: &CompilerCatalogs,
) -> CompiledProgram {
    compile_program_source_with_catalogs(source, catalogs).expect("compile source")
}

pub fn register_program_from_source(source: &str) -> RegisteredProgram {
    register_program_from_source_with_catalogs(source, &standard_catalogs())
}

pub fn register_program_from_source_with_catalogs(
    source: &str,
    catalogs: &CompilerCatalogs,
) -> RegisteredProgram {
    compile_and_register_program_source(source, catalogs).expect("compile and register source")
}

pub fn logical_state_snapshot(
    registered_program: &RegisteredProgram,
    cells: &[(ir::TableId, Vec<PortableValue>, ir::FieldId, PortableValue)],
) -> CommittedStateSnapshot {
    let opts = PreparedOptions::try_standard().expect("standard prepared options");
    let executor = prepare_executor(Arc::new(registered_program.clone()), &opts)
        .expect("build prepared executor");
    executor
        .materialize_logical_state(
            cells
                .iter()
                .map(|(table, key, field, value)| (*table, key.clone(), *field, value.clone())),
        )
        .expect("build committed state snapshot")
}

pub fn tx_batch(calls: Vec<ir::EntryCall>) -> ir::EntryBatch {
    ir::EntryBatch { calls }
}

pub fn context_input(
    fields: impl IntoIterator<Item = (ir::ContextFieldId, PortableValue)>,
) -> ir::ContextInput {
    ir::ContextInput {
        fields: fields.into_iter().collect(),
    }
}

fn standard_catalogs() -> CompilerCatalogs {
    CompilerCatalogs::standard()
        .expect("standard compiler catalogs")
        .with_capability_descriptor(SourceCapabilityDescriptor {
            path: "poseidon_hash".into(),
            inputs: vec![TYPE_U64_ID],
            outputs: vec![TYPE_BYTES32_ID],
            totality: ir::CapabilityTotality::Total,
            query_policy: ir::CapabilityQueryPolicy::QuerySafe,
            proof_visibility: ir::CapabilityProofVisibility::OpaqueRuntimeOnly,
            hash_family: Some(ir::HashFamily::Poseidon),
        })
        .expect("capability catalog")
}
