use tabula_contract::format::static_tables::{
    StaticTableArtifact, StaticTableArtifactRow, compute_static_table_artifact_root,
};
use tabula_contract::format::typed_tuple::{
    EncodedTypedTupleElement, TupleEncodingDefaults, TypedTupleRole, compute_typed_tuple_digest,
};
use tabula_core::error::TabulaError;
use tabula_ir as ir;
use tabula_types::{EncodingRuntimeRegistry, TypedValue};

use crate::registration::RegistrationContext;

pub(crate) fn build_static_table_artifact(
    program: &ir::Program,
    context: &RegistrationContext,
    tuple_encoding_defaults: &TupleEncodingDefaults,
) -> Result<StaticTableArtifact, TabulaError> {
    let mut rows = Vec::new();
    let empty_output_digest = compute_typed_tuple_digest(TypedTupleRole::RelationOutput, &[])?;

    for entry in &program.relation_manifest.entries {
        match &entry.binding {
            ir::RelationBinding::EnumSet { values } => {
                for value in values {
                    let typed = context.type_runtimes.decode_portable(value)?;
                    let encoded_inputs = encode_tuple_elements(
                        &[typed],
                        tuple_encoding_defaults,
                        &context.encoding_runtimes,
                    )?;
                    rows.push(StaticTableArtifactRow {
                        relation_id: entry.id.0,
                        input_digest: compute_typed_tuple_digest(
                            TypedTupleRole::RelationInput,
                            &encoded_inputs,
                        )?,
                        output_digest: empty_output_digest,
                    });
                }
            }
            ir::RelationBinding::Map {
                rows: relation_rows,
            } => {
                for row in relation_rows {
                    let typed_inputs = row
                        .inputs
                        .iter()
                        .map(|value| context.type_runtimes.decode_portable(value))
                        .collect::<Result<Vec<_>, _>>()?;
                    let typed_outputs = row
                        .outputs
                        .iter()
                        .map(|value| context.type_runtimes.decode_portable(value))
                        .collect::<Result<Vec<_>, _>>()?;
                    let encoded_inputs = encode_tuple_elements(
                        &typed_inputs,
                        tuple_encoding_defaults,
                        &context.encoding_runtimes,
                    )?;
                    let encoded_outputs = encode_tuple_elements(
                        &typed_outputs,
                        tuple_encoding_defaults,
                        &context.encoding_runtimes,
                    )?;
                    rows.push(StaticTableArtifactRow {
                        relation_id: entry.id.0,
                        input_digest: compute_typed_tuple_digest(
                            TypedTupleRole::RelationInput,
                            &encoded_inputs,
                        )?,
                        output_digest: compute_typed_tuple_digest(
                            TypedTupleRole::RelationOutput,
                            &encoded_outputs,
                        )?,
                    });
                }
            }
        }
    }

    rows.sort();
    rows.dedup();

    Ok(StaticTableArtifact {
        root: compute_static_table_artifact_root(&rows),
        rows,
    })
}

fn encode_tuple_elements(
    values: &[TypedValue],
    tuple_encoding_defaults: &TupleEncodingDefaults,
    encoding_runtimes: &EncodingRuntimeRegistry,
) -> Result<Vec<EncodedTypedTupleElement>, TabulaError> {
    values
        .iter()
        .map(|value| {
            let encoding_profile_id = tuple_encoding_defaults.resolve(value.type_id())?;
            Ok(EncodedTypedTupleElement {
                type_id: value.type_id(),
                field_elements: encoding_runtimes
                    .encode_field_elements_for_profile(encoding_profile_id, value)?,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile_program_source;

    #[test]
    fn explicit_registration_context_produces_deterministic_relation_artifact() {
        let program = compile_program_source(
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
        let context = RegistrationContext::builtin().expect("builtin context");

        let tuple_encoding_defaults = tabula_contract::TupleEncodingDefaults::new(
            crate::CompilerCatalogs::standard()
                .expect("standard compiler catalogs")
                .semantics()
                .default_encoding_entries()
                .into_iter()
                .map(
                    |(type_id, encoding_profile_id)| tabula_contract::TupleEncodingSelection {
                        type_id,
                        encoding_profile_id,
                    },
                )
                .collect(),
        )
        .expect("tuple defaults");
        let first =
            build_static_table_artifact(program.program(), &context, &tuple_encoding_defaults)
                .expect("first artifact");
        let second =
            build_static_table_artifact(program.program(), &context, &tuple_encoding_defaults)
                .expect("second artifact");

        assert_eq!(first, second);
    }
}
