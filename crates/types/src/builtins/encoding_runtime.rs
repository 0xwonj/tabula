use std::sync::Arc;

use p3_koala_bear::KoalaBear;
use tabula_core::EncodingProfileId;
use tabula_core::error::TabulaError;
use tabula_profile::{
    ENCODING_BOOL_ID, ENCODING_BYTES32_ID, ENCODING_I64_ID, ENCODING_U64_ID, builtin_catalog,
};

use super::codecs::{decode_seeded_field_elements, encode_seeded_field_elements};
use super::lookup_encoding;
use crate::{EncodingRuntime, OrderedKeySegmentKind, TypedValue};

/// Build the seeded standard built-in encoding runtime registrations.
pub fn builtin_encoding_runtimes() -> Result<Vec<Arc<dyn EncodingRuntime>>, TabulaError> {
    let catalog = builtin_catalog().map_err(|err| TabulaError::Custom(err.to_string()))?;
    Ok(vec![
        Arc::new(BuiltinEncodingRuntime::new(
            ENCODING_U64_ID,
            lookup_encoding(&catalog, ENCODING_U64_ID)?,
        )),
        Arc::new(BuiltinEncodingRuntime::new(
            ENCODING_I64_ID,
            lookup_encoding(&catalog, ENCODING_I64_ID)?,
        )),
        Arc::new(BuiltinEncodingRuntime::new(
            ENCODING_BOOL_ID,
            lookup_encoding(&catalog, ENCODING_BOOL_ID)?,
        )),
        Arc::new(BuiltinEncodingRuntime::new(
            ENCODING_BYTES32_ID,
            lookup_encoding(&catalog, ENCODING_BYTES32_ID)?,
        )),
    ])
}

struct BuiltinEncodingRuntime {
    encoding_profile_id: EncodingProfileId,
    descriptor: tabula_profile::EncodingProfile,
}

impl BuiltinEncodingRuntime {
    fn new(
        encoding_profile_id: EncodingProfileId,
        descriptor: tabula_profile::EncodingProfile,
    ) -> Self {
        Self {
            encoding_profile_id,
            descriptor,
        }
    }
}

impl EncodingRuntime for BuiltinEncodingRuntime {
    fn encoding_profile_id(&self) -> EncodingProfileId {
        self.encoding_profile_id
    }

    fn descriptor(&self) -> &tabula_profile::EncodingProfile {
        &self.descriptor
    }

    fn encode_field_elements(&self, value: &TypedValue) -> Result<Vec<KoalaBear>, TabulaError> {
        encode_seeded_field_elements(value)
    }

    fn encode_key_payload_elements(
        &self,
        value: &TypedValue,
    ) -> Result<Vec<KoalaBear>, TabulaError> {
        use tabula_profile::{TYPE_BOOL_ID, TYPE_I64_ID, TYPE_U64_ID};

        match value.type_id() {
            TYPE_U64_ID | TYPE_I64_ID => {
                let mut payload = encode_seeded_field_elements(value)?;
                payload.reverse();
                Ok(payload)
            }
            TYPE_BOOL_ID => encode_seeded_field_elements(value),
            _ => encode_seeded_field_elements(value),
        }
    }

    fn decode_field_elements(
        &self,
        field_elements: &[KoalaBear],
    ) -> Result<TypedValue, TabulaError> {
        decode_seeded_field_elements(self.descriptor.type_id, field_elements)
    }

    fn encode_transcript_atoms(&self, value: &TypedValue) -> Result<Vec<KoalaBear>, TabulaError> {
        self.encode_field_elements(value)
    }

    fn encode_committed_bytes(&self, value: &TypedValue) -> Result<Vec<u8>, TabulaError> {
        use tabula_profile::{TYPE_BOOL_ID, TYPE_BYTES32_ID, TYPE_I64_ID, TYPE_U64_ID};

        match value.type_id() {
            TYPE_BOOL_ID => Ok(vec![u8::from(super::codecs::decode_bool_payload(
                value,
                TYPE_BOOL_ID,
            )?)]),
            TYPE_U64_ID => Ok(super::codecs::decode_u64_payload(value, TYPE_U64_ID)?
                .to_be_bytes()
                .to_vec()),
            TYPE_I64_ID => {
                let offset = (super::codecs::decode_i64_payload(value, TYPE_I64_ID)? as i128
                    + (1i128 << 63)) as u64;
                Ok(offset.to_be_bytes().to_vec())
            }
            TYPE_BYTES32_ID => {
                Ok(super::codecs::decode_bytes32_payload(value, TYPE_BYTES32_ID)?.to_vec())
            }
            other => Err(TabulaError::TypeMismatch {
                expected: "registered built-in type".to_string(),
                actual: format!("type {}", other.0),
            }),
        }
    }

    fn decode_committed_bytes(&self, bytes: &[u8]) -> Result<TypedValue, TabulaError> {
        use tabula_profile::{TYPE_BOOL_ID, TYPE_BYTES32_ID, TYPE_I64_ID, TYPE_U64_ID};

        match self.descriptor.type_id {
            TYPE_BOOL_ID => {
                let [byte] = bytes.try_into().map_err(|_| {
                    TabulaError::FieldEncodingError(format!(
                        "expected 1 committed byte for bool, got {}",
                        bytes.len()
                    ))
                })?;
                match byte {
                    0 => Ok(crate::bool_typed(false)),
                    1 => Ok(crate::bool_typed(true)),
                    other => Err(TabulaError::FieldEncodingError(format!(
                        "invalid committed bool byte {other}"
                    ))),
                }
            }
            TYPE_U64_ID => {
                let payload: [u8; 8] = bytes.try_into().map_err(|_| {
                    TabulaError::FieldEncodingError(format!(
                        "expected 8 committed bytes for u64, got {}",
                        bytes.len()
                    ))
                })?;
                Ok(crate::u64_typed(u64::from_be_bytes(payload)))
            }
            TYPE_I64_ID => {
                let payload: [u8; 8] = bytes.try_into().map_err(|_| {
                    TabulaError::FieldEncodingError(format!(
                        "expected 8 committed bytes for i64, got {}",
                        bytes.len()
                    ))
                })?;
                let offset = u64::from_be_bytes(payload);
                Ok(crate::i64_typed((offset as i128 - (1i128 << 63)) as i64))
            }
            TYPE_BYTES32_ID => {
                let payload: [u8; 32] = bytes.try_into().map_err(|_| {
                    TabulaError::FieldEncodingError(format!(
                        "expected 32 committed bytes for bytes32, got {}",
                        bytes.len()
                    ))
                })?;
                Ok(crate::bytes32_typed(payload))
            }
            other => Err(TabulaError::TypeMismatch {
                expected: "registered built-in type".to_string(),
                actual: format!("type {}", other.0),
            }),
        }
    }

    fn trace_width(&self) -> usize {
        self.descriptor.width as usize
    }

    fn ordered_key_segment_kind(&self) -> Option<OrderedKeySegmentKind> {
        use tabula_profile::{TYPE_BOOL_ID, TYPE_I64_ID, TYPE_U64_ID};

        if !self.descriptor.key_eligible || !self.descriptor.ordering_preserving {
            return None;
        }

        match self.descriptor.type_id {
            TYPE_U64_ID => Some(OrderedKeySegmentKind::U64Limbs3),
            TYPE_I64_ID => Some(OrderedKeySegmentKind::I64OffsetLimbs3),
            TYPE_BOOL_ID => Some(OrderedKeySegmentKind::Bool1),
            _ => None,
        }
    }
}
