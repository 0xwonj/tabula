use tabula_core::error::TabulaError;
use tabula_core::{RowKey, TypeId};
use tabula_profile::is_u64_type;
use tabula_types::{TypeRuntimeRegistry, TypedValue, typed_row_key};

pub(crate) fn decode_row_key(
    value: &TypedValue,
    key_ty: TypeId,
    type_runtimes: &TypeRuntimeRegistry,
) -> Result<RowKey, TabulaError> {
    if !is_u64_type(key_ty) {
        return Err(TabulaError::InvalidIr(format!(
            "V1 canonical executor only supports [u64] key schema, got {}",
            key_ty.0
        )));
    }
    typed_row_key(value, type_runtimes).map_err(|_| {
        TabulaError::InvalidIr(format!(
            "V1 canonical executor expects state keys to be u64, got {}",
            value.type_id().0
        ))
    })
}
