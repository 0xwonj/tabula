use std::collections::BTreeMap;

use tabula_core::{EncodingProfileId, SchemeId, SchemeProfileId, TypeId};

use crate::{ProfileCatalog, ProfileError};

/// Authoring-time registry that maps names and default selections onto the
/// reusable descriptors stored in a canonical [`ProfileCatalog`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SemanticRegistry {
    catalog: ProfileCatalog,
    type_names: BTreeMap<String, TypeId>,
    scheme_names: BTreeMap<String, SchemeId>,
    default_encoding_by_type: BTreeMap<TypeId, EncodingProfileId>,
    default_scheme_profile_by_key: BTreeMap<(SchemeId, EncodingProfileId), SchemeProfileId>,
}

impl SemanticRegistry {
    /// Create an empty semantic registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Borrow the reusable descriptor catalog owned by this registry.
    pub fn catalog(&self) -> &ProfileCatalog {
        &self.catalog
    }

    /// Consume the registry and return its descriptor catalog.
    pub fn into_catalog(self) -> ProfileCatalog {
        self.catalog
    }

    /// Register one reusable type descriptor and its source-level name.
    pub fn register_type_name(
        &mut self,
        name: impl Into<String>,
        type_id: TypeId,
    ) -> Result<(), ProfileError> {
        insert_unique_name(&mut self.type_names, name.into(), type_id, "type")
    }

    /// Register one source-level scheme family name.
    pub fn register_scheme_name(
        &mut self,
        name: impl Into<String>,
        scheme_id: SchemeId,
    ) -> Result<(), ProfileError> {
        insert_unique_name(&mut self.scheme_names, name.into(), scheme_id, "scheme")
    }

    /// Register the default encoding to use for one type.
    pub fn register_default_encoding(
        &mut self,
        type_id: TypeId,
        encoding_profile_id: EncodingProfileId,
    ) -> Result<(), ProfileError> {
        insert_unique_default(
            &mut self.default_encoding_by_type,
            type_id,
            encoding_profile_id,
            "default encoding",
        )
    }

    /// Register the default scheme profile to use for one `(scheme family,
    /// encoding)` pair.
    pub fn register_default_scheme_profile(
        &mut self,
        scheme_id: SchemeId,
        encoding_profile_id: EncodingProfileId,
        scheme_profile_id: SchemeProfileId,
    ) -> Result<(), ProfileError> {
        insert_unique_default(
            &mut self.default_scheme_profile_by_key,
            (scheme_id, encoding_profile_id),
            scheme_profile_id,
            "default scheme profile",
        )
    }

    /// Insert one reusable type descriptor into the registry catalog.
    pub fn register_type_descriptor(
        &mut self,
        descriptor: crate::TypeDescriptor,
    ) -> Result<(), ProfileError> {
        self.catalog.register_type(descriptor)
    }

    /// Insert one reusable encoding profile into the registry catalog.
    pub fn register_encoding_profile(
        &mut self,
        profile: crate::EncodingProfile,
    ) -> Result<(), ProfileError> {
        self.catalog.register_encoding(profile)
    }

    /// Insert one reusable scheme profile into the registry catalog.
    pub fn register_scheme_profile(
        &mut self,
        profile: crate::SchemeProfile,
    ) -> Result<(), ProfileError> {
        self.catalog.register_scheme(profile)
    }

    /// Resolve a source-level type name into a registered [`TypeId`].
    pub fn resolve_type_name(&self, name: &str) -> Result<TypeId, ProfileError> {
        self.type_names
            .get(name)
            .copied()
            .ok_or_else(|| ProfileError::UnknownNamedSemantic {
                kind: "type",
                name: name.to_string(),
            })
    }

    /// Resolve a source-level scheme family name into a registered [`SchemeId`].
    pub fn resolve_scheme_name(&self, name: &str) -> Result<SchemeId, ProfileError> {
        self.scheme_names
            .get(name)
            .copied()
            .ok_or_else(|| ProfileError::UnknownNamedSemantic {
                kind: "scheme",
                name: name.to_string(),
            })
    }

    /// Resolve the default encoding for one type.
    pub fn resolve_default_encoding(
        &self,
        type_id: TypeId,
    ) -> Result<EncodingProfileId, ProfileError> {
        self.default_encoding_by_type
            .get(&type_id)
            .copied()
            .ok_or(ProfileError::MissingDefaultEncoding(type_id))
    }

    /// Snapshot the canonical default-encoding selection map in deterministic
    /// type-id order.
    #[must_use]
    pub fn default_encoding_entries(&self) -> Vec<(TypeId, EncodingProfileId)> {
        self.default_encoding_by_type
            .iter()
            .map(|(type_id, encoding_profile_id)| (*type_id, *encoding_profile_id))
            .collect()
    }

    /// Resolve the default scheme profile for one `(scheme family, encoding)`
    /// pair.
    pub fn resolve_default_scheme_profile(
        &self,
        scheme_id: SchemeId,
        encoding_profile_id: EncodingProfileId,
    ) -> Result<SchemeProfileId, ProfileError> {
        self.default_scheme_profile_by_key
            .get(&(scheme_id, encoding_profile_id))
            .copied()
            .ok_or(ProfileError::MissingDefaultSchemeProfile {
                scheme_id,
                encoding_profile_id,
            })
    }

    /// Validate the registry and all descriptors it owns.
    pub fn validate(&self) -> Result<(), ProfileError> {
        self.catalog.validate()?;
        for type_id in self.type_names.values() {
            if !self
                .catalog
                .types
                .iter()
                .any(|descriptor| descriptor.type_id == *type_id)
            {
                return Err(ProfileError::MissingReference {
                    kind: "type",
                    raw: type_id.0,
                });
            }
        }
        for type_id in self.default_encoding_by_type.keys() {
            if !self
                .catalog
                .types
                .iter()
                .any(|descriptor| descriptor.type_id == *type_id)
            {
                return Err(ProfileError::MissingReference {
                    kind: "type",
                    raw: type_id.0,
                });
            }
        }
        for encoding_profile_id in self.default_encoding_by_type.values() {
            if !self
                .catalog
                .encodings
                .iter()
                .any(|profile| profile.encoding_profile_id == *encoding_profile_id)
            {
                return Err(ProfileError::MissingReference {
                    kind: "encoding",
                    raw: encoding_profile_id.0,
                });
            }
        }
        for scheme_profile_id in self.default_scheme_profile_by_key.values() {
            if !self
                .catalog
                .schemes
                .iter()
                .any(|profile| profile.scheme_profile_id == *scheme_profile_id)
            {
                return Err(ProfileError::MissingReference {
                    kind: "scheme",
                    raw: scheme_profile_id.0,
                });
            }
        }
        Ok(())
    }
}

fn insert_unique_name<K: Ord + Copy>(
    map: &mut BTreeMap<String, K>,
    name: String,
    value: K,
    kind: &'static str,
) -> Result<(), ProfileError> {
    if map.insert(name.clone(), value).is_some() {
        return Err(ProfileError::DuplicateNamedSemantic { kind, name });
    }
    Ok(())
}

fn insert_unique_default<K: Ord + Copy, V: Copy>(
    map: &mut BTreeMap<K, V>,
    key: K,
    value: V,
    kind: &'static str,
) -> Result<(), ProfileError> {
    if map.insert(key, value).is_some() {
        return Err(ProfileError::DuplicateDefaultResolution { kind });
    }
    Ok(())
}
