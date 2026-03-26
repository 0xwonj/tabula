use tabula_core::EncodingProfileId;
use tabula_core::TypeId;
use tabula_core::error::TabulaError;

mod codecs;
mod constructors;
mod encoding_runtime;
mod type_runtime;

pub use codecs::{decode_seeded_field_elements, encode_seeded_field_elements};
pub use constructors::{
    bool_portable, bool_typed, bytes32_portable, bytes32_typed, i64_portable, i64_typed,
    u64_portable, u64_typed,
};
pub use encoding_runtime::builtin_encoding_runtimes;
pub use type_runtime::builtin_type_runtimes;

pub(super) fn lookup_type(
    catalog: &tabula_profile::ProfileCatalog,
    type_id: TypeId,
) -> Result<tabula_profile::TypeDescriptor, TabulaError> {
    catalog
        .types
        .iter()
        .find(|descriptor| descriptor.type_id == type_id)
        .cloned()
        .ok_or_else(|| {
            TabulaError::Custom(format!("missing built-in type descriptor {}", type_id.0))
        })
}

pub(super) fn lookup_encoding(
    catalog: &tabula_profile::ProfileCatalog,
    encoding_profile_id: EncodingProfileId,
) -> Result<tabula_profile::EncodingProfile, TabulaError> {
    catalog
        .encodings
        .iter()
        .find(|profile| profile.encoding_profile_id == encoding_profile_id)
        .cloned()
        .ok_or_else(|| {
            TabulaError::Custom(format!(
                "missing built-in encoding profile {}",
                encoding_profile_id.0
            ))
        })
}
