use std::collections::BTreeMap;

use tabula_core::SchemeId;
use tabula_core::{ColId, ColumnDef, ColumnProfileId, TableSchema};
use tabula_ir as ir;
use tabula_lang::hir;
use tabula_profile::{
    CommitmentRole, ProfileCatalog, SchemeProfile, SemanticRegistry, TypeDescriptor,
};

use crate::hir_lower::{lower_field_id, lower_table_id};
use crate::pipeline::StateFieldSchemeBinding;

pub(crate) const DEFAULT_COLUMN_SCHEME_ID: SchemeId = SchemeId::SSMC;

pub(crate) fn derive_field_schemes(program: &hir::VerifiedProgram) -> Vec<StateFieldSchemeBinding> {
    let mut bindings = Vec::new();
    if let Some(state) = &program.program().state {
        for table in &state.tables {
            for field in &table.fields {
                if let Some(scheme) = &field.scheme {
                    bindings.push(StateFieldSchemeBinding {
                        table: lower_table_id(table.id),
                        field: lower_field_id(field.id),
                        scheme_id: scheme.id,
                    });
                }
            }
        }
    }
    bindings.sort_by_key(|binding| (binding.table, binding.field));
    bindings
}

pub(crate) fn seal_state_profiles(
    program: &ir::Program,
    field_schemes: &[StateFieldSchemeBinding],
    registry: &SemanticRegistry,
) -> Result<(Vec<TableSchema>, ProfileCatalog), String> {
    let mut scheme_by_key = field_schemes
        .iter()
        .map(|binding| ((binding.table, binding.field), binding.scheme_id))
        .collect::<BTreeMap<_, _>>();
    let catalog = registry.catalog();
    let mut sealed_catalog = ProfileCatalog::new();
    let mut table_schemas = Vec::with_capacity(program.state.tables.len());
    let mut next_column_profile_id = 0u32;

    for table in &program.state.tables {
        let mut columns = Vec::with_capacity(table.fields.len());
        for field in &table.fields {
            let type_descriptor = catalog.type_descriptor(field.ty).cloned().map_err(|_| {
                format!(
                    "state field {}.{} references unknown type id {}",
                    table.symbol, field.symbol, field.ty.0
                )
            })?;
            let encoding_profile_id = registry
                .resolve_default_encoding(field.ty)
                .map_err(|err| err.to_string())?;
            let encoding_profile = catalog
                .encoding_profile(encoding_profile_id)
                .cloned()
                .map_err(|_| {
                    format!(
                        "type id {} resolved default encoding {} that is missing from the registry catalog",
                        field.ty.0, encoding_profile_id.0
                    )
                })?;
            let scheme_family_id = scheme_by_key
                .remove(&(table.id, field.id))
                .unwrap_or(DEFAULT_COLUMN_SCHEME_ID);
            let scheme_profile_id = registry
                .resolve_default_scheme_profile(scheme_family_id, encoding_profile_id)
                .map_err(|err| err.to_string())?;
            let scheme_profile =
                catalog
                    .scheme_profile(scheme_profile_id)
                    .cloned()
                    .map_err(|_| {
                        format!(
                            "scheme family {} + encoding {} resolved missing scheme profile {}",
                            scheme_family_id.0, encoding_profile_id.0, scheme_profile_id.0
                        )
                    })?;

            register_reused_profile_definitions(
                &mut sealed_catalog,
                &type_descriptor,
                &encoding_profile,
                &scheme_profile,
            )
            .map_err(|err| err.to_string())?;

            let column_profile = tabula_profile::ColumnProfile::new(
                ColumnProfileId(next_column_profile_id),
                format!("{}.{}", table.symbol, field.symbol),
                None,
                &type_descriptor,
                &encoding_profile,
                &scheme_profile,
                CommitmentRole::IncludedInRoot,
            )
            .map_err(|err| err.to_string())?;
            let column_profile_id = column_profile.column_profile_id;
            next_column_profile_id += 1;
            sealed_catalog
                .register_column(column_profile)
                .map_err(|err| err.to_string())?;
            columns.push(ColumnDef {
                id: ColId(field.id.0),
                name: field.symbol.clone(),
                column_profile_id,
            });
        }

        table_schemas.push(TableSchema {
            id: table.id.into(),
            name: table.symbol.clone(),
            columns,
        });
    }

    if let Some(((table, field), _)) = scheme_by_key.first_key_value() {
        return Err(format!(
            "field scheme selection references unknown table {} field {}",
            table.0, field.0
        ));
    }

    sealed_catalog.validate().map_err(|err| err.to_string())?;
    Ok((table_schemas, sealed_catalog))
}

fn register_reused_profile_definitions(
    catalog: &mut ProfileCatalog,
    type_descriptor: &TypeDescriptor,
    encoding_profile: &tabula_profile::EncodingProfile,
    scheme_profile: &SchemeProfile,
) -> Result<(), tabula_profile::ProfileError> {
    if !catalog
        .types
        .iter()
        .any(|descriptor| descriptor.type_id == type_descriptor.type_id)
    {
        catalog.register_type(type_descriptor.clone())?;
    }
    if !catalog
        .encodings
        .iter()
        .any(|profile| profile.encoding_profile_id == encoding_profile.encoding_profile_id)
    {
        catalog.register_encoding(encoding_profile.clone())?;
    }
    if !catalog
        .schemes
        .iter()
        .any(|profile| profile.scheme_profile_id == scheme_profile.scheme_profile_id)
    {
        catalog.register_scheme(scheme_profile.clone())?;
    }
    Ok(())
}
