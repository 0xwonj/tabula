use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use tabula_core::{
    ColumnLayoutKind, ColumnProfileId, Digest, EncodingProfileId, PropertyQueryKind, RootProfileId,
    SchemeId, SchemeProfileId, TypeId,
};

use crate::ProfileError;
use crate::canonical::canonical_json_digest;

const TYPE_DESCRIPTOR_HASH_LABEL: &str = "type_descriptor";
const ENCODING_PROFILE_HASH_LABEL: &str = "encoding_profile";
const SCHEME_PROFILE_HASH_LABEL: &str = "scheme_profile";
const COLUMN_PROFILE_HASH_LABEL: &str = "column_profile";

/// Host-side representation family for one semantic type definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum HostValueFamily {
    /// Unsigned integer represented in host memory at a fixed bit width.
    UnsignedInt {
        /// Integer bit width.
        bits: u16,
    },
    /// Signed integer represented in host memory at a fixed bit width.
    SignedInt {
        /// Integer bit width.
        bits: u16,
    },
    /// Boolean host value.
    Bool,
    /// Fixed-length byte string.
    Bytes {
        /// Byte length.
        len: u16,
    },
    /// Escape hatch for future non-built-in host families.
    Opaque {
        /// Opaque family label.
        family: String,
    },
}

/// Closed generic-IR semantics family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum GenericIrFamily {
    /// Equality-only values.
    EqOnly,
    /// Ordered but non-arithmetic values.
    Ordered,
    /// Unsigned integers.
    UnsignedInteger,
    /// Signed integers.
    SignedInteger,
    /// Boolean values.
    Boolean,
}

/// Capability bits exposed from one semantic type to generic IR.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TypeCapabilities {
    /// Whether equality/inequality are supported.
    pub equality: bool,
    /// Whether ordering comparisons are supported.
    pub ordering: bool,
    /// Whether arithmetic operators are supported.
    pub arithmetic: bool,
}

/// Canonical zero-value contract for one type family.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum ZeroValueSpec {
    /// Integer zero.
    IntegerZero,
    /// Boolean false.
    BoolFalse,
    /// All-zero bytes of a fixed length.
    ZeroBytes {
        /// Byte length.
        len: u16,
    },
    /// Escape hatch for future non-built-in zero-value rules.
    Opaque {
        /// Opaque zero-value label.
        label: String,
    },
}

/// Nullability semantics exposed by one type family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum NullSemantics {
    /// Null values are represented by a separate flag plus canonical zero value.
    NullableWithCanonicalZero,
    /// The type does not support null.
    NonNullable,
}

/// Canonical semantic definition of one registered type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TypeDescriptor {
    /// Catalog-scoped lookup identifier.
    pub type_id: TypeId,
    /// Human-readable display name. Not part of semantic identity.
    pub display_name: String,
    /// Optional documentation string. Not part of semantic identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Domain-separated semantic identity hash.
    pub semantic_hash: Digest,
    /// Host-side representation family.
    pub host_value_family: HostValueFamily,
    /// Generic-IR semantics family.
    pub generic_ir_family: GenericIrFamily,
    /// Capability bits exposed to generic IR.
    pub capabilities: TypeCapabilities,
    /// Canonical zero-value contract.
    pub zero_value_spec: ZeroValueSpec,
    /// Nullability contract.
    pub null_semantics: NullSemantics,
}

impl TypeDescriptor {
    /// Build one canonical type descriptor and compute its semantic hash.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        type_id: TypeId,
        display_name: impl Into<String>,
        description: Option<String>,
        host_value_family: HostValueFamily,
        generic_ir_family: GenericIrFamily,
        capabilities: TypeCapabilities,
        zero_value_spec: ZeroValueSpec,
        null_semantics: NullSemantics,
    ) -> Result<Self, ProfileError> {
        let mut descriptor = Self {
            type_id,
            display_name: display_name.into(),
            description,
            semantic_hash: [0; 32],
            host_value_family,
            generic_ir_family,
            capabilities,
            zero_value_spec,
            null_semantics,
        };
        descriptor.semantic_hash = descriptor.compute_semantic_hash()?;
        Ok(descriptor)
    }

    /// Recompute the canonical semantic hash for this descriptor.
    pub fn compute_semantic_hash(&self) -> Result<Digest, ProfileError> {
        canonical_json_digest(
            TYPE_DESCRIPTOR_HASH_LABEL,
            &TypeDescriptorSemanticView {
                host_value_family: &self.host_value_family,
                generic_ir_family: self.generic_ir_family,
                capabilities: self.capabilities,
                zero_value_spec: &self.zero_value_spec,
                null_semantics: self.null_semantics,
            },
        )
    }
}

#[derive(Serialize)]
struct TypeDescriptorSemanticView<'a> {
    host_value_family: &'a HostValueFamily,
    generic_ir_family: GenericIrFamily,
    capabilities: TypeCapabilities,
    zero_value_spec: &'a ZeroValueSpec,
    null_semantics: NullSemantics,
}

/// Proof/transcript encoding class for one type family.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum EncodingClass {
    /// Fixed-width field-element array encoding.
    FieldElementArray,
    /// Escape hatch for future non-built-in encoding families.
    Opaque {
        /// Opaque family label.
        family: String,
    },
}

/// Field family used by one encoding profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum FieldFamily {
    /// KoalaBear field used by the current proof stack.
    KoalaBear31,
    /// Escape hatch for future field families.
    Opaque {
        /// Opaque family label.
        family: String,
    },
}

/// Canonical null encoding contract for one encoding profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum CanonicalNullEncoding {
    /// A separate null flag plus canonical zero value limbs.
    SeparateNullFlagWithZeroValue,
    /// The encoding is not nullable.
    NotApplicable,
}

/// Transcript-facing serialization rule for one encoding profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum TranscriptSerialization {
    /// Field-element limbs followed by a null flag.
    FieldElementsWithNullFlag,
    /// Raw field-element limbs only.
    RawFieldElements,
    /// Escape hatch for future transcript contracts.
    Opaque {
        /// Opaque family label.
        family: String,
    },
}

/// Canonical proof/transcript representation contract for one type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EncodingProfile {
    /// Catalog-scoped lookup identifier.
    pub encoding_profile_id: EncodingProfileId,
    /// Human-readable display name. Not part of semantic identity.
    pub display_name: String,
    /// Optional documentation string. Not part of semantic identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Domain-separated semantic identity hash.
    pub semantic_hash: Digest,
    /// Compatible semantic type descriptor.
    pub type_id: TypeId,
    /// Proof/transcript encoding class.
    pub encoding_class: EncodingClass,
    /// Proof field family.
    pub field_family: FieldFamily,
    /// Fixed field-element width.
    pub width: u16,
    /// Canonical null encoding rule.
    pub canonical_null_encoding: CanonicalNullEncoding,
    /// Transcript serialization rule.
    pub transcript_serialization: TranscriptSerialization,
    /// Whether this encoding preserves ordering semantics.
    pub ordering_preserving: bool,
}

impl EncodingProfile {
    /// Build one canonical encoding profile and compute its semantic hash.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        encoding_profile_id: EncodingProfileId,
        display_name: impl Into<String>,
        description: Option<String>,
        compatible_type: &TypeDescriptor,
        encoding_class: EncodingClass,
        field_family: FieldFamily,
        width: u16,
        canonical_null_encoding: CanonicalNullEncoding,
        transcript_serialization: TranscriptSerialization,
        ordering_preserving: bool,
    ) -> Result<Self, ProfileError> {
        let mut profile = Self {
            encoding_profile_id,
            display_name: display_name.into(),
            description,
            semantic_hash: [0; 32],
            type_id: compatible_type.type_id,
            encoding_class,
            field_family,
            width,
            canonical_null_encoding,
            transcript_serialization,
            ordering_preserving,
        };
        profile.semantic_hash = profile.compute_semantic_hash(compatible_type)?;
        Ok(profile)
    }

    /// Recompute the canonical semantic hash for this profile.
    pub fn compute_semantic_hash(
        &self,
        compatible_type: &TypeDescriptor,
    ) -> Result<Digest, ProfileError> {
        canonical_json_digest(
            ENCODING_PROFILE_HASH_LABEL,
            &EncodingProfileSemanticView {
                compatible_type_semantic_hash: compatible_type.semantic_hash,
                encoding_class: &self.encoding_class,
                field_family: &self.field_family,
                width: self.width,
                canonical_null_encoding: &self.canonical_null_encoding,
                transcript_serialization: &self.transcript_serialization,
                ordering_preserving: self.ordering_preserving,
            },
        )
    }
}

#[derive(Serialize)]
struct EncodingProfileSemanticView<'a> {
    compatible_type_semantic_hash: Digest,
    encoding_class: &'a EncodingClass,
    field_family: &'a FieldFamily,
    width: u16,
    canonical_null_encoding: &'a CanonicalNullEncoding,
    transcript_serialization: &'a TranscriptSerialization,
    ordering_preserving: bool,
}

/// Width acceptance rule for a scheme profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum WidthConstraint {
    /// Accept any positive width.
    Any,
    /// Accept one exact width.
    Exact(u16),
    /// Accept any width at least `min`.
    AtLeast(u16),
    /// Accept any width in the inclusive range.
    InclusiveRange {
        /// Inclusive minimum width.
        min: u16,
        /// Inclusive maximum width.
        max: u16,
    },
}

impl WidthConstraint {
    fn contains(self, width: u16) -> bool {
        match self {
            Self::Any => width > 0,
            Self::Exact(exact) => width == exact,
            Self::AtLeast(min) => width >= min,
            Self::InclusiveRange { min, max } => (min..=max).contains(&width),
        }
    }
}

/// Accepted encoding contract for one scheme profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EncodingRequirements {
    /// Required field family.
    pub field_family: FieldFamily,
    /// Required encoding class.
    pub encoding_class: EncodingClass,
    /// Accepted width rule.
    pub width_constraint: WidthConstraint,
    /// Required canonical null encoding.
    pub canonical_null_encoding: CanonicalNullEncoding,
    /// Required transcript serialization rule.
    pub transcript_serialization: TranscriptSerialization,
    /// Required ordering-preserving bit, if constrained.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ordering_preserving: Option<bool>,
}

impl EncodingRequirements {
    fn validate_encoding(&self, profile: &EncodingProfile) -> Result<(), String> {
        if self.field_family != profile.field_family {
            return Err(format!(
                "field family mismatch: expected {:?}, got {:?}",
                self.field_family, profile.field_family
            ));
        }
        if self.encoding_class != profile.encoding_class {
            return Err(format!(
                "encoding class mismatch: expected {:?}, got {:?}",
                self.encoding_class, profile.encoding_class
            ));
        }
        if !self.width_constraint.contains(profile.width) {
            return Err(format!(
                "width {} is outside accepted constraint {:?}",
                profile.width, self.width_constraint
            ));
        }
        if self.canonical_null_encoding != profile.canonical_null_encoding {
            return Err(format!(
                "null encoding mismatch: expected {:?}, got {:?}",
                self.canonical_null_encoding, profile.canonical_null_encoding
            ));
        }
        if self.transcript_serialization != profile.transcript_serialization {
            return Err(format!(
                "transcript serialization mismatch: expected {:?}, got {:?}",
                self.transcript_serialization, profile.transcript_serialization
            ));
        }
        if let Some(ordering_preserving) = self.ordering_preserving
            && ordering_preserving != profile.ordering_preserving
        {
            return Err(format!(
                "ordering-preserving mismatch: expected {}, got {}",
                ordering_preserving, profile.ordering_preserving
            ));
        }
        Ok(())
    }
}

/// Verifier-visible commitment/opening contract family.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum CommitmentContractKind {
    /// Sorted-state Merkle chain.
    SortedStateMerkleChain,
    /// Sparse Merkle tree.
    SparseMerkleTree,
    /// Escape hatch for future scheme families.
    Opaque {
        /// Opaque family label.
        family: String,
    },
}

/// Canonical verifier-visible digest normalization format.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum VerifierDigestFormat {
    /// Field-element digest with a fixed width.
    FieldElementArray {
        /// Digest width in field elements.
        width: u16,
    },
    /// Raw 32-byte digest.
    Bytes32,
    /// Escape hatch for future digest formats.
    Opaque {
        /// Opaque family label.
        family: String,
    },
}

/// Canonical commitment/opening contract for one scheme profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SchemeProfile {
    /// Catalog-scoped lookup identifier.
    pub scheme_profile_id: SchemeProfileId,
    /// Human-readable display name. Not part of semantic identity.
    pub display_name: String,
    /// Optional documentation string. Not part of semantic identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Domain-separated semantic identity hash.
    pub semantic_hash: Digest,
    /// Portable scheme family identifier.
    pub scheme_family_id: SchemeId,
    /// Verifier-visible commitment/opening contract kind.
    pub commitment_contract_kind: CommitmentContractKind,
    /// Verifier-visible digest normalization format.
    pub verifier_digest_format: VerifierDigestFormat,
    /// Structural property capabilities.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub property_query_capabilities: Vec<PropertyQueryKind>,
    /// Accepted encoding contract.
    pub encoding_requirements: EncodingRequirements,
    /// Transitional proof layout compatibility id.
    pub proof_layout_family: ColumnLayoutKind,
    /// Transitional root binding compatibility id.
    pub root_binding_family: RootProfileId,
}

impl SchemeProfile {
    /// Build one canonical scheme profile and compute its semantic hash.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        scheme_profile_id: SchemeProfileId,
        display_name: impl Into<String>,
        description: Option<String>,
        scheme_family_id: SchemeId,
        commitment_contract_kind: CommitmentContractKind,
        verifier_digest_format: VerifierDigestFormat,
        mut property_query_capabilities: Vec<PropertyQueryKind>,
        encoding_requirements: EncodingRequirements,
        proof_layout_family: ColumnLayoutKind,
        root_binding_family: RootProfileId,
    ) -> Result<Self, ProfileError> {
        property_query_capabilities.sort();
        property_query_capabilities.dedup();
        let mut profile = Self {
            scheme_profile_id,
            display_name: display_name.into(),
            description,
            semantic_hash: [0; 32],
            scheme_family_id,
            commitment_contract_kind,
            verifier_digest_format,
            property_query_capabilities,
            encoding_requirements,
            proof_layout_family,
            root_binding_family,
        };
        profile.semantic_hash = profile.compute_semantic_hash()?;
        Ok(profile)
    }

    /// Recompute the canonical semantic hash for this profile.
    pub fn compute_semantic_hash(&self) -> Result<Digest, ProfileError> {
        canonical_json_digest(
            SCHEME_PROFILE_HASH_LABEL,
            &SchemeProfileSemanticView {
                scheme_family_id: self.scheme_family_id,
                commitment_contract_kind: &self.commitment_contract_kind,
                verifier_digest_format: &self.verifier_digest_format,
                property_query_capabilities: &self.property_query_capabilities,
                encoding_requirements: &self.encoding_requirements,
                proof_layout_family: self.proof_layout_family,
                root_binding_family: self.root_binding_family,
            },
        )
    }
}

#[derive(Serialize)]
struct SchemeProfileSemanticView<'a> {
    scheme_family_id: SchemeId,
    commitment_contract_kind: &'a CommitmentContractKind,
    verifier_digest_format: &'a VerifierDigestFormat,
    property_query_capabilities: &'a [PropertyQueryKind],
    encoding_requirements: &'a EncodingRequirements,
    proof_layout_family: ColumnLayoutKind,
    root_binding_family: RootProfileId,
}

/// Whether a committed column contributes to the root commitment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum CommitmentRole {
    /// This column participates in the root commitment.
    IncludedInRoot,
    /// This column is tracked but omitted from the root commitment.
    Detached,
}

/// Per-column sealed composition of type, encoding, and scheme choices.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColumnProfile {
    /// Catalog-scoped lookup identifier.
    pub column_profile_id: ColumnProfileId,
    /// Human-readable display name. Not part of semantic identity.
    pub display_name: String,
    /// Optional documentation string. Not part of semantic identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Domain-separated semantic identity hash for the sealed composition.
    pub profile_hash: Digest,
    /// Referenced type descriptor.
    pub type_id: TypeId,
    /// Referenced encoding profile.
    pub encoding_profile_id: EncodingProfileId,
    /// Referenced scheme profile.
    pub scheme_profile_id: SchemeProfileId,
    /// Root-commitment participation role.
    pub commitment_role: CommitmentRole,
}

impl ColumnProfile {
    /// Build one canonical column profile and compute its sealed profile hash.
    pub fn new(
        column_profile_id: ColumnProfileId,
        display_name: impl Into<String>,
        description: Option<String>,
        type_descriptor: &TypeDescriptor,
        encoding_profile: &EncodingProfile,
        scheme_profile: &SchemeProfile,
        commitment_role: CommitmentRole,
    ) -> Result<Self, ProfileError> {
        let mut profile = Self {
            column_profile_id,
            display_name: display_name.into(),
            description,
            profile_hash: [0; 32],
            type_id: type_descriptor.type_id,
            encoding_profile_id: encoding_profile.encoding_profile_id,
            scheme_profile_id: scheme_profile.scheme_profile_id,
            commitment_role,
        };
        profile.profile_hash =
            profile.compute_profile_hash(type_descriptor, encoding_profile, scheme_profile)?;
        Ok(profile)
    }

    /// Recompute the canonical sealed profile hash for this column profile.
    pub fn compute_profile_hash(
        &self,
        type_descriptor: &TypeDescriptor,
        encoding_profile: &EncodingProfile,
        scheme_profile: &SchemeProfile,
    ) -> Result<Digest, ProfileError> {
        canonical_json_digest(
            COLUMN_PROFILE_HASH_LABEL,
            &ColumnProfileSemanticView {
                type_semantic_hash: type_descriptor.semantic_hash,
                encoding_semantic_hash: encoding_profile.semantic_hash,
                scheme_semantic_hash: scheme_profile.semantic_hash,
                commitment_role: self.commitment_role,
            },
        )
    }
}

#[derive(Serialize)]
struct ColumnProfileSemanticView {
    type_semantic_hash: Digest,
    encoding_semantic_hash: Digest,
    scheme_semantic_hash: Digest,
    commitment_role: CommitmentRole,
}

/// Non-serialized resolved view used by future compiler/runtime/prover adoption.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedColumnProfile {
    /// Resolved per-column sealed profile.
    pub column_profile: ColumnProfile,
    /// Resolved semantic type definition.
    pub type_descriptor: TypeDescriptor,
    /// Resolved encoding profile.
    pub encoding_profile: EncodingProfile,
    /// Resolved scheme profile.
    pub scheme_profile: SchemeProfile,
}

impl ResolvedColumnProfile {
    /// Whether the resolved column participates in the root commitment.
    pub fn receives_commitment(&self) -> bool {
        matches!(
            self.column_profile.commitment_role,
            CommitmentRole::IncludedInRoot
        )
    }

    /// Type capabilities exposed to generic IR.
    pub fn type_capabilities(&self) -> TypeCapabilities {
        self.type_descriptor.capabilities
    }

    /// Closed generic-IR family for this column's type.
    pub fn generic_ir_family(&self) -> GenericIrFamily {
        self.type_descriptor.generic_ir_family
    }

    /// Fixed field-element width of this column's proof/transcript encoding.
    pub fn encoding_width(&self) -> u16 {
        self.encoding_profile.width
    }

    /// Transcript serialization contract for this column.
    pub fn transcript_serialization(&self) -> TranscriptSerialization {
        self.encoding_profile.transcript_serialization.clone()
    }

    /// Verifier-visible digest normalization format for this column's scheme.
    pub fn verifier_digest_format(&self) -> VerifierDigestFormat {
        self.scheme_profile.verifier_digest_format.clone()
    }

    /// Transitional proof layout family for this column's scheme.
    pub fn proof_layout_family(&self) -> ColumnLayoutKind {
        self.scheme_profile.proof_layout_family
    }

    /// Transitional root binding family for this column's scheme.
    pub fn root_binding_family(&self) -> RootProfileId {
        self.scheme_profile.root_binding_family
    }

    /// Whether this scheme exposes one structural property query kind.
    pub fn supports_property_query(&self, query_kind: PropertyQueryKind) -> bool {
        self.scheme_profile
            .property_query_capabilities
            .contains(&query_kind)
    }
}

/// Borrowed resolved view used as the default query surface for profile lookups.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedColumnProfileRef<'a> {
    /// Resolved per-column sealed profile.
    pub column_profile: &'a ColumnProfile,
    /// Resolved semantic type definition.
    pub type_descriptor: &'a TypeDescriptor,
    /// Resolved encoding profile.
    pub encoding_profile: &'a EncodingProfile,
    /// Resolved scheme profile.
    pub scheme_profile: &'a SchemeProfile,
}

impl<'a> ResolvedColumnProfileRef<'a> {
    /// Clone into an owned resolved profile view.
    pub fn to_owned(self) -> ResolvedColumnProfile {
        ResolvedColumnProfile {
            column_profile: self.column_profile.clone(),
            type_descriptor: self.type_descriptor.clone(),
            encoding_profile: self.encoding_profile.clone(),
            scheme_profile: self.scheme_profile.clone(),
        }
    }

    /// Whether the resolved column participates in the root commitment.
    pub fn receives_commitment(&self) -> bool {
        matches!(
            self.column_profile.commitment_role,
            CommitmentRole::IncludedInRoot
        )
    }

    /// Type capabilities exposed to generic IR.
    pub fn type_capabilities(&self) -> TypeCapabilities {
        self.type_descriptor.capabilities
    }

    /// Closed generic-IR family for this column's type.
    pub fn generic_ir_family(&self) -> GenericIrFamily {
        self.type_descriptor.generic_ir_family
    }

    /// Fixed field-element width of this column's proof/transcript encoding.
    pub fn encoding_width(&self) -> u16 {
        self.encoding_profile.width
    }

    /// Transcript serialization contract for this column.
    pub fn transcript_serialization(&self) -> TranscriptSerialization {
        self.encoding_profile.transcript_serialization.clone()
    }

    /// Verifier-visible digest normalization format for this column's scheme.
    pub fn verifier_digest_format(&self) -> VerifierDigestFormat {
        self.scheme_profile.verifier_digest_format.clone()
    }

    /// Transitional proof layout family for this column's scheme.
    pub fn proof_layout_family(&self) -> ColumnLayoutKind {
        self.scheme_profile.proof_layout_family
    }

    /// Transitional root binding family for this column's scheme.
    pub fn root_binding_family(&self) -> RootProfileId {
        self.scheme_profile.root_binding_family
    }

    /// Whether this scheme exposes one structural property query kind.
    pub fn supports_property_query(&self, query_kind: PropertyQueryKind) -> bool {
        self.scheme_profile
            .property_query_capabilities
            .contains(&query_kind)
    }
}

/// Canonical storage and validation container for profile descriptors.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileCatalog {
    /// Registered type descriptors, sorted by `type_id`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub types: Vec<TypeDescriptor>,
    /// Registered encoding profiles, sorted by `encoding_profile_id`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub encodings: Vec<EncodingProfile>,
    /// Registered scheme profiles, sorted by `scheme_profile_id`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub schemes: Vec<SchemeProfile>,
    /// Registered per-column sealed profiles, sorted by `column_profile_id`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub columns: Vec<ColumnProfile>,
}

impl ProfileCatalog {
    /// Create an empty profile catalog.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register one type descriptor while preserving deterministic id order.
    pub fn register_type(&mut self, descriptor: TypeDescriptor) -> Result<(), ProfileError> {
        insert_sorted(
            &mut self.types,
            descriptor,
            |descriptor| descriptor.type_id.0,
            "type",
        )
    }

    /// Register one encoding profile while preserving deterministic id order.
    pub fn register_encoding(&mut self, profile: EncodingProfile) -> Result<(), ProfileError> {
        insert_sorted(
            &mut self.encodings,
            profile,
            |profile| profile.encoding_profile_id.0,
            "encoding",
        )
    }

    /// Register one scheme profile while preserving deterministic id order.
    pub fn register_scheme(&mut self, profile: SchemeProfile) -> Result<(), ProfileError> {
        insert_sorted(
            &mut self.schemes,
            profile,
            |profile| profile.scheme_profile_id.0,
            "scheme",
        )
    }

    /// Register one column profile while preserving deterministic id order.
    pub fn register_column(&mut self, profile: ColumnProfile) -> Result<(), ProfileError> {
        insert_sorted(
            &mut self.columns,
            profile,
            |profile| profile.column_profile_id.0,
            "column",
        )
    }

    /// Validate deterministic ordering, references, hashes, and compatibility.
    pub fn validate(&self) -> Result<(), ProfileError> {
        ensure_sorted_unique(&self.types, |descriptor| descriptor.type_id.0, "type")?;
        ensure_sorted_unique(
            &self.encodings,
            |profile| profile.encoding_profile_id.0,
            "encoding",
        )?;
        ensure_sorted_unique(
            &self.schemes,
            |profile| profile.scheme_profile_id.0,
            "scheme",
        )?;
        ensure_sorted_unique(
            &self.columns,
            |profile| profile.column_profile_id.0,
            "column",
        )?;

        let type_map: BTreeMap<_, _> = self
            .types
            .iter()
            .map(|descriptor| (descriptor.type_id, descriptor))
            .collect();
        let encoding_map: BTreeMap<_, _> = self
            .encodings
            .iter()
            .map(|profile| (profile.encoding_profile_id, profile))
            .collect();
        let scheme_map: BTreeMap<_, _> = self
            .schemes
            .iter()
            .map(|profile| (profile.scheme_profile_id, profile))
            .collect();

        for descriptor in &self.types {
            if descriptor.semantic_hash != descriptor.compute_semantic_hash()? {
                return Err(ProfileError::HashMismatch {
                    kind: "type",
                    raw: descriptor.type_id.0,
                });
            }
        }

        for profile in &self.encodings {
            let compatible_type =
                type_map
                    .get(&profile.type_id)
                    .copied()
                    .ok_or(ProfileError::MissingReference {
                        kind: "type",
                        raw: profile.type_id.0,
                    })?;
            if profile.semantic_hash != profile.compute_semantic_hash(compatible_type)? {
                return Err(ProfileError::HashMismatch {
                    kind: "encoding",
                    raw: profile.encoding_profile_id.0,
                });
            }
        }

        for profile in &self.schemes {
            if profile.semantic_hash != profile.compute_semantic_hash()? {
                return Err(ProfileError::HashMismatch {
                    kind: "scheme",
                    raw: profile.scheme_profile_id.0,
                });
            }
        }

        for profile in &self.columns {
            let type_descriptor =
                type_map
                    .get(&profile.type_id)
                    .copied()
                    .ok_or(ProfileError::MissingReference {
                        kind: "type",
                        raw: profile.type_id.0,
                    })?;
            let encoding_profile = encoding_map
                .get(&profile.encoding_profile_id)
                .copied()
                .ok_or(ProfileError::MissingReference {
                    kind: "encoding",
                    raw: profile.encoding_profile_id.0,
                })?;
            let scheme_profile = scheme_map.get(&profile.scheme_profile_id).copied().ok_or(
                ProfileError::MissingReference {
                    kind: "scheme",
                    raw: profile.scheme_profile_id.0,
                },
            )?;

            if encoding_profile.type_id != profile.type_id {
                return Err(ProfileError::EncodingTypeMismatch {
                    column_profile_id: profile.column_profile_id,
                    type_id: profile.type_id,
                    encoding_profile_id: profile.encoding_profile_id,
                });
            }
            scheme_profile
                .encoding_requirements
                .validate_encoding(encoding_profile)
                .map_err(|reason| ProfileError::SchemeEncodingIncompatibility {
                    column_profile_id: profile.column_profile_id,
                    encoding_profile_id: profile.encoding_profile_id,
                    scheme_profile_id: profile.scheme_profile_id,
                    reason,
                })?;
            if profile.profile_hash
                != profile.compute_profile_hash(
                    type_descriptor,
                    encoding_profile,
                    scheme_profile,
                )?
            {
                return Err(ProfileError::HashMismatch {
                    kind: "column",
                    raw: profile.column_profile_id.0,
                });
            }
        }

        Ok(())
    }

    /// Borrow one canonical type descriptor by id.
    pub fn type_descriptor(&self, type_id: TypeId) -> Result<&TypeDescriptor, ProfileError> {
        self.types
            .iter()
            .find(|descriptor| descriptor.type_id == type_id)
            .ok_or(ProfileError::MissingReference {
                kind: "type",
                raw: type_id.0,
            })
    }

    /// Borrow one canonical encoding profile by id.
    pub fn encoding_profile(
        &self,
        encoding_profile_id: EncodingProfileId,
    ) -> Result<&EncodingProfile, ProfileError> {
        self.encodings
            .iter()
            .find(|profile| profile.encoding_profile_id == encoding_profile_id)
            .ok_or(ProfileError::MissingReference {
                kind: "encoding",
                raw: encoding_profile_id.0,
            })
    }

    /// Borrow one canonical scheme profile by id.
    pub fn scheme_profile(
        &self,
        scheme_profile_id: SchemeProfileId,
    ) -> Result<&SchemeProfile, ProfileError> {
        self.schemes
            .iter()
            .find(|profile| profile.scheme_profile_id == scheme_profile_id)
            .ok_or(ProfileError::MissingReference {
                kind: "scheme",
                raw: scheme_profile_id.0,
            })
    }

    /// Borrow one canonical column profile by id.
    pub fn column_profile(
        &self,
        column_profile_id: ColumnProfileId,
    ) -> Result<&ColumnProfile, ProfileError> {
        self.columns
            .iter()
            .find(|profile| profile.column_profile_id == column_profile_id)
            .ok_or(ProfileError::UnknownColumnProfile(column_profile_id))
    }

    /// Resolve one column profile into its validated join view.
    pub fn resolve_column_profile(
        &self,
        column_profile_id: ColumnProfileId,
    ) -> Result<ResolvedColumnProfileRef<'_>, ProfileError> {
        self.validate()?;

        let column_profile = self.column_profile(column_profile_id)?;
        let type_descriptor = self.type_descriptor(column_profile.type_id)?;
        let encoding_profile = self.encoding_profile(column_profile.encoding_profile_id)?;
        let scheme_profile = self.scheme_profile(column_profile.scheme_profile_id)?;

        Ok(ResolvedColumnProfileRef {
            column_profile,
            type_descriptor,
            encoding_profile,
            scheme_profile,
        })
    }
}

fn insert_sorted<T, F>(
    entries: &mut Vec<T>,
    entry: T,
    id: F,
    kind: &'static str,
) -> Result<(), ProfileError>
where
    F: Fn(&T) -> u32,
{
    let raw = id(&entry);
    match entries.binary_search_by_key(&raw, &id) {
        Ok(_) => Err(ProfileError::DuplicateId { kind, raw }),
        Err(idx) => {
            entries.insert(idx, entry);
            Ok(())
        }
    }
}

fn ensure_sorted_unique<T, F>(entries: &[T], id: F, kind: &'static str) -> Result<(), ProfileError>
where
    F: Fn(&T) -> u32,
{
    for window in entries.windows(2) {
        let left = id(&window[0]);
        let right = id(&window[1]);
        if left == right {
            return Err(ProfileError::DuplicateId { kind, raw: left });
        }
        if left > right {
            return Err(ProfileError::UnsortedCatalog { kind });
        }
    }
    Ok(())
}
