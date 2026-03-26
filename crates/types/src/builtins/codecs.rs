use borsh::BorshDeserialize;
use p3_field::{PrimeCharacteristicRing, PrimeField32};
use p3_koala_bear::KoalaBear;
use tabula_core::TypeId;
use tabula_core::error::TabulaError;
use tabula_profile::{TYPE_BOOL_ID, TYPE_BYTES32_ID, TYPE_I64_ID, TYPE_U64_ID};

use super::constructors::{bool_typed, bytes32_typed, i64_typed, u64_typed};
use crate::TypedValue;

/// Encode one typed built-in runtime value into KoalaBear field elements.
pub fn encode_seeded_field_elements(value: &TypedValue) -> Result<Vec<KoalaBear>, TabulaError> {
    match value.type_id() {
        TYPE_BOOL_ID => Ok(vec![if decode_bool_payload(value, TYPE_BOOL_ID)? {
            KoalaBear::ONE
        } else {
            KoalaBear::ZERO
        }]),
        TYPE_U64_ID => Ok(encode_u64_limbs(decode_u64_payload(value, TYPE_U64_ID)?).to_vec()),
        TYPE_I64_ID => {
            let offset = (decode_i64_payload(value, TYPE_I64_ID)? as i128 + (1i128 << 63)) as u64;
            Ok(encode_u64_limbs(offset).to_vec())
        }
        TYPE_BYTES32_ID => {
            let bytes = decode_bytes32_payload(value, TYPE_BYTES32_ID)?;
            let mut fes = Vec::with_capacity(8);
            for (i, chunk) in bytes.chunks_exact(4).enumerate() {
                let limb = u32::from_le_bytes(chunk.try_into().expect("chunk"));
                if limb >= KoalaBear::ORDER_U32 {
                    return Err(TabulaError::FieldEncodingError(format!(
                        "Bytes32 chunk {i}: {limb} >= KoalaBear modulus {}",
                        KoalaBear::ORDER_U32
                    )));
                }
                fes.push(KoalaBear::new(limb));
            }
            Ok(fes)
        }
        other => Err(TabulaError::TypeMismatch {
            expected: "registered built-in type".to_string(),
            actual: format!("type {}", other.0),
        }),
    }
}

/// Decode KoalaBear field elements into one typed built-in runtime value.
pub fn decode_seeded_field_elements(
    type_id: TypeId,
    field_elements: &[KoalaBear],
) -> Result<TypedValue, TabulaError> {
    match type_id {
        TYPE_BOOL_ID => {
            if field_elements.len() != 1 {
                return Err(TabulaError::FieldEncodingError(format!(
                    "expected 1 FE for bool, got {}",
                    field_elements.len()
                )));
            }
            let value = match field_elements[0].as_canonical_u32() {
                0 => false,
                1 => true,
                other => {
                    return Err(TabulaError::FieldEncodingError(format!(
                        "invalid bool FE: {other}"
                    )));
                }
            };
            Ok(bool_typed(value))
        }
        TYPE_U64_ID => {
            let limbs: &[KoalaBear; 3] = field_elements
                .try_into()
                .map_err(|_| TabulaError::FieldEncodingError("expected 3 FEs".to_string()))?;
            Ok(u64_typed(decode_u64_limbs(limbs)?))
        }
        TYPE_I64_ID => {
            let limbs: &[KoalaBear; 3] = field_elements
                .try_into()
                .map_err(|_| TabulaError::FieldEncodingError("expected 3 FEs".to_string()))?;
            let offset = decode_u64_limbs(limbs)?;
            Ok(i64_typed((offset as i128 - (1i128 << 63)) as i64))
        }
        TYPE_BYTES32_ID => {
            if field_elements.len() != 8 {
                return Err(TabulaError::FieldEncodingError(format!(
                    "expected 8 FEs for Bytes32, got {}",
                    field_elements.len()
                )));
            }
            let mut bytes = [0u8; 32];
            for (i, fe) in field_elements.iter().enumerate() {
                bytes[i * 4..i * 4 + 4].copy_from_slice(&fe.as_canonical_u32().to_le_bytes());
            }
            Ok(bytes32_typed(bytes))
        }
        other => Err(TabulaError::TypeMismatch {
            expected: "registered built-in type".to_string(),
            actual: format!("type {}", other.0),
        }),
    }
}

#[cfg(test)]
pub(super) fn seeded_field_elements_width(type_id: TypeId) -> Result<usize, TabulaError> {
    Ok(match type_id {
        TYPE_BOOL_ID => 1,
        TYPE_U64_ID | TYPE_I64_ID => 3,
        TYPE_BYTES32_ID => 8,
        other => {
            return Err(TabulaError::TypeMismatch {
                expected: "registered built-in type".to_string(),
                actual: format!("type {}", other.0),
            });
        }
    })
}

#[cfg(test)]
pub(super) fn seeded_trace_width(type_id: TypeId) -> Result<usize, TabulaError> {
    Ok(seeded_field_elements_width(type_id)? + 1)
}

#[cfg(test)]
pub(super) fn encode_seeded_trace(
    type_id: TypeId,
    value: &TypedValue,
    is_null: bool,
) -> Result<Vec<KoalaBear>, TabulaError> {
    let width = seeded_field_elements_width(type_id)?;
    let mut fes = Vec::with_capacity(width + 1);
    if is_null {
        fes.resize(width, KoalaBear::ZERO);
        fes.push(KoalaBear::ONE);
    } else {
        fes.extend(encode_seeded_field_elements(value)?);
        fes.push(KoalaBear::ZERO);
    }
    Ok(fes)
}

#[cfg(test)]
pub(super) fn decode_seeded_trace(
    type_id: TypeId,
    field_elements: &[KoalaBear],
) -> Result<(TypedValue, bool), TabulaError> {
    let expected = seeded_trace_width(type_id)?;
    if field_elements.len() != expected {
        return Err(TabulaError::FieldEncodingError(format!(
            "trace decode: expected {expected} field elements for type {}, got {}",
            type_id.0,
            field_elements.len()
        )));
    }

    let null_flag = field_elements[expected - 1].as_canonical_u32();
    match null_flag {
        1 => Ok((seeded_zero_typed(type_id)?, true)),
        0 => {
            let width = seeded_field_elements_width(type_id)?;
            Ok((
                decode_seeded_field_elements(type_id, &field_elements[..width])?,
                false,
            ))
        }
        other => Err(TabulaError::FieldEncodingError(format!(
            "invalid val_is_null flag: {other}"
        ))),
    }
}

pub(super) fn ensure_type(value: &TypedValue, expected: TypeId) -> Result<(), TabulaError> {
    if value.type_id() != expected {
        return Err(TabulaError::TypeMismatch {
            expected: format!("type {}", expected.0),
            actual: format!("type {}", value.type_id().0),
        });
    }
    Ok(())
}

#[cfg(test)]
pub(super) fn seeded_zero_typed(type_id: TypeId) -> Result<TypedValue, TabulaError> {
    Ok(match type_id {
        TYPE_U64_ID => u64_typed(0),
        TYPE_I64_ID => i64_typed(0),
        TYPE_BOOL_ID => bool_typed(false),
        TYPE_BYTES32_ID => bytes32_typed([0; 32]),
        other => {
            return Err(TabulaError::TypeMismatch {
                expected: "registered built-in type".to_string(),
                actual: format!("type {}", other.0),
            });
        }
    })
}

pub(super) fn decode_u64_payload(value: &TypedValue, expected: TypeId) -> Result<u64, TabulaError> {
    ensure_type(value, expected)?;
    u64::try_from_slice(value.payload()).map_err(|err| TabulaError::Custom(err.to_string()))
}

pub(super) fn decode_i64_payload(value: &TypedValue, expected: TypeId) -> Result<i64, TabulaError> {
    ensure_type(value, expected)?;
    i64::try_from_slice(value.payload()).map_err(|err| TabulaError::Custom(err.to_string()))
}

pub(super) fn decode_bool_payload(
    value: &TypedValue,
    expected: TypeId,
) -> Result<bool, TabulaError> {
    ensure_type(value, expected)?;
    bool::try_from_slice(value.payload()).map_err(|err| TabulaError::Custom(err.to_string()))
}

pub(super) fn decode_bytes32_payload(
    value: &TypedValue,
    expected: TypeId,
) -> Result<[u8; 32], TabulaError> {
    ensure_type(value, expected)?;
    <[u8; 32]>::try_from_slice(value.payload()).map_err(|err| TabulaError::Custom(err.to_string()))
}

const LIMB0_BITS: u32 = 30;
const LIMB1_BITS: u32 = 30;
const LIMB2_BITS: u32 = 4;
const LIMB0_MASK: u64 = (1u64 << LIMB0_BITS) - 1;
const LIMB1_MASK: u64 = (1u64 << LIMB1_BITS) - 1;
const LIMB2_MASK: u64 = (1u64 << LIMB2_BITS) - 1;

pub(super) fn encode_u64_limbs(value: u64) -> [KoalaBear; 3] {
    let limb0 = (value & LIMB0_MASK) as u32;
    let limb1 = ((value >> LIMB0_BITS) & LIMB1_MASK) as u32;
    let limb2 = ((value >> (LIMB0_BITS + LIMB1_BITS)) & LIMB2_MASK) as u32;
    [
        KoalaBear::new(limb0),
        KoalaBear::new(limb1),
        KoalaBear::new(limb2),
    ]
}

pub(super) fn decode_u64_limbs(limbs: &[KoalaBear; 3]) -> Result<u64, TabulaError> {
    let limb0 = limbs[0].as_canonical_u32() as u64;
    let limb1 = limbs[1].as_canonical_u32() as u64;
    let limb2 = limbs[2].as_canonical_u32() as u64;
    if limb0 > LIMB0_MASK || limb1 > LIMB1_MASK || limb2 > LIMB2_MASK {
        return Err(TabulaError::FieldEncodingError(
            "u64 limb exceeds canonical range".to_string(),
        ));
    }
    Ok(limb0 | (limb1 << LIMB0_BITS) | (limb2 << (LIMB0_BITS + LIMB1_BITS)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field_round_trip(value: &TypedValue, type_id: TypeId) {
        let encoded = encode_seeded_field_elements(value).unwrap();
        assert_eq!(
            encoded.len(),
            seeded_field_elements_width(type_id).unwrap(),
            "field width mismatch for type {}",
            type_id.0
        );
        let decoded = decode_seeded_field_elements(type_id, &encoded).unwrap();
        assert_eq!(*value, decoded, "field round-trip failed for {value:?}");
    }

    fn trace_round_trip(value: &TypedValue, type_id: TypeId, is_null: bool) {
        let encoded = encode_seeded_trace(type_id, value, is_null).unwrap();
        assert_eq!(
            encoded.len(),
            seeded_trace_width(type_id).unwrap(),
            "trace width mismatch for type {}",
            type_id.0
        );
        let (decoded, decoded_null) = decode_seeded_trace(type_id, &encoded).unwrap();
        assert_eq!(decoded_null, is_null);
        if is_null {
            assert_eq!(decoded, seeded_zero_typed(type_id).unwrap());
        } else {
            assert_eq!(decoded, *value);
        }
    }

    #[test]
    fn builtin_field_encoding_round_trips() {
        field_round_trip(&bool_typed(true), TYPE_BOOL_ID);
        field_round_trip(&bool_typed(false), TYPE_BOOL_ID);
        for value in [0u64, 1, 42, (1 << 31) - 1, 1 << 31, 1 << 62, u64::MAX] {
            field_round_trip(&u64_typed(value), TYPE_U64_ID);
        }
        for value in [i64::MIN, -1000, -1, 0, 1, 1000, i64::MAX] {
            field_round_trip(&i64_typed(value), TYPE_I64_ID);
        }
        let mut bytes = [0u8; 32];
        for (i, chunk) in bytes.chunks_exact_mut(4).enumerate() {
            chunk.copy_from_slice(&((i as u32) * 1000).to_le_bytes());
        }
        field_round_trip(&bytes32_typed([0; 32]), TYPE_BYTES32_ID);
        field_round_trip(&bytes32_typed(bytes), TYPE_BYTES32_ID);
    }

    #[test]
    fn builtin_bytes32_rejects_non_canonical_chunk() {
        let mut bytes = [0u8; 32];
        bytes[0..4].copy_from_slice(&KoalaBear::ORDER_U32.to_le_bytes());
        assert!(encode_seeded_field_elements(&bytes32_typed(bytes)).is_err());
    }

    #[test]
    fn builtin_trace_round_trips() {
        trace_round_trip(&bool_typed(true), TYPE_BOOL_ID, false);
        trace_round_trip(&bool_typed(false), TYPE_BOOL_ID, true);
        trace_round_trip(&u64_typed(42), TYPE_U64_ID, false);
        trace_round_trip(&u64_typed(0), TYPE_U64_ID, true);
        trace_round_trip(&i64_typed(-999), TYPE_I64_ID, false);
        trace_round_trip(&i64_typed(0), TYPE_I64_ID, true);
        let mut bytes = [0u8; 32];
        bytes[0..4].copy_from_slice(&100u32.to_le_bytes());
        trace_round_trip(&bytes32_typed(bytes), TYPE_BYTES32_ID, false);
        trace_round_trip(&bytes32_typed([0; 32]), TYPE_BYTES32_ID, true);
    }

    #[test]
    fn builtin_trace_decode_rejects_wrong_width() {
        let field_elements = vec![KoalaBear::ZERO; 3];
        assert!(decode_seeded_trace(TYPE_BOOL_ID, &field_elements).is_err());
    }
}
