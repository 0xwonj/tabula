//! Value encoding and SSMC hash-chain utilities.
//!
//! Pure encoding and commitment helpers for witness preparation:
//! - Value → field element encoding with null handling
//! - Poseidon2 hash-chain input construction for SSMC commitments

use p3_field::PrimeCharacteristicRing;
use p3_koala_bear::KoalaBear;

use tabula_core::error::TabulaError;
use tabula_core::traits::ValueCodec;
use tabula_core::{Value, ValueType};

/// Encode a value as Tier 1 ComEnc field elements, with null handling.
///
/// When `is_null` is true, produces literal zeros (`[0; w]`) matching
/// the `encode_trace` convention — the null flag gates the value, so
/// the actual FE content is irrelevant.
pub(crate) fn encode_value_with_null_flag(
    codec: &impl ValueCodec<FieldRepr = KoalaBear>,
    value: &Value,
    is_null: bool,
    value_type: ValueType,
) -> Result<(Vec<KoalaBear>, bool), TabulaError> {
    if is_null {
        let w = codec.field_elements_per(value_type);
        Ok((vec![KoalaBear::ZERO; w], true))
    } else {
        let fes = codec.encode(value)?;
        Ok((fes, false))
    }
}

/// Encode an `Option<Value>` as Tier 1 ComEnc field elements.
///
/// `None` maps to null (literal zeros). Delegates to `encode_value_with_null_flag`.
pub(crate) fn encode_value(
    codec: &impl ValueCodec<FieldRepr = KoalaBear>,
    value: &Option<Value>,
    value_type: ValueType,
) -> Result<(Vec<KoalaBear>, bool), TabulaError> {
    match value {
        Some(v) => encode_value_with_null_flag(codec, v, false, value_type),
        None => {
            let w = codec.field_elements_per(value_type);
            Ok((vec![KoalaBear::ZERO; w], true))
        }
    }
}
