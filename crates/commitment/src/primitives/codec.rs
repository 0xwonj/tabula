//! KoalaBear value codec: Tier 1 (ComEnc) encoding for field-element commitments.

use p3_field::{PrimeCharacteristicRing, PrimeField32};
use p3_koala_bear::KoalaBear;

use tabula_core::error::TabulaError;
use tabula_core::traits::ValueCodec;
use tabula_core::{Value, ValueType};

use super::field::{decode_u64_limbs, encode_u64_limbs};

/// Tier 1 (ComEnc) codec: encodes `Value` into KoalaBear field elements.
///
/// Width table:
/// - Bool   → 1 FE (`{0, 1}`)
/// - U64    → 3 FE (30+30+4 bit limbs)
/// - I64    → 3 FE (offset encoding: `val + 2^63` → 3-limb U64)
/// - Bytes32 → 8 FE (4 LE bytes per FE, rejecting non-canonical chunks)
#[derive(Clone, Debug)]
pub struct KoalaBearCodec;

impl ValueCodec for KoalaBearCodec {
    type FieldRepr = KoalaBear;

    fn encode(&self, value: &Value) -> Result<Vec<KoalaBear>, TabulaError> {
        Ok(match value {
            Value::Bool(b) => {
                vec![if *b { KoalaBear::ONE } else { KoalaBear::ZERO }]
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
                    if val >= KoalaBear::ORDER_U32 {
                        return Err(TabulaError::FieldEncodingError(format!(
                            "Bytes32 chunk {i}: {val} >= KoalaBear modulus {}",
                            KoalaBear::ORDER_U32
                        )));
                    }
                    fes.push(KoalaBear::new(val));
                }
                fes
            }
        })
    }

    fn decode(
        &self,
        field_elements: &[KoalaBear],
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
                let limbs: &[KoalaBear; 3] = field_elements
                    .try_into()
                    .map_err(|_| TabulaError::FieldEncodingError("expected 3 FEs".into()))?;
                let val = decode_u64_limbs(limbs)?;
                Ok(Value::U64(val))
            }
            ValueType::I64 => {
                let limbs: &[KoalaBear; 3] = field_elements
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
    use super::super::field::decode_u64_limbs;
    use super::*;

    fn round_trip(value: Value, ty: ValueType) {
        let codec = KoalaBearCodec;
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
        let codec = KoalaBearCodec;
        let mut bytes = [0u8; 32];
        // Set first 4 bytes to ORDER_U32 (non-canonical).
        bytes[0..4].copy_from_slice(&KoalaBear::ORDER_U32.to_le_bytes());
        assert!(codec.encode(&Value::Bytes32(bytes)).is_err());
    }

    #[test]
    fn i64_offset_preserves_ordering() {
        let codec = KoalaBearCodec;
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
            let arr_a: &[KoalaBear; 3] = fes_a.as_slice().try_into().unwrap();
            let arr_b: &[KoalaBear; 3] = fes_b.as_slice().try_into().unwrap();
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
        let codec = KoalaBearCodec;
        assert_eq!(codec.field_elements_per(ValueType::Bool), 1);
        assert_eq!(codec.field_elements_per(ValueType::U64), 3);
        assert_eq!(codec.field_elements_per(ValueType::I64), 3);
        assert_eq!(codec.field_elements_per(ValueType::Bytes32), 8);
    }

    #[test]
    fn decode_wrong_width_rejected() {
        let codec = KoalaBearCodec;
        let fes = vec![KoalaBear::ZERO; 2];
        assert!(codec.decode(&fes, ValueType::Bool).is_err());
        assert!(codec.decode(&fes, ValueType::U64).is_err());
    }

    #[test]
    fn decode_bool_non_binary_rejected() {
        let codec = KoalaBearCodec;
        let fes = vec![KoalaBear::new(2)];
        assert!(codec.decode(&fes, ValueType::Bool).is_err());
    }
}
