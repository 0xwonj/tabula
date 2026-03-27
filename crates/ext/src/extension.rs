//! Extension bundle type and its fluent builder.

use std::collections::BTreeSet;

use crate::contribution::{Capability, EncodingContribution, SchemeContribution, TypeContribution};
use crate::{ExtError, ExtResult};
#[cfg(feature = "prove")]
use crate::{RootBackend, root::RootBackendBundle};

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
    /// Create a new [`ExtensionBuilder`] for the given extension name.
    pub fn builder(name: impl Into<String>) -> ExtensionBuilder {
        ExtensionBuilder::new(name)
    }

    /// The name of this extension.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The type contributions registered by this extension.
    pub fn types(&self) -> &[TypeContribution] {
        &self.types
    }

    /// The encoding contributions registered by this extension.
    pub fn encodings(&self) -> &[EncodingContribution] {
        &self.encodings
    }

    /// The scheme contributions registered by this extension.
    pub fn schemes(&self) -> &[SchemeContribution] {
        &self.schemes
    }

    /// The capability contributions registered by this extension.
    pub fn capabilities(&self) -> &[Capability] {
        &self.capabilities
    }

    /// The root proof backend bundle provided by this extension, if any.
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
    /// Create a new builder with the given extension name.
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

    /// Add a type contribution to the extension.
    pub fn add_type(mut self, contribution: TypeContribution) -> Self {
        self.types.push(contribution);
        self
    }

    /// Add an encoding contribution to the extension.
    pub fn add_encoding(mut self, contribution: EncodingContribution) -> Self {
        self.encodings.push(contribution);
        self
    }

    /// Add a scheme contribution to the extension.
    pub fn add_scheme(mut self, contribution: SchemeContribution) -> Self {
        self.schemes.push(contribution);
        self
    }

    /// Add a capability contribution to the extension.
    pub fn add_capability(mut self, contribution: Capability) -> Self {
        self.capabilities.push(contribution);
        self
    }

    /// Set the root proof backend for this extension.
    #[cfg(feature = "prove")]
    pub fn with_root_backend(mut self, backend: impl RootBackend + 'static) -> Self {
        self.root_backend_bundle = Some(RootBackendBundle::new(backend));
        self
    }

    /// Set the root proof backend bundle directly for this extension.
    #[cfg(feature = "prove")]
    pub fn with_root_backend_bundle(mut self, bundle: RootBackendBundle) -> Self {
        self.root_backend_bundle = Some(bundle);
        self
    }

    /// Validate and build the [`Extension`].
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
