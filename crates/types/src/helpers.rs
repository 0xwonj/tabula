use p3_field::{PrimeCharacteristicRing, PrimeField32};
use p3_koala_bear::KoalaBear;

use tabula_core::RowKey;
use tabula_core::error::TabulaError;
use tabula_profile::{GenericIrFamily, HostValueFamily};

use crate::{EncodingRuntime, TypeRuntime, TypeRuntimeRegistry, TypedValue, u64_typed};

/// Interpret one typed value as a generic-IR boolean.
pub fn typed_bool(
    value: &TypedValue,
    type_runtimes: &TypeRuntimeRegistry,
) -> Result<bool, TabulaError> {
    let runtime = type_runtimes.resolve(value.type_id())?;
    let descriptor = runtime.descriptor();
    match (&descriptor.generic_ir_family, &descriptor.host_value_family) {
        (GenericIrFamily::Boolean, HostValueFamily::Bool) => {
            let payload: [u8; 1] =
                value
                    .payload()
                    .try_into()
                    .map_err(|_| TabulaError::TypeMismatch {
                        expected: "Boolean".to_string(),
                        actual: format!("type {}", value.type_id().0),
                    })?;
            Ok(payload[0] != 0)
        }
        _ => Err(TabulaError::TypeMismatch {
            expected: "Boolean".to_string(),
            actual: format!("type {}", value.type_id().0),
        }),
    }
}

/// Interpret one typed value as a row key.
pub fn typed_row_key(
    value: &TypedValue,
    type_runtimes: &TypeRuntimeRegistry,
) -> Result<RowKey, TabulaError> {
    let runtime = type_runtimes.resolve(value.type_id())?;
    let descriptor = runtime.descriptor();
    match (&descriptor.generic_ir_family, &descriptor.host_value_family) {
        (GenericIrFamily::UnsignedInteger, HostValueFamily::UnsignedInt { bits: 64 }) => {
            let payload: [u8; 8] =
                value
                    .payload()
                    .try_into()
                    .map_err(|_| TabulaError::TypeMismatch {
                        expected: "UnsignedInteger(64)".to_string(),
                        actual: format!("type {}", value.type_id().0),
                    })?;
            Ok(RowKey(u64::from_le_bytes(payload)))
        }
        _ => Err(TabulaError::TypeMismatch {
            expected: "UnsignedInteger(64)".to_string(),
            actual: format!("type {}", value.type_id().0),
        }),
    }
}

/// Encode one typed value as field elements plus a detached null flag.
pub fn encode_value_with_null_flag(
    type_runtime: &dyn TypeRuntime,
    encoding_runtime: &dyn EncodingRuntime,
    value: &TypedValue,
    is_null: bool,
) -> Result<(Vec<KoalaBear>, bool), TabulaError> {
    if is_null {
        return Ok((vec![KoalaBear::ZERO; encoding_runtime.trace_width()], true));
    }
    type_runtime.validate(value)?;
    let encoded = encoding_runtime.encode_field_elements(value)?;
    if encoded.len() != encoding_runtime.trace_width() {
        return Err(TabulaError::FieldEncodingError(format!(
            "encoding profile {} reported width {} but encoded {} field elements",
            encoding_runtime.encoding_profile_id().0,
            encoding_runtime.trace_width(),
            encoded.len()
        )));
    }
    Ok((encoded, false))
}

/// Encode one typed value as trace field elements followed by a null flag.
pub fn encode_trace_with_null_flag(
    type_runtime: &dyn TypeRuntime,
    encoding_runtime: &dyn EncodingRuntime,
    value: &TypedValue,
    is_null: bool,
) -> Result<Vec<KoalaBear>, TabulaError> {
    let (mut encoded, null_flag) =
        encode_value_with_null_flag(type_runtime, encoding_runtime, value, is_null)?;
    encoded.push(if null_flag {
        KoalaBear::ONE
    } else {
        KoalaBear::ZERO
    });
    Ok(encoded)
}

/// Decode one null-aware trace payload back into a typed value.
pub fn decode_trace_with_null_flag(
    type_runtime: &dyn TypeRuntime,
    encoding_runtime: &dyn EncodingRuntime,
    trace: &[KoalaBear],
) -> Result<(TypedValue, bool), TabulaError> {
    let expected = encoding_runtime.trace_width() + 1;
    if trace.len() != expected {
        return Err(TabulaError::FieldEncodingError(format!(
            "trace decode: expected {expected} field elements, got {}",
            trace.len()
        )));
    }
    let null_flag = trace[expected - 1].as_canonical_u32();
    match null_flag {
        1 => Ok((type_runtime.zero_typed(), true)),
        0 => Ok((
            encoding_runtime.decode_field_elements(&trace[..expected - 1])?,
            false,
        )),
        other => Err(TabulaError::FieldEncodingError(format!(
            "invalid val_is_null flag: {other}"
        ))),
    }
}

/// Encode one structural `u64` into a width-padded KoalaBear payload.
pub fn encode_structural_u64<const W: usize>(value: u64) -> Result<Vec<KoalaBear>, TabulaError> {
    let mut encoded = crate::builtins::encode_seeded_field_elements(&u64_typed(value))?;
    if encoded.len() > W {
        return Err(TabulaError::ProofError {
            phase: "structural_u64_encoding",
            detail: format!(
                "structural u64 encoded width {} exceeds proof width {}",
                encoded.len(),
                W
            ),
        });
    }
    encoded.resize(W, KoalaBear::ZERO);
    Ok(encoded)
}
