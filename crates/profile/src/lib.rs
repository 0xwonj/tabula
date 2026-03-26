//! Canonical profile descriptors, catalogs, built-ins, and authoring registries.

mod builtins;
mod canonical;
mod error;
mod model;
mod registry;

pub use builtins::{
    ENCODING_BOOL_ID, ENCODING_BYTES32_ID, ENCODING_I64_ID, ENCODING_U64_ID, SCHEME_PROFILE_SMT_ID,
    SCHEME_PROFILE_SSMC_ID, TYPE_BOOL_ID, TYPE_BYTES32_ID, TYPE_I64_ID, TYPE_U64_ID,
    builtin_catalog, builtin_semantic_registry, builtin_smt_scheme_profile,
    builtin_ssmc_scheme_profile, is_bool_type, is_bytes32_type, is_i64_type, is_u64_type,
};
pub use error::ProfileError;
pub use model::{
    CanonicalNullEncoding, ColumnProfile, CommitmentContractKind, CommitmentRole, EncodingClass,
    EncodingProfile, EncodingRequirements, FieldFamily, GenericIrFamily, HostValueFamily,
    NullSemantics, ProfileCatalog, ResolvedColumnProfile, ResolvedColumnProfileRef, SchemeProfile,
    TranscriptSerialization, TypeCapabilities, TypeDescriptor, VerifierDigestFormat,
    WidthConstraint, ZeroValueSpec,
};
pub use registry::SemanticRegistry;

#[cfg(test)]
mod tests {
    use tabula_core::{ColumnProfileId, TypeId};

    use super::*;

    #[test]
    fn type_semantic_hash_ignores_id_and_display_metadata() {
        let left = TypeDescriptor::new(
            TypeId(10),
            "left",
            Some("doc left".to_string()),
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
        .unwrap();
        let right = TypeDescriptor::new(
            TypeId(999),
            "right",
            Some("doc right".to_string()),
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
        .unwrap();

        assert_eq!(left.semantic_hash, right.semantic_hash);
    }

    #[test]
    fn encoding_hash_changes_when_width_changes() {
        let ty = builtin_catalog()
            .unwrap()
            .type_descriptor(TYPE_U64_ID)
            .cloned()
            .expect("built-in u64 descriptor");
        let narrow = EncodingProfile::new(
            ENCODING_U64_ID,
            "enc",
            None,
            &ty,
            EncodingClass::FieldElementArray,
            FieldFamily::KoalaBear31,
            3,
            CanonicalNullEncoding::SeparateNullFlagWithZeroValue,
            TranscriptSerialization::FieldElementsWithNullFlag,
            true,
        )
        .unwrap();
        let wide = EncodingProfile::new(
            ENCODING_U64_ID,
            "enc",
            None,
            &ty,
            EncodingClass::FieldElementArray,
            FieldFamily::KoalaBear31,
            4,
            CanonicalNullEncoding::SeparateNullFlagWithZeroValue,
            TranscriptSerialization::FieldElementsWithNullFlag,
            true,
        )
        .unwrap();

        assert_ne!(narrow.semantic_hash, wide.semantic_hash);
    }

    #[test]
    fn scheme_hash_changes_when_digest_format_changes() {
        let left = SchemeProfile::new(
            SCHEME_PROFILE_SMT_ID,
            "left",
            None,
            tabula_core::SchemeId::SMT,
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
            tabula_core::ColumnLayoutKind::SMT_V1,
            tabula_core::RootProfileId::SMT_V1,
        )
        .unwrap();
        let right = SchemeProfile::new(
            SCHEME_PROFILE_SMT_ID,
            "right",
            None,
            tabula_core::SchemeId::SMT,
            CommitmentContractKind::SparseMerkleTree,
            VerifierDigestFormat::FieldElementArray { width: 16 },
            vec![],
            EncodingRequirements {
                field_family: FieldFamily::KoalaBear31,
                encoding_class: EncodingClass::FieldElementArray,
                width_constraint: WidthConstraint::Any,
                canonical_null_encoding: CanonicalNullEncoding::SeparateNullFlagWithZeroValue,
                transcript_serialization: TranscriptSerialization::FieldElementsWithNullFlag,
                ordering_preserving: None,
            },
            tabula_core::ColumnLayoutKind::SMT_V1,
            tabula_core::RootProfileId::SMT_V1,
        )
        .unwrap();

        assert_ne!(left.semantic_hash, right.semantic_hash);
    }

    #[test]
    fn builtins_validate_without_privileged_path() {
        let catalog = builtin_catalog().unwrap();

        catalog.validate().unwrap();
        assert_eq!(catalog.types.len(), 4);
        assert_eq!(catalog.encodings.len(), 4);
        assert_eq!(catalog.schemes.len(), 2);
        assert!(catalog.columns.is_empty());
    }

    #[test]
    fn catalog_validation_rejects_type_encoding_mismatch() {
        let mut catalog = builtin_catalog().unwrap();
        let bytes32 = catalog
            .types
            .iter()
            .find(|descriptor| descriptor.type_id == TYPE_BYTES32_ID)
            .cloned()
            .unwrap();
        let u64_encoding = catalog
            .encodings
            .iter()
            .find(|profile| profile.encoding_profile_id == ENCODING_U64_ID)
            .cloned()
            .unwrap();
        let smt = catalog
            .schemes
            .iter()
            .find(|profile| profile.scheme_profile_id == SCHEME_PROFILE_SMT_ID)
            .cloned()
            .unwrap();
        let bad_column = ColumnProfile::new(
            ColumnProfileId(7),
            "bad",
            None,
            &bytes32,
            &u64_encoding,
            &smt,
            CommitmentRole::IncludedInRoot,
        )
        .unwrap();
        catalog.register_column(bad_column).unwrap();

        let err = catalog.validate().unwrap_err();
        assert!(matches!(err, ProfileError::EncodingTypeMismatch { .. }));
    }

    #[test]
    fn catalog_validation_rejects_scheme_encoding_incompatibility() {
        let mut catalog = builtin_catalog().unwrap();
        let bytes32 = catalog
            .types
            .iter()
            .find(|descriptor| descriptor.type_id == TYPE_BYTES32_ID)
            .cloned()
            .unwrap();
        let bytes32_encoding = catalog
            .encodings
            .iter()
            .find(|profile| profile.encoding_profile_id == ENCODING_BYTES32_ID)
            .cloned()
            .unwrap();
        let ssmc = catalog
            .schemes
            .iter()
            .find(|profile| profile.scheme_profile_id == SCHEME_PROFILE_SSMC_ID)
            .cloned()
            .unwrap();
        let column = ColumnProfile::new(
            ColumnProfileId(8),
            "bytes32_ssmc",
            None,
            &bytes32,
            &bytes32_encoding,
            &ssmc,
            CommitmentRole::IncludedInRoot,
        )
        .unwrap();
        catalog.register_column(column).unwrap();

        let err = catalog.validate().unwrap_err();
        assert!(matches!(
            err,
            ProfileError::SchemeEncodingIncompatibility { .. }
        ));
    }

    #[test]
    fn resolve_column_profile_returns_complete_join_view() {
        let mut catalog = builtin_catalog().unwrap();
        let u64_type = catalog
            .types
            .iter()
            .find(|descriptor| descriptor.type_id == TYPE_U64_ID)
            .cloned()
            .unwrap();
        let u64_encoding = catalog
            .encodings
            .iter()
            .find(|profile| profile.encoding_profile_id == ENCODING_U64_ID)
            .cloned()
            .unwrap();
        let ssmc = catalog
            .schemes
            .iter()
            .find(|profile| profile.scheme_profile_id == SCHEME_PROFILE_SSMC_ID)
            .cloned()
            .unwrap();
        let column = ColumnProfile::new(
            ColumnProfileId(9),
            "accounts.balance",
            None,
            &u64_type,
            &u64_encoding,
            &ssmc,
            CommitmentRole::IncludedInRoot,
        )
        .unwrap();
        let expected_hash = column.profile_hash;
        catalog.register_column(column).unwrap();

        let resolved = catalog.resolve_column_profile(ColumnProfileId(9)).unwrap();
        assert_eq!(resolved.type_descriptor.type_id, TYPE_U64_ID);
        assert_eq!(
            resolved.encoding_profile.encoding_profile_id,
            ENCODING_U64_ID
        );
        assert_eq!(
            resolved.scheme_profile.scheme_profile_id,
            SCHEME_PROFILE_SSMC_ID
        );
        assert_eq!(resolved.column_profile.profile_hash, expected_hash);
        assert!(resolved.receives_commitment());
    }

    #[test]
    fn built_in_type_ids_resolve_without_loss() {
        let catalog = builtin_catalog().unwrap();
        for type_id in [TYPE_U64_ID, TYPE_I64_ID, TYPE_BOOL_ID, TYPE_BYTES32_ID] {
            let descriptor = catalog
                .type_descriptor(type_id)
                .expect("built-in descriptor");
            assert_eq!(descriptor.type_id, type_id);
        }
    }

    #[test]
    fn builtin_registry_resolves_names_and_defaults() {
        let registry = builtin_semantic_registry().unwrap();

        assert_eq!(registry.resolve_type_name("u64").unwrap(), TYPE_U64_ID);
        assert_eq!(registry.resolve_type_name("bool").unwrap(), TYPE_BOOL_ID);
        assert_eq!(
            registry.resolve_scheme_name("ssmc").unwrap(),
            tabula_core::SchemeId::SSMC
        );
        assert_eq!(
            registry.resolve_default_encoding(TYPE_U64_ID).unwrap(),
            ENCODING_U64_ID
        );
        assert_eq!(
            registry
                .resolve_default_scheme_profile(tabula_core::SchemeId::SMT, ENCODING_BOOL_ID)
                .unwrap(),
            SCHEME_PROFILE_SMT_ID
        );
    }
}
