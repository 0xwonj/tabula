use std::collections::BTreeMap;
use std::sync::Arc;

use p3_koala_bear::KoalaBear;

use tabula_core::EncodingProfileId;
use tabula_core::TypeId;
use tabula_core::error::TabulaError;
use tabula_profile::EncodingProfile;

use crate::TypedValue;

/// Runtime encoding behavior for one registered encoding profile.
pub trait EncodingRuntime: Send + Sync {
    /// Encoding profile identifier.
    fn encoding_profile_id(&self) -> EncodingProfileId;

    /// Semantic descriptor backing this runtime encoding.
    fn descriptor(&self) -> &EncodingProfile;

    /// Encode one typed value into machine field elements.
    fn encode_field_elements(&self, value: &TypedValue) -> Result<Vec<KoalaBear>, TabulaError>;

    /// Decode machine field elements into one typed value.
    fn decode_field_elements(
        &self,
        field_elements: &[KoalaBear],
    ) -> Result<TypedValue, TabulaError>;

    /// Encode the transcript payload atoms for this value.
    fn encode_transcript_atoms(&self, value: &TypedValue) -> Result<Vec<KoalaBear>, TabulaError>;

    /// Fixed trace width contributed by this encoding profile.
    fn trace_width(&self) -> usize;
}

/// Process-local registry of runtime encoding behavior.
#[derive(Clone, Default)]
pub struct EncodingRuntimeRegistry {
    runtimes: BTreeMap<EncodingProfileId, Arc<dyn EncodingRuntime>>,
}

impl EncodingRuntimeRegistry {
    /// Create an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Seed the registry with standard built-in encoding runtimes.
    pub fn seeded() -> Result<Self, TabulaError> {
        let mut registry = Self::new();
        for runtime in crate::builtins::builtin_encoding_runtimes()? {
            registry.register(runtime)?;
        }
        Ok(registry)
    }

    /// Register one runtime encoding.
    pub fn register(&mut self, runtime: Arc<dyn EncodingRuntime>) -> Result<(), TabulaError> {
        let encoding_profile_id = runtime.encoding_profile_id();
        if self.runtimes.insert(encoding_profile_id, runtime).is_some() {
            return Err(TabulaError::Custom(format!(
                "duplicate encoding runtime registration for encoding profile {}",
                encoding_profile_id.0
            )));
        }
        Ok(())
    }

    /// Resolve one encoding runtime or fail closed.
    pub fn resolve(
        &self,
        encoding_profile_id: EncodingProfileId,
    ) -> Result<&Arc<dyn EncodingRuntime>, TabulaError> {
        self.runtimes.get(&encoding_profile_id).ok_or_else(|| {
            TabulaError::Custom(format!(
                "missing runtime encoding implementation for encoding profile {}",
                encoding_profile_id.0
            ))
        })
    }

    /// Resolve the unique encoding runtime installed for one type id.
    pub fn resolve_for_type(
        &self,
        type_id: TypeId,
    ) -> Result<&Arc<dyn EncodingRuntime>, TabulaError> {
        let mut matches = self
            .runtimes
            .values()
            .filter(|runtime| runtime.descriptor().type_id == type_id);
        let Some(runtime) = matches.next() else {
            return Err(TabulaError::Custom(format!(
                "missing runtime encoding implementation for type {}",
                type_id.0
            )));
        };
        if matches.next().is_some() {
            return Err(TabulaError::Custom(format!(
                "multiple runtime encodings registered for type {}; explicit encoding profile required",
                type_id.0
            )));
        }
        Ok(runtime)
    }

    /// Snapshot all registered encoding runtimes.
    #[must_use]
    pub fn entries(&self) -> Vec<Arc<dyn EncodingRuntime>> {
        self.runtimes.values().cloned().collect()
    }
}
