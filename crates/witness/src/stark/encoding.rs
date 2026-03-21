//! Value encoding and SSMC hash-chain utilities.
//!
//! Pure encoding and commitment helpers for witness preparation:
//! - Value → field element encoding with null handling
//! - TraceEnc helpers for row payloads
//! - Poseidon2 hash-chain input construction for SSMC commitments

use p3_field::PrimeCharacteristicRing;
use p3_koala_bear::KoalaBear;

use tabula_core::error::TabulaError;
use tabula_core::traits::ValueCodec;
use tabula_core::{Value, ValueType};

#[cfg(test)]
use p3_field::PrimeField32;
#[cfg(test)]
use tabula_core::zero_value;

/// Tier 2 (TraceEnc) width: `w(T) + 1` (value FEs + val_is_null flag).
pub(crate) fn trace_width(
    codec: &impl ValueCodec<FieldRepr = KoalaBear>,
    value_type: ValueType,
) -> usize {
    codec.field_elements_per(value_type) + 1
}

/// Tier 2 (TraceEnc) encoding: ComEnc(value) ++ val_is_null flag.
///
/// - `is_null=false` → `ComEnc(value) ++ [0]`
/// - `is_null=true`  → `[0; w(T)] ++ [1]` (canonical zero encoding)
pub(crate) fn encode_trace(
    codec: &impl ValueCodec<FieldRepr = KoalaBear>,
    value: &Value,
    is_null: bool,
    value_type: ValueType,
) -> Result<Vec<KoalaBear>, TabulaError> {
    let w = codec.field_elements_per(value_type);
    let mut fes = Vec::with_capacity(trace_width(codec, value_type));
    if is_null {
        fes.resize(w, KoalaBear::ZERO);
        fes.push(KoalaBear::ONE);
    } else {
        fes.extend(codec.encode(value)?);
        fes.push(KoalaBear::ZERO);
    }
    Ok(fes)
}

/// Tier 2 (TraceEnc) decoding: inverse of [`encode_trace`].
#[cfg(test)]
pub(crate) fn decode_trace(
    codec: &impl ValueCodec<FieldRepr = KoalaBear>,
    fes: &[KoalaBear],
    value_type: ValueType,
) -> Result<(Value, bool), TabulaError> {
    let expected = trace_width(codec, value_type);
    if fes.len() != expected {
        return Err(TabulaError::FieldEncodingError(format!(
            "trace decode: expected {expected} FEs for {value_type}, got {}",
            fes.len()
        )));
    }
    let null_flag = fes[expected - 1].as_canonical_u32();
    match null_flag {
        1 => Ok((zero_value(value_type), true)),
        0 => {
            let value = codec.decode(&fes[..expected - 1], value_type)?;
            Ok((value, false))
        }
        _ => Err(TabulaError::FieldEncodingError(format!(
            "invalid val_is_null flag: {null_flag}"
        ))),
    }
}

/// Encode a value as Tier 1 ComEnc field elements plus separate null flag.
///
/// When `is_null` is true, produces literal zeros (`[0; w]`) and a `true`
/// null flag matching the TraceEnc convention.
pub(crate) fn encode_value_with_null_flag(
    codec: &impl ValueCodec<FieldRepr = KoalaBear>,
    value: &Value,
    is_null: bool,
    value_type: ValueType,
) -> Result<(Vec<KoalaBear>, bool), TabulaError> {
    let mut trace = encode_trace(codec, value, is_null, value_type)?;
    let null_flag = trace
        .pop()
        .expect("trace encoding always includes null flag");
    Ok((trace, null_flag == KoalaBear::ONE))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tabula_commitment::KoalaBearCodec;
    use tabula_commitment::primitives::decode_u64_limbs;

    fn trace_round_trip(value: Value, is_null: bool, ty: ValueType) {
        let codec = KoalaBearCodec;
        let fes = encode_trace(&codec, &value, is_null, ty).unwrap();
        assert_eq!(fes.len(), trace_width(&codec, ty));
        let (decoded, decoded_null) = decode_trace(&codec, &fes, ty).unwrap();
        assert_eq!(decoded_null, is_null);
        if is_null {
            assert_eq!(decoded, zero_value(ty));
        } else {
            assert_eq!(decoded, value);
        }
    }

    #[test]
    fn trace_round_trip_bool() {
        trace_round_trip(Value::Bool(true), false, ValueType::Bool);
        trace_round_trip(Value::Bool(false), true, ValueType::Bool);
    }

    #[test]
    fn trace_round_trip_u64() {
        trace_round_trip(Value::U64(42), false, ValueType::U64);
        trace_round_trip(Value::U64(0), true, ValueType::U64);
    }

    #[test]
    fn trace_round_trip_i64() {
        trace_round_trip(Value::I64(-999), false, ValueType::I64);
        trace_round_trip(Value::I64(0), true, ValueType::I64);
    }

    #[test]
    fn trace_round_trip_bytes32() {
        let mut bytes = [0u8; 32];
        bytes[0..4].copy_from_slice(&100u32.to_le_bytes());
        trace_round_trip(Value::Bytes32(bytes), false, ValueType::Bytes32);
        trace_round_trip(Value::Bytes32([0; 32]), true, ValueType::Bytes32);
    }

    #[test]
    fn trace_width_correct() {
        let codec = KoalaBearCodec;
        assert_eq!(trace_width(&codec, ValueType::Bool), 2);
        assert_eq!(trace_width(&codec, ValueType::U64), 4);
        assert_eq!(trace_width(&codec, ValueType::I64), 4);
        assert_eq!(trace_width(&codec, ValueType::Bytes32), 9);
    }

    #[test]
    fn trace_null_i64_canonical_zero() {
        let codec = KoalaBearCodec;
        let fes = encode_trace(&codec, &Value::I64(0), true, ValueType::I64).unwrap();
        assert_eq!(fes[0], KoalaBear::ZERO);
        assert_eq!(fes[1], KoalaBear::ZERO);
        assert_eq!(fes[2], KoalaBear::ZERO);
        assert_eq!(fes[3], KoalaBear::ONE);
    }

    #[test]
    fn trace_non_null_i64_uses_offset_encoding() {
        let codec = KoalaBearCodec;
        let fes = encode_trace(&codec, &Value::I64(0), false, ValueType::I64).unwrap();
        let limbs: &[KoalaBear; 3] = fes[..3].try_into().unwrap();
        let offset = decode_u64_limbs(limbs).unwrap();
        assert_eq!(offset, 1u64 << 63);
        assert_eq!(fes[3], KoalaBear::ZERO);
    }

    #[test]
    fn trace_decode_wrong_width_rejected() {
        let codec = KoalaBearCodec;
        let fes = vec![KoalaBear::ZERO; 3];
        assert!(decode_trace(&codec, &fes, ValueType::Bool).is_err());
    }
}
