use tabula_contract::{
    BINDING_REGISTRY_VERSION, CONTRACT_SCHEMA_VERSION, ContractMetadataEnvelope,
    STATEMENT_SCHEMA_VERSION, TupleEncodingDefaults, TupleEncodingSelection,
    VERIFIER_PROFILE_VERSION,
};

use crate::CompilerCatalogs;
use crate::error::{CompilerError, CompilerResult};
use crate::pipeline::{CompiledProgram, RegisteredProgram, compile_program_source_with_catalogs};
use crate::registration::RegistrationContext;
use crate::registration::binding::{
    compute_profile_hash, compute_program_binding, compute_semantic_hash_stub,
};
use crate::registration::profiles::seal_state_profiles;
use crate::registration::static_tables::build_static_table_artifact;

/// Register a compiled rewritten program into a native sealed artifact.
pub fn register_compiled_program(
    compiled: CompiledProgram,
    catalogs: &CompilerCatalogs,
) -> CompilerResult<RegisteredProgram> {
    let context = RegistrationContext::builtin()
        .map_err(|source| CompilerError::InvalidProgram(anyhow::anyhow!(source.to_string())))?;
    register_compiled_program_with_context(compiled, catalogs, &context)
}

fn register_compiled_program_with_context(
    compiled: CompiledProgram,
    catalogs: &CompilerCatalogs,
    context: &RegistrationContext,
) -> CompilerResult<RegisteredProgram> {
    let (validated, field_schemes) = compiled.into_parts();
    let (table_schemas, profile_catalog) =
        seal_state_profiles(validated.as_program(), &field_schemes, catalogs.semantics())
            .map_err(|detail| CompilerError::InvalidProgram(anyhow::anyhow!(detail)))?;
    let tuple_encoding_defaults = TupleEncodingDefaults::new(
        catalogs
            .semantics()
            .default_encoding_entries()
            .into_iter()
            .map(|(type_id, encoding_profile_id)| TupleEncodingSelection {
                type_id,
                encoding_profile_id,
            })
            .collect(),
    )
    .map_err(|source| CompilerError::InvalidProgram(anyhow::anyhow!(source.to_string())))?;
    let capability_manifest = validated.as_program().capability_manifest.entries.clone();
    let static_table_artifact =
        build_static_table_artifact(validated.as_program(), context, &tuple_encoding_defaults)
            .map_err(|source| CompilerError::InvalidProgram(anyhow::anyhow!(source.to_string())))?;
    let profile_hash = compute_profile_hash(&table_schemas, &profile_catalog)
        .map_err(CompilerError::InvalidProgram)?;
    let semantic_hash_stub = Some(
        compute_semantic_hash_stub(validated.as_program(), &field_schemes, &profile_catalog)
            .map_err(CompilerError::InvalidProgram)?,
    );
    let metadata_envelope = ContractMetadataEnvelope {
        profile_hash,
        contract_schema_version: CONTRACT_SCHEMA_VERSION,
        binding_registry_version: BINDING_REGISTRY_VERSION,
        statement_schema_version: STATEMENT_SCHEMA_VERSION,
        verifier_profile_version: VERIFIER_PROFILE_VERSION,
        semantic_hash_stub,
    };
    let binding =
        compute_program_binding(validated.as_program(), &field_schemes, &metadata_envelope)
            .map_err(CompilerError::InvalidProgram)?;

    Ok(RegisteredProgram {
        validated,
        field_schemes,
        table_schemas,
        profile_catalog,
        tuple_encoding_defaults,
        capability_manifest,
        static_table_artifact,
        metadata_envelope,
        binding,
    })
}

/// Compile and register rewritten source into a native sealed artifact.
pub fn compile_and_register_program_source(
    source: &str,
    catalogs: &CompilerCatalogs,
) -> CompilerResult<RegisteredProgram> {
    let compiled = compile_program_source_with_catalogs(source, catalogs)?;
    register_compiled_program(compiled, catalogs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile_program_source;

    #[test]
    fn explicit_registration_context_matches_public_registration_path() {
        let compiled = compile_program_source(
            r#"
program P

relation Allowed(x: u64) = enum { 1, 2, 3 };

tx check(x: u64) {
  assert relation Allowed(x);
  return;
}
"#,
        )
        .expect("compiled");
        let catalogs = CompilerCatalogs::standard().expect("standard compiler catalogs");
        let context = RegistrationContext::builtin().expect("builtin context");

        let explicit =
            register_compiled_program_with_context(compiled.clone(), &catalogs, &context)
                .expect("explicit registration");
        let public = register_compiled_program(compiled, &catalogs).expect("public registration");

        assert_eq!(
            explicit.static_table_artifact(),
            public.static_table_artifact()
        );
        assert_eq!(explicit.binding(), public.binding());
    }
}
