use tabula_core::SchemeId;
use tabula_lang::hir;
use tabula_profile::{ProfileCatalog, SchemeProfile, TypeDescriptor};

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

pub(crate) fn register_reused_profile_definitions(
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
