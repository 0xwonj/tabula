use std::collections::BTreeSet;
use std::sync::Arc;

use tabula_compiler::SourceCapabilityDescriptor;
use tabula_core::{EncodingProfileId, TypeId};
use tabula_profile::{EncodingProfile, SchemeProfile, TypeDescriptor};
use tabula_types::{EncodingRuntime, TypeRuntime};

#[cfg(feature = "verify")]
use crate::scheme::ColumnBackendFactoryBundle;
use crate::{ExtError, ExtResult};
#[cfg(feature = "prove")]
use crate::{RootBackend, root::RootBackendBundle};

/// One public capability contribution bundled into an extension install.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capability {
    pub(crate) descriptor: SourceCapabilityDescriptor,
}

impl Capability {
    pub fn new(descriptor: SourceCapabilityDescriptor) -> Self {
        Self { descriptor }
    }

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

    pub fn source_name(&self) -> &str {
        &self.source_name
    }

    pub fn descriptor(&self) -> &TypeDescriptor {
        &self.descriptor
    }

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
    pub fn new(profile: EncodingProfile, runtime: impl EncodingRuntime + 'static) -> Self {
        Self {
            profile,
            runtime: Arc::new(runtime),
            default_for_type: None,
        }
    }

    pub fn with_default_for_type(mut self, type_id: TypeId) -> Self {
        self.default_for_type = Some(type_id);
        self
    }

    pub fn profile(&self) -> &EncodingProfile {
        &self.profile
    }

    pub fn default_for_type(&self) -> Option<TypeId> {
        self.default_for_type
    }

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

    pub fn with_default_for_encoding(mut self, encoding_profile_id: EncodingProfileId) -> Self {
        self.default_encodings.push(encoding_profile_id);
        self
    }

    pub fn source_name(&self) -> &str {
        &self.source_name
    }

    pub fn profile(&self) -> &SchemeProfile {
        &self.profile
    }

    pub fn default_encodings(&self) -> &[EncodingProfileId] {
        &self.default_encodings
    }

    #[cfg(feature = "verify")]
    pub fn backend_bundle(&self) -> ColumnBackendFactoryBundle {
        self.backend_bundle.clone()
    }
}

/// Immutable atomic extension bundle.
#[derive(Clone)]
pub struct Extension {
    name: String,
    pub(crate) types: Vec<TypeContribution>,
    pub(crate) encodings: Vec<EncodingContribution>,
    pub(crate) schemes: Vec<SchemeContribution>,
    pub(crate) capabilities: Vec<Capability>,
    #[cfg(feature = "prove")]
    pub(crate) root_backend_bundle: Option<RootBackendBundle>,
}

impl Extension {
    pub fn builder(name: impl Into<String>) -> ExtensionBuilder {
        ExtensionBuilder::new(name)
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn types(&self) -> &[TypeContribution] {
        &self.types
    }

    pub fn encodings(&self) -> &[EncodingContribution] {
        &self.encodings
    }

    pub fn schemes(&self) -> &[SchemeContribution] {
        &self.schemes
    }

    pub fn capabilities(&self) -> &[Capability] {
        &self.capabilities
    }

    #[cfg(feature = "prove")]
    pub fn root_backend_bundle(&self) -> Option<RootBackendBundle> {
        self.root_backend_bundle.clone()
    }
}

impl std::fmt::Debug for Extension {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Extension")
            .field("name", &self.name)
            .field("types", &self.types.len())
            .field("encodings", &self.encodings.len())
            .field("schemes", &self.schemes.len())
            .field("capabilities", &self.capabilities.len())
            .finish_non_exhaustive()
    }
}

/// Fluent extension bundle builder.
pub struct ExtensionBuilder {
    name: String,
    types: Vec<TypeContribution>,
    encodings: Vec<EncodingContribution>,
    schemes: Vec<SchemeContribution>,
    capabilities: Vec<Capability>,
    #[cfg(feature = "prove")]
    root_backend_bundle: Option<RootBackendBundle>,
}

impl ExtensionBuilder {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            types: Vec::new(),
            encodings: Vec::new(),
            schemes: Vec::new(),
            capabilities: Vec::new(),
            #[cfg(feature = "prove")]
            root_backend_bundle: None,
        }
    }

    pub fn add_type(mut self, contribution: TypeContribution) -> Self {
        self.types.push(contribution);
        self
    }

    pub fn add_encoding(mut self, contribution: EncodingContribution) -> Self {
        self.encodings.push(contribution);
        self
    }

    pub fn add_scheme(mut self, contribution: SchemeContribution) -> Self {
        self.schemes.push(contribution);
        self
    }

    pub fn add_capability(mut self, contribution: Capability) -> Self {
        self.capabilities.push(contribution);
        self
    }

    #[cfg(feature = "prove")]
    pub fn with_root_backend(mut self, backend: impl RootBackend + 'static) -> Self {
        self.root_backend_bundle = Some(RootBackendBundle::new(backend));
        self
    }

    #[cfg(feature = "prove")]
    pub fn with_root_backend_bundle(mut self, bundle: RootBackendBundle) -> Self {
        self.root_backend_bundle = Some(bundle);
        self
    }

    pub fn build(self) -> ExtResult<Extension> {
        if self.name.trim().is_empty() {
            return Err(ExtError::validation("extension name must not be empty"));
        }

        let mut type_names = BTreeSet::new();
        let mut type_ids = BTreeSet::new();
        for contribution in &self.types {
            if !type_names.insert(contribution.source_name.clone()) {
                return Err(ExtError::validation(format!(
                    "duplicate type source name `{}` in extension `{}`",
                    contribution.source_name, self.name
                )));
            }
            if !type_ids.insert(contribution.descriptor.type_id) {
                return Err(ExtError::validation(format!(
                    "duplicate type id {} in extension `{}`",
                    contribution.descriptor.type_id.0, self.name
                )));
            }
        }

        let mut encoding_ids = BTreeSet::new();
        for contribution in &self.encodings {
            if !encoding_ids.insert(contribution.profile.encoding_profile_id) {
                return Err(ExtError::validation(format!(
                    "duplicate encoding profile id {} in extension `{}`",
                    contribution.profile.encoding_profile_id.0, self.name
                )));
            }
            if let Some(type_id) = contribution.default_for_type
                && type_id != contribution.profile.type_id
            {
                return Err(ExtError::validation(format!(
                    "encoding profile {} cannot be default for type {} because it encodes type {}",
                    contribution.profile.encoding_profile_id.0,
                    type_id.0,
                    contribution.profile.type_id.0,
                )));
            }
        }

        let mut scheme_names = BTreeSet::new();
        let mut scheme_profile_ids = BTreeSet::new();
        for contribution in &self.schemes {
            if !scheme_names.insert(contribution.source_name.clone()) {
                return Err(ExtError::validation(format!(
                    "duplicate scheme source name `{}` in extension `{}`",
                    contribution.source_name, self.name
                )));
            }
            if !scheme_profile_ids.insert(contribution.profile.scheme_profile_id) {
                return Err(ExtError::validation(format!(
                    "duplicate scheme profile id {} in extension `{}`",
                    contribution.profile.scheme_profile_id.0, self.name
                )));
            }
        }

        let mut capability_paths = BTreeSet::new();
        for contribution in &self.capabilities {
            if !capability_paths.insert(contribution.descriptor.path.clone()) {
                return Err(ExtError::validation(format!(
                    "duplicate capability path `{}` in extension `{}`",
                    contribution.descriptor.path, self.name
                )));
            }
        }

        Ok(Extension {
            name: self.name,
            types: self.types,
            encodings: self.encodings,
            schemes: self.schemes,
            capabilities: self.capabilities,
            #[cfg(feature = "prove")]
            root_backend_bundle: self.root_backend_bundle,
        })
    }
}

impl std::fmt::Debug for ExtensionBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExtensionBuilder")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}
