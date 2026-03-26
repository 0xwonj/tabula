use tabula_core::error::TabulaError;
use tabula_types::{EncodingRuntimeRegistry, TypeRuntimeRegistry};

#[derive(Clone)]
pub(crate) struct RegistrationContext {
    pub(crate) type_runtimes: TypeRuntimeRegistry,
    pub(crate) encoding_runtimes: EncodingRuntimeRegistry,
}

impl RegistrationContext {
    pub(crate) fn builtin() -> Result<Self, TabulaError> {
        Ok(Self {
            type_runtimes: TypeRuntimeRegistry::seeded()?,
            encoding_runtimes: EncodingRuntimeRegistry::seeded()?,
        })
    }
}
