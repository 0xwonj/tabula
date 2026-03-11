//! BabyBear value codec: Tier 1 (ComEnc) encoding for field-element commitments.

use p3_baby_bear::BabyBear;
use p3_field::{PrimeCharacteristicRing, PrimeField32};

use tabula_core::error::TabulaError;
use tabula_core::traits::ValueCodec;
use tabula_core::{Value, ValueType, zero_value};

use crate::field::{decode_u64_limbs, encode_u64_limbs};

/// Tier 1 (ComEnc) codec: encodes `Value` into BabyBear field elements.
///
/// Width table:
/// - Bool   → 1 FE (`{0, 1}`)
/// - U64    → 3 FE (30+30+4 bit limbs)
/// - I64    → 3 FE (offset encoding: `val + 2^63` → 3-limb U64)
/// - Bytes32 → 8 FE (4 LE bytes per FE, rejecting non-canonical chunks)
#[derive(Clone, Debug)]
pub struct BabyBearCodec;

/// Tier 2 (TraceEnc) width: `w(T) + 1` (value FEs + val_is_null flag).
pub fn trace_width(codec: &BabyBearCodec, value_type: ValueType) -> usize {
    codec.field_elements_per(value_type) + 1
}

/// Tier 2 (TraceEnc) encoding: ComEnc(value) ++ val_is_null flag.
///
/// - `is_null=false` → `ComEnc(value) ++ [0]`
/// - `is_null=true`  → `[0; w(T)] ++ [1]` (canonical zero encoding)
pub fn encode_trace(
    value: &Value,
    is_null: bool,
    value_type: ValueType,
    codec: &BabyBearCodec,
) -> Result<Vec<BabyBear>, TabulaError> {
    let w = codec.field_elements_per(value_type);
    let mut fes = Vec::with_capacity(w + 1);
    if is_null {
        fes.resize(w, BabyBear::ZERO);
        fes.push(BabyBear::ONE);
    } else {
        fes.extend(codec.encode(value)?);
        fes.push(BabyBear::ZERO);
    }
    Ok(fes)
}

/// Tier 2 (TraceEnc) decoding: inverse of `encode_trace`.
///
/// Returns `(value, is_null)`. When null, returns the canonical zero value.
pub fn decode_trace(
    fes: &[BabyBear],
    value_type: ValueType,
    codec: &BabyBearCodec,
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

impl ValueCodec for BabyBearCodec {
    type FieldRepr = BabyBear;

    fn encode(&self, value: &Value) -> Result<Vec<BabyBear>, TabulaError> {
        Ok(match value {
            Value::Bool(b) => {
                vec![if *b { BabyBear::ONE } else { BabyBear::ZERO }]
            }
            Value::U64(n) => encode_u64_limbs(*n).to_vec(),
            Value::I64(n) => {
                // Offset encoding: val + 2^63 → u64 → 3-limb.
                // Maps i64::MIN → 0, 0 → 2^63, i64::MAX → 2^64 - 1.
                let offset = (*n as i128 + (1i128 << 63)) as u64;
                encode_u64_limbs(offset).to_vec()
            }
            Value::Bytes32(b) => {
                let mut fes = Vec::with_capacity(8);
                for (i, chunk) in b.chunks_exact(4).enumerate() {
                    let val = u32::from_le_bytes(chunk.try_into().unwrap());
                    if val >= BabyBear::ORDER_U32 {
                        return Err(TabulaError::FieldEncodingError(format!(
                            "Bytes32 chunk {i}: {val} >= BabyBear modulus {}",
                            BabyBear::ORDER_U32
                        )));
                    }
                    fes.push(BabyBear::new(val));
                }
                fes
            }
        })
    }

    fn decode(
        &self,
        field_elements: &[BabyBear],
        target_type: ValueType,
    ) -> Result<Value, TabulaError> {
        let expected = self.field_elements_per(target_type);
        if field_elements.len() != expected {
            return Err(TabulaError::FieldEncodingError(format!(
                "expected {expected} FEs for {target_type}, got {}",
                field_elements.len()
            )));
        }

        match target_type {
            ValueType::Bool => {
                let val = field_elements[0].as_canonical_u32();
                match val {
                    0 => Ok(Value::Bool(false)),
                    1 => Ok(Value::Bool(true)),
                    _ => Err(TabulaError::FieldEncodingError(format!(
                        "invalid bool FE: {val}"
                    ))),
                }
            }
            ValueType::U64 => {
                let limbs: &[BabyBear; 3] = field_elements
                    .try_into()
                    .map_err(|_| TabulaError::FieldEncodingError("expected 3 FEs".into()))?;
                let val = decode_u64_limbs(limbs)?;
                Ok(Value::U64(val))
            }
            ValueType::I64 => {
                let limbs: &[BabyBear; 3] = field_elements
                    .try_into()
                    .map_err(|_| TabulaError::FieldEncodingError("expected 3 FEs".into()))?;
                let offset = decode_u64_limbs(limbs)?;
                let val = (offset as i128 - (1i128 << 63)) as i64;
                Ok(Value::I64(val))
            }
            ValueType::Bytes32 => {
                let mut bytes = [0u8; 32];
                for (i, fe) in field_elements.iter().enumerate() {
                    bytes[i * 4..i * 4 + 4].copy_from_slice(&fe.as_canonical_u32().to_le_bytes());
                }
                Ok(Value::Bytes32(bytes))
            }
        }
    }

    fn field_elements_per(&self, value_type: ValueType) -> usize {
        match value_type {
            ValueType::Bool => 1,
            ValueType::U64 | ValueType::I64 => 3,
            ValueType::Bytes32 => 8,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::field::decode_u64_limbs;

    fn round_trip(value: Value, ty: ValueType) {
        let codec = BabyBearCodec;
        let fes = codec.encode(&value).unwrap();
        assert_eq!(
            fes.len(),
            codec.field_elements_per(ty),
            "width mismatch for {ty:?}"
        );
        let decoded = codec.decode(&fes, ty).unwrap();
        assert_eq!(value, decoded, "round-trip failed for {value:?}");
    }

    #[test]
    fn encode_decode_bool() {
        round_trip(Value::Bool(true), ValueType::Bool);
        round_trip(Value::Bool(false), ValueType::Bool);
    }

    #[test]
    fn encode_decode_u64() {
        for val in [0u64, 1, 42, (1 << 31) - 1, 1 << 31, 1 << 62, u64::MAX] {
            round_trip(Value::U64(val), ValueType::U64);
        }
    }

    #[test]
    fn encode_decode_i64() {
        for val in [i64::MIN, -1000, -1, 0, 1, 1000, i64::MAX] {
            round_trip(Value::I64(val), ValueType::I64);
        }
    }

    #[test]
    fn encode_decode_bytes32() {
        // All-zeros (valid).
        round_trip(Value::Bytes32([0; 32]), ValueType::Bytes32);

        // Non-zero but valid: each 4-byte chunk < p.
        let mut bytes = [0u8; 32];
        for (i, chunk) in bytes.chunks_exact_mut(4).enumerate() {
            chunk.copy_from_slice(&((i as u32) * 1000).to_le_bytes());
        }
        round_trip(Value::Bytes32(bytes), ValueType::Bytes32);
    }

    #[test]
    fn bytes32_non_canonical_rejected() {
        let codec = BabyBearCodec;
        let mut bytes = [0u8; 32];
        // Set first 4 bytes to ORDER_U32 (non-canonical).
        bytes[0..4].copy_from_slice(&BabyBear::ORDER_U32.to_le_bytes());
        assert!(codec.encode(&Value::Bytes32(bytes)).is_err());
    }

    #[test]
    fn i64_offset_preserves_ordering() {
        let codec = BabyBearCodec;
        let pairs: &[(i64, i64)] = &[
            (i64::MIN, i64::MIN + 1),
            (-1, 0),
            (0, 1),
            (i64::MAX - 1, i64::MAX),
            (-100, 100),
        ];
        for &(a, b) in pairs {
            let fes_a = codec.encode(&Value::I64(a)).unwrap();
            let fes_b = codec.encode(&Value::I64(b)).unwrap();
            let arr_a: &[BabyBear; 3] = fes_a.as_slice().try_into().unwrap();
            let arr_b: &[BabyBear; 3] = fes_b.as_slice().try_into().unwrap();
            let offset_a = decode_u64_limbs(arr_a).unwrap();
            let offset_b = decode_u64_limbs(arr_b).unwrap();
            assert!(
                offset_a < offset_b,
                "ordering not preserved: i64 {a} < {b} but offsets {offset_a} >= {offset_b}"
            );
        }
    }

    #[test]
    fn width_correctness() {
        let codec = BabyBearCodec;
        assert_eq!(codec.field_elements_per(ValueType::Bool), 1);
        assert_eq!(codec.field_elements_per(ValueType::U64), 3);
        assert_eq!(codec.field_elements_per(ValueType::I64), 3);
        assert_eq!(codec.field_elements_per(ValueType::Bytes32), 8);
    }

    #[test]
    fn decode_wrong_width_rejected() {
        let codec = BabyBearCodec;
        let fes = vec![BabyBear::ZERO; 2];
        assert!(codec.decode(&fes, ValueType::Bool).is_err());
        assert!(codec.decode(&fes, ValueType::U64).is_err());
    }

    #[test]
    fn decode_bool_non_binary_rejected() {
        let codec = BabyBearCodec;
        let fes = vec![BabyBear::new(2)];
        assert!(codec.decode(&fes, ValueType::Bool).is_err());
    }

    // ── Tier 2 (TraceEnc) tests ──────────────────────────────────────────

    fn trace_round_trip(value: Value, is_null: bool, ty: ValueType) {
        let codec = BabyBearCodec;
        let fes = super::encode_trace(&value, is_null, ty, &codec).unwrap();
        assert_eq!(fes.len(), super::trace_width(&codec, ty));
        let (decoded, decoded_null) = super::decode_trace(&fes, ty, &codec).unwrap();
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
        let codec = BabyBearCodec;
        assert_eq!(super::trace_width(&codec, ValueType::Bool), 2); // 1 + 1
        assert_eq!(super::trace_width(&codec, ValueType::U64), 4); // 3 + 1
        assert_eq!(super::trace_width(&codec, ValueType::I64), 4); // 3 + 1
        assert_eq!(super::trace_width(&codec, ValueType::Bytes32), 9); // 8 + 1
    }

    #[test]
    fn trace_null_i64_canonical_zero() {
        // I64 null must encode as (0,0,0,1), NOT as offset-encoded I64(0) = (0,0,8,1)
        let codec = BabyBearCodec;
        let fes = super::encode_trace(&Value::I64(0), true, ValueType::I64, &codec).unwrap();
        // First 3 FEs should all be zero (canonical zero), not offset-encoded
        assert_eq!(fes[0], BabyBear::ZERO);
        assert_eq!(fes[1], BabyBear::ZERO);
        assert_eq!(fes[2], BabyBear::ZERO);
        assert_eq!(fes[3], BabyBear::ONE); // null flag
    }

    #[test]
    fn trace_non_null_i64_uses_offset_encoding() {
        let codec = BabyBearCodec;
        let fes = super::encode_trace(&Value::I64(0), false, ValueType::I64, &codec).unwrap();
        // I64(0) → offset 2^63 → (0, 0, 8) via limb encoding
        let limbs: &[BabyBear; 3] = fes[..3].try_into().unwrap();
        let offset = decode_u64_limbs(limbs).unwrap();
        assert_eq!(offset, 1u64 << 63);
        assert_eq!(fes[3], BabyBear::ZERO); // not null
    }

    #[test]
    fn trace_decode_wrong_width_rejected() {
        let codec = BabyBearCodec;
        let fes = vec![BabyBear::ZERO; 3]; // Wrong width for Bool (should be 2)
        assert!(super::decode_trace(&fes, ValueType::Bool, &codec).is_err());
    }
}
