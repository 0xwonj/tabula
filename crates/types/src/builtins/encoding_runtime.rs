use std::sync::Arc;

use p3_koala_bear::KoalaBear;
use tabula_core::EncodingProfileId;
use tabula_core::error::TabulaError;
use tabula_profile::{
    ENCODING_BOOL_ID, ENCODING_BYTES32_ID, ENCODING_I64_ID, ENCODING_U64_ID, builtin_catalog,
};

use super::codecs::{decode_seeded_field_elements, encode_seeded_field_elements};
use super::lookup_encoding;
use crate::{EncodingRuntime, TypedValue};

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

    fn decode_field_elements(
        &self,
        field_elements: &[KoalaBear],
    ) -> Result<TypedValue, TabulaError> {
        decode_seeded_field_elements(self.descriptor.type_id, field_elements)
    }

    fn encode_transcript_atoms(&self, value: &TypedValue) -> Result<Vec<KoalaBear>, TabulaError> {
        self.encode_field_elements(value)
    }

    fn trace_width(&self) -> usize {
        self.descriptor.width as usize
    }
}
