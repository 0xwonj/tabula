use tabula_core::{
    ColumnLayoutKind, EncodingProfileId, PropertyQueryKind, RootProfileId, SchemeId,
    SchemeProfileId, TypeId,
};

use crate::error::ProfileError;
use crate::model::{
    CanonicalNullEncoding, CommitmentContractKind, EncodingClass, EncodingProfile,
    EncodingRequirements, FieldFamily, GenericIrFamily, HostValueFamily, NullSemantics,
    ProfileCatalog, SchemeProfile, TranscriptSerialization, TypeCapabilities, TypeDescriptor,
    VerifierDigestFormat, WidthConstraint, ZeroValueSpec,
};
use crate::registry::SemanticRegistry;

/// Built-in catalog-scoped type id for `U64`.
pub const TYPE_U64_ID: TypeId = TypeId(0);
/// Built-in catalog-scoped type id for `I64`.
pub const TYPE_I64_ID: TypeId = TypeId(1);
/// Built-in catalog-scoped type id for `Bool`.
pub const TYPE_BOOL_ID: TypeId = TypeId(2);
/// Built-in catalog-scoped type id for `Bytes32`.
pub const TYPE_BYTES32_ID: TypeId = TypeId(3);

/// Built-in catalog-scoped encoding id for `U64`.
pub const ENCODING_U64_ID: EncodingProfileId = EncodingProfileId(0);
/// Built-in catalog-scoped encoding id for `I64`.
pub const ENCODING_I64_ID: EncodingProfileId = EncodingProfileId(1);
/// Built-in catalog-scoped encoding id for `Bool`.
pub const ENCODING_BOOL_ID: EncodingProfileId = EncodingProfileId(2);
/// Built-in catalog-scoped encoding id for `Bytes32`.
pub const ENCODING_BYTES32_ID: EncodingProfileId = EncodingProfileId(3);

/// Built-in catalog-scoped scheme profile id for SSMC.
pub const SCHEME_PROFILE_SSMC_ID: SchemeProfileId = SchemeProfileId(0);
/// Built-in catalog-scoped scheme profile id for SMT.
pub const SCHEME_PROFILE_SMT_ID: SchemeProfileId = SchemeProfileId(1);

/// Construct the canonical built-in profile catalog.
pub fn builtin_catalog() -> Result<ProfileCatalog, ProfileError> {
    let u64_type = builtin_u64_type()?;
    let i64_type = builtin_i64_type()?;
    let bool_type = builtin_bool_type()?;
    let bytes32_type = builtin_bytes32_type()?;

    let u64_encoding = builtin_u64_encoding(&u64_type)?;
    let i64_encoding = builtin_i64_encoding(&i64_type)?;
    let bool_encoding = builtin_bool_encoding(&bool_type)?;
    let bytes32_encoding = builtin_bytes32_encoding(&bytes32_type)?;

    let ssmc_scheme = builtin_ssmc_scheme_profile()?;
    let smt_scheme = builtin_smt_scheme_profile()?;

    let mut catalog = ProfileCatalog::new();
    catalog.register_type(u64_type)?;
    catalog.register_type(i64_type)?;
    catalog.register_type(bool_type)?;
    catalog.register_type(bytes32_type)?;
    catalog.register_encoding(u64_encoding)?;
    catalog.register_encoding(i64_encoding)?;
    catalog.register_encoding(bool_encoding)?;
    catalog.register_encoding(bytes32_encoding)?;
    catalog.register_scheme(ssmc_scheme)?;
    catalog.register_scheme(smt_scheme)?;
    catalog.validate()?;
    Ok(catalog)
}

/// Construct the built-in authoring/semantic registry.
pub fn builtin_semantic_registry() -> Result<SemanticRegistry, ProfileError> {
    let catalog = builtin_catalog()?;
    let mut registry = SemanticRegistry::new();
    for descriptor in &catalog.types {
        registry.register_type_descriptor(descriptor.clone())?;
    }
    for profile in &catalog.encodings {
        registry.register_encoding_profile(profile.clone())?;
    }
    for profile in &catalog.schemes {
        registry.register_scheme_profile(profile.clone())?;
    }

    registry.register_type_name("u64", TYPE_U64_ID)?;
    registry.register_type_name("i64", TYPE_I64_ID)?;
    registry.register_type_name("bool", TYPE_BOOL_ID)?;
    registry.register_type_name("bytes32", TYPE_BYTES32_ID)?;

    registry.register_scheme_name("ssmc", SchemeId::SSMC)?;
    registry.register_scheme_name("smt", SchemeId::SMT)?;

    registry.register_default_encoding(TYPE_U64_ID, ENCODING_U64_ID)?;
    registry.register_default_encoding(TYPE_I64_ID, ENCODING_I64_ID)?;
    registry.register_default_encoding(TYPE_BOOL_ID, ENCODING_BOOL_ID)?;
    registry.register_default_encoding(TYPE_BYTES32_ID, ENCODING_BYTES32_ID)?;

    registry.register_default_scheme_profile(
        SchemeId::SSMC,
        ENCODING_U64_ID,
        SCHEME_PROFILE_SSMC_ID,
    )?;
    registry.register_default_scheme_profile(
        SchemeId::SSMC,
        ENCODING_I64_ID,
        SCHEME_PROFILE_SSMC_ID,
    )?;
    registry.register_default_scheme_profile(
        SchemeId::SSMC,
        ENCODING_BOOL_ID,
        SCHEME_PROFILE_SSMC_ID,
    )?;
    registry.register_default_scheme_profile(
        SchemeId::SMT,
        ENCODING_U64_ID,
        SCHEME_PROFILE_SMT_ID,
    )?;
    registry.register_default_scheme_profile(
        SchemeId::SMT,
        ENCODING_I64_ID,
        SCHEME_PROFILE_SMT_ID,
    )?;
    registry.register_default_scheme_profile(
        SchemeId::SMT,
        ENCODING_BOOL_ID,
        SCHEME_PROFILE_SMT_ID,
    )?;
    registry.register_default_scheme_profile(
        SchemeId::SMT,
        ENCODING_BYTES32_ID,
        SCHEME_PROFILE_SMT_ID,
    )?;
    registry.validate()?;
    Ok(registry)
}

fn builtin_u64_type() -> Result<TypeDescriptor, ProfileError> {
    TypeDescriptor::new(
        TYPE_U64_ID,
        "U64",
        Some("Built-in unsigned 64-bit integer type.".to_string()),
        HostValueFamily::UnsignedInt { bits: 64 },
        GenericIrFamily::UnsignedInteger,
        TypeCapabilities {
            equality: true,
            ordering: true,
            arithmetic: true,
        },
        ZeroValueSpec::IntegerZero,
        NullSemantics::NullableWithCanonicalZero,
    )
}

fn builtin_i64_type() -> Result<TypeDescriptor, ProfileError> {
    TypeDescriptor::new(
        TYPE_I64_ID,
        "I64",
        Some("Built-in signed 64-bit integer type.".to_string()),
        HostValueFamily::SignedInt { bits: 64 },
        GenericIrFamily::SignedInteger,
        TypeCapabilities {
            equality: true,
            ordering: true,
            arithmetic: true,
        },
        ZeroValueSpec::IntegerZero,
        NullSemantics::NullableWithCanonicalZero,
    )
}

fn builtin_bool_type() -> Result<TypeDescriptor, ProfileError> {
    TypeDescriptor::new(
        TYPE_BOOL_ID,
        "Bool",
        Some("Built-in boolean type.".to_string()),
        HostValueFamily::Bool,
        GenericIrFamily::Boolean,
        TypeCapabilities {
            equality: true,
            ordering: true,
            arithmetic: false,
        },
        ZeroValueSpec::BoolFalse,
        NullSemantics::NullableWithCanonicalZero,
    )
}

fn builtin_bytes32_type() -> Result<TypeDescriptor, ProfileError> {
    TypeDescriptor::new(
        TYPE_BYTES32_ID,
        "Bytes32",
        Some("Built-in 32-byte opaque value.".to_string()),
        HostValueFamily::Bytes { len: 32 },
        GenericIrFamily::EqOnly,
        TypeCapabilities {
            equality: true,
            ordering: false,
            arithmetic: false,
        },
        ZeroValueSpec::ZeroBytes { len: 32 },
        NullSemantics::NullableWithCanonicalZero,
    )
}

fn builtin_u64_encoding(compatible_type: &TypeDescriptor) -> Result<EncodingProfile, ProfileError> {
    EncodingProfile::new(
        ENCODING_U64_ID,
        "koalabear_u64_limbs",
        Some("Built-in U64 KoalaBear limb encoding.".to_string()),
        compatible_type,
        EncodingClass::FieldElementArray,
        FieldFamily::KoalaBear31,
        3,
        CanonicalNullEncoding::SeparateNullFlagWithZeroValue,
        TranscriptSerialization::FieldElementsWithNullFlag,
        true,
    )
}

fn builtin_i64_encoding(compatible_type: &TypeDescriptor) -> Result<EncodingProfile, ProfileError> {
    EncodingProfile::new(
        ENCODING_I64_ID,
        "koalabear_i64_offset_limbs",
        Some("Built-in I64 offset KoalaBear limb encoding.".to_string()),
        compatible_type,
        EncodingClass::FieldElementArray,
        FieldFamily::KoalaBear31,
        3,
        CanonicalNullEncoding::SeparateNullFlagWithZeroValue,
        TranscriptSerialization::FieldElementsWithNullFlag,
        true,
    )
}

fn builtin_bool_encoding(
    compatible_type: &TypeDescriptor,
) -> Result<EncodingProfile, ProfileError> {
    EncodingProfile::new(
        ENCODING_BOOL_ID,
        "koalabear_bool",
        Some("Built-in Bool KoalaBear encoding.".to_string()),
        compatible_type,
        EncodingClass::FieldElementArray,
        FieldFamily::KoalaBear31,
        1,
        CanonicalNullEncoding::SeparateNullFlagWithZeroValue,
        TranscriptSerialization::FieldElementsWithNullFlag,
        true,
    )
}

fn builtin_bytes32_encoding(
    compatible_type: &TypeDescriptor,
) -> Result<EncodingProfile, ProfileError> {
    EncodingProfile::new(
        ENCODING_BYTES32_ID,
        "koalabear_bytes32_chunks",
        Some("Built-in Bytes32 KoalaBear chunk encoding.".to_string()),
        compatible_type,
        EncodingClass::FieldElementArray,
        FieldFamily::KoalaBear31,
        8,
        CanonicalNullEncoding::SeparateNullFlagWithZeroValue,
        TranscriptSerialization::FieldElementsWithNullFlag,
        false,
    )
}

/// Return the canonical built-in SSMC scheme profile.
pub fn builtin_ssmc_scheme_profile() -> Result<SchemeProfile, ProfileError> {
    SchemeProfile::new(
        SCHEME_PROFILE_SSMC_ID,
        "builtin_ssmc_v1",
        Some("Built-in sorted-state Merkle chain scheme profile.".to_string()),
        SchemeId::SSMC,
        CommitmentContractKind::SortedStateMerkleChain,
        VerifierDigestFormat::FieldElementArray { width: 8 },
        vec![PropertyQueryKind::Successor, PropertyQueryKind::Predecessor],
        EncodingRequirements {
            field_family: FieldFamily::KoalaBear31,
            encoding_class: EncodingClass::FieldElementArray,
            width_constraint: WidthConstraint::InclusiveRange { min: 1, max: 5 },
            canonical_null_encoding: CanonicalNullEncoding::SeparateNullFlagWithZeroValue,
            transcript_serialization: TranscriptSerialization::FieldElementsWithNullFlag,
            ordering_preserving: Some(true),
        },
        ColumnLayoutKind::SSMC_V1,
        RootProfileId::SMT_V1,
    )
}

/// Return the canonical built-in SMT scheme profile.
pub fn builtin_smt_scheme_profile() -> Result<SchemeProfile, ProfileError> {
    SchemeProfile::new(
        SCHEME_PROFILE_SMT_ID,
        "builtin_smt_v1",
        Some("Built-in sparse Merkle tree scheme profile.".to_string()),
        SchemeId::SMT,
        CommitmentContractKind::SparseMerkleTree,
        VerifierDigestFormat::FieldElementArray { width: 8 },
        vec![],
        EncodingRequirements {
            field_family: FieldFamily::KoalaBear31,
            encoding_class: EncodingClass::FieldElementArray,
            width_constraint: WidthConstraint::Any,
            canonical_null_encoding: CanonicalNullEncoding::SeparateNullFlagWithZeroValue,
            transcript_serialization: TranscriptSerialization::FieldElementsWithNullFlag,
            ordering_preserving: None,
        },
        ColumnLayoutKind::SMT_V1,
        RootProfileId::SMT_V1,
    )
}
