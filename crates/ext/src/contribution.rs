//! Individual extension contributions: types, encodings, schemes, and capabilities.

use std::sync::Arc;

use tabula_compiler::SourceCapabilityDescriptor;
use tabula_core::{EncodingProfileId, TypeId};
use tabula_profile::{EncodingProfile, SchemeProfile, TypeDescriptor};
use tabula_types::{EncodingRuntime, TypeRuntime};

#[cfg(feature = "verify")]
use crate::scheme::ColumnBackendFactoryBundle;

/// One public capability contribution bundled into an extension install.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capability {
    pub(crate) descriptor: SourceCapabilityDescriptor,
}

impl Capability {
    /// Create a capability contribution from a compiler-level source descriptor.
    pub fn new(descriptor: SourceCapabilityDescriptor) -> Self {
        Self { descriptor }
    }

    /// Borrow the underlying source capability descriptor.
    pub fn descriptor(&self) -> &SourceCapabilityDescriptor {
        &self.descriptor
    }
}

/// One semantic type plus its runtime behavior.
#[derive(Clone)]
pub struct TypeContribution {
    pub(crate) source_name: String,
    pub(crate) descriptor: TypeDescriptor,
    pub(crate) runtime: Arc<dyn TypeRuntime>,
}

impl TypeContribution {
    /// Create a type contribution from a source name, type descriptor, and runtime.
    pub fn new(
        source_name: impl Into<String>,
        descriptor: TypeDescriptor,
        runtime: impl TypeRuntime + 'static,
    ) -> Self {
        Self {
            source_name: source_name.into(),
            descriptor,
            runtime: Arc::new(runtime),
        }
    }

    /// Source-level type name as it appears in Tabula programs.
    pub fn source_name(&self) -> &str {
        &self.source_name
    }

    /// The low-level type descriptor (ID, field-element width, etc.).
    pub fn descriptor(&self) -> &TypeDescriptor {
        &self.descriptor
    }

    /// A shared reference to the type's runtime codec.
    pub fn runtime(&self) -> Arc<dyn TypeRuntime> {
        Arc::clone(&self.runtime)
    }
}

/// One semantic encoding plus its runtime behavior.
#[derive(Clone)]
pub struct EncodingContribution {
    pub(crate) profile: EncodingProfile,
    pub(crate) runtime: Arc<dyn EncodingRuntime>,
    pub(crate) default_for_type: Option<TypeId>,
}

impl EncodingContribution {
    /// Create an encoding contribution from an encoding profile and runtime.
    pub fn new(profile: EncodingProfile, runtime: impl EncodingRuntime + 'static) -> Self {
        Self {
            profile,
            runtime: Arc::new(runtime),
            default_for_type: None,
        }
    }

    /// Mark this encoding as the default for a given type ID.
    pub fn with_default_for_type(mut self, type_id: TypeId) -> Self {
        self.default_for_type = Some(type_id);
        self
    }

    /// The encoding profile (ID, supported type, field-element layout).
    pub fn profile(&self) -> &EncodingProfile {
        &self.profile
    }

    /// The type ID this encoding is the default for, if any.
    pub fn default_for_type(&self) -> Option<TypeId> {
        self.default_for_type
    }

    /// A shared reference to the encoding's runtime codec.
    pub fn runtime(&self) -> Arc<dyn EncodingRuntime> {
        Arc::clone(&self.runtime)
    }
}

/// One scheme family contribution plus its default selections and backend materializer.
#[derive(Clone)]
pub struct SchemeContribution {
    pub(crate) source_name: String,
    pub(crate) profile: SchemeProfile,
    pub(crate) default_encodings: Vec<EncodingProfileId>,
    #[cfg(feature = "verify")]
    pub(crate) backend_bundle: ColumnBackendFactoryBundle,
}

impl SchemeContribution {
    /// Create a scheme contribution from a source name, profile, and backend bundle.
    #[cfg(feature = "verify")]
    pub fn new(
        source_name: impl Into<String>,
        profile: SchemeProfile,
        backend_bundle: ColumnBackendFactoryBundle,
    ) -> Self {
        Self {
            source_name: source_name.into(),
            profile,
            default_encodings: Vec::new(),
            backend_bundle,
        }
    }

    /// Add an encoding profile as the default for this scheme.
    pub fn with_default_for_encoding(mut self, encoding_profile_id: EncodingProfileId) -> Self {
        self.default_encodings.push(encoding_profile_id);
        self
    }

    /// Source-level scheme name as it appears in Tabula programs.
    pub fn source_name(&self) -> &str {
        &self.source_name
    }

    /// The scheme profile (ID, supported encodings, etc.).
    pub fn profile(&self) -> &SchemeProfile {
        &self.profile
    }

    /// Encoding profile IDs that this scheme marks as defaults.
    pub fn default_encodings(&self) -> &[EncodingProfileId] {
        &self.default_encodings
    }

    /// The column backend factory bundle for this scheme.
    #[cfg(feature = "verify")]
    pub fn backend_bundle(&self) -> ColumnBackendFactoryBundle {
        self.backend_bundle.clone()
    }
}
