//! Compiler: canonical semantic ownership for program loading,
//! registration, and contract metadata generation.
//!
//! CLI commands and daemon service should call this crate instead of
//! duplicating semantic checks. Compiler-side sealing catalogs describe
//! semantic contracts only; they do not install runtime or verifier backends.

mod compile;
mod error;
mod example;
mod load;
mod profile;
mod program;
mod register;
mod sources;

pub use compile::{compile_program_source, compile_program_source_with_catalogs};
pub use error::{CompileDiagnostic, CompilerCatalogError, CompilerError, CompilerResult};
pub use example::{ExampleBundle, TRANSFER_EXAMPLE_TAB_SOURCE, transfer_example_bundle};
pub use load::{
    load_and_register_program, load_artifact, load_artifact_strict, load_program_definition,
    load_program_definition_strict, parse_artifact, parse_program_definition,
};
pub use program::SealedProgram;
pub use register::{
    CompilerCatalogs, PrecompileDescriptorCatalog, register_artifact, register_program_definition,
    register_program_definition_with_catalogs,
};
pub use sources::{ColumnSchemeSelection, ProgramDefinition, SourceColumnDef, SourceTableSchema};

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    use sha2::{Digest as _, Sha256};
    use tabula_artifact::Artifact;
    use tabula_core::{
        ColId, ColumnLayoutKind, ColumnProfileId, RootProfileId, SchemeId, SchemeProfileId,
        TableId, TableSchema, TxTypeId,
    };
    use tabula_ir::{
        Instruction, ParamDef, PrecompileId, PrecompileSignature, PrecompileValueProfile, RowExpr,
        ValueExpr,
    };
    use tabula_profile::{
        CanonicalNullEncoding, CommitmentContractKind, ENCODING_U64_ID, EncodingClass,
        EncodingRequirements, FieldFamily, SCHEME_PROFILE_SMT_ID, SchemeProfile, SemanticRegistry,
        TYPE_BYTES32_ID, TYPE_U64_ID, TranscriptSerialization, VerifierDigestFormat,
        WidthConstraint, builtin_catalog, builtin_semantic_registry, builtin_smt_scheme_profile,
    };

    use tabula_contract::{
        BINDING_VERSION_V1, CONTRACT_SCHEMA_VERSION_V1, STATEMENT_SCHEMA_VERSION_V1,
        VERIFIER_PROFILE_VERSION_V1,
    };

    fn test_column(id: ColId, name: &str, type_id: tabula_core::TypeId) -> tabula_core::ColumnDef {
        let _ = type_id;
        tabula_core::ColumnDef {
            id,
            name: name.to_string(),
            column_profile_id: ColumnProfileId(0),
        }
    }

    fn test_param(name: &str, type_id: tabula_core::TypeId) -> ParamDef {
        ParamDef {
            name: name.to_string(),
            type_id,
        }
    }

    fn source_u64_schema(
        table_id: TableId,
        name: &str,
        col_id: ColId,
        col_name: &str,
    ) -> SourceTableSchema {
        SourceTableSchema {
            id: table_id,
            name: name.to_string(),
            columns: vec![SourceColumnDef {
                id: col_id,
                name: col_name.to_string(),
                type_id: TYPE_U64_ID,
            }],
        }
    }

    fn custom_smt_like_registry(scheme_id: SchemeId) -> SemanticRegistry {
        let mut registry = builtin_semantic_registry().expect("built-in semantic registry");
        let profile = SchemeProfile::new(
            SchemeProfileId(42),
            "custom_smt_like_v1",
            None,
            scheme_id,
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
        .expect("custom scheme profile");
        registry
            .register_scheme_profile(profile)
            .expect("register custom scheme profile");
        registry
            .register_default_scheme_profile(scheme_id, ENCODING_U64_ID, SchemeProfileId(42))
            .expect("register default custom scheme mapping");
        registry.validate().expect("registry validation");
        registry
    }

    fn tx_missing_schema() -> tabula_ir::TxTypeDef {
        tabula_ir::TxTypeDef {
            id: TxTypeId(1),
            name: "missing_schema".to_string(),
            param_schema: vec![],
            body: vec![Instruction::Read {
                dst_val: 0,
                dst_is_null: 1,
                table: TableId(7),
                row: RowExpr::Literal(tabula_core::RowKey(0)),
                col: tabula_core::ColId(0),
            }],
        }
    }

    fn tx_missing_col() -> tabula_ir::TxTypeDef {
        tabula_ir::TxTypeDef {
            id: TxTypeId(1),
            name: "missing_col".to_string(),
            param_schema: vec![],
            body: vec![Instruction::Write {
                table: TableId(1),
                row: RowExpr::Literal(tabula_core::RowKey(0)),
                col: tabula_core::ColId(9),
                src_val: ValueExpr::Literal(tabula_core::PortableValue::new(
                    tabula_profile::TYPE_U64_ID,
                    borsh::to_vec(&7u64).expect("u64 literal"),
                )),
                src_is_null: ValueExpr::Literal(tabula_core::PortableValue::new(
                    tabula_profile::TYPE_BOOL_ID,
                    vec![0],
                )),
            }],
        }
    }

    #[test]
    fn register_program_rejects_missing_schema_for_accessed_table() {
        let err = register_program_definition(&ProgramDefinition {
            table_schemas: vec![],
            tx_types: vec![tx_missing_schema()],
            column_schemes: vec![],
        })
        .unwrap_err();
        assert!(err.to_string().contains("no schema"));
    }

    #[test]
    fn register_program_rejects_missing_column_in_schema() {
        let mut tx = tx_missing_col();
        tx.param_schema = vec![test_param("x", TYPE_U64_ID)];

        let err = register_program_definition(&ProgramDefinition {
            table_schemas: vec![source_u64_schema(
                TableId(1),
                "t",
                tabula_core::ColId(0),
                "x",
            )],
            tx_types: vec![tx],
            column_schemes: vec![],
        })
        .unwrap_err();
        assert!(err.to_string().contains("missing in schema"));
    }

    #[test]
    fn profile_hash_is_deterministic() {
        let schemas = vec![TableSchema {
            id: TableId(1),
            name: "accounts".to_string(),
            columns: vec![
                test_column(tabula_core::ColId(2), "nonce", TYPE_U64_ID),
                test_column(tabula_core::ColId(1), "balance", TYPE_U64_ID),
            ],
        }];
        let tx_types = vec![tabula_ir::TxTypeDef {
            id: TxTypeId(2),
            name: "noop".to_string(),
            param_schema: vec![],
            body: vec![],
        }];

        let catalog = builtin_catalog().expect("built-in profile catalog");
        let h1 = profile::compute_profile_hash(&schemas, &tx_types, &catalog).expect("hash");
        let h2 = profile::compute_profile_hash(&schemas, &tx_types, &catalog).expect("hash");
        assert_eq!(h1, h2);
    }

    fn write_temp_program_file(program: &Artifact) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("tabula_compiler_test_{nonce}.json"));
        let body = serde_json::to_string_pretty(program).expect("serialize");
        std::fs::write(&path, body).expect("write temp program file");
        path
    }

    fn simple_valid_program_sources() -> ProgramDefinition {
        ProgramDefinition {
            table_schemas: vec![source_u64_schema(
                TableId(1),
                "accounts",
                tabula_core::ColId(0),
                "balance",
            )],
            tx_types: vec![tabula_ir::TxTypeDef {
                id: TxTypeId(1),
                name: "touch".to_string(),
                param_schema: vec![],
                body: vec![Instruction::Read {
                    dst_val: 0,
                    dst_is_null: 1,
                    table: TableId(1),
                    col: tabula_core::ColId(0),
                    row: RowExpr::Literal(tabula_core::RowKey(0)),
                }],
            }],
            column_schemes: vec![],
        }
    }

    fn precompile_program_sources() -> ProgramDefinition {
        ProgramDefinition {
            table_schemas: vec![],
            tx_types: vec![tabula_ir::TxTypeDef {
                id: TxTypeId(1),
                name: "pcall".to_string(),
                param_schema: vec![],
                body: vec![Instruction::Precompile {
                    id: tabula_ir::PrecompileId(0x0001),
                    dst_slots: vec![0],
                    inputs: vec![],
                }],
            }],
            column_schemes: vec![],
        }
    }

    fn precompile_descriptor(id: tabula_ir::PrecompileId) -> tabula_artifact::PrecompileDescriptor {
        tabula_artifact::PrecompileDescriptor::new(
            id,
            1,
            PrecompileSignature::new(
                vec![],
                vec![PrecompileValueProfile {
                    type_id: TYPE_U64_ID,
                    encoding_profile_id: ENCODING_U64_ID,
                }],
            ),
            semantic_hash("testing.constant_one.semantic"),
        )
    }

    fn mismatched_precompile_descriptor(
        id: tabula_ir::PrecompileId,
    ) -> tabula_artifact::PrecompileDescriptor {
        tabula_artifact::PrecompileDescriptor::new(
            id,
            1,
            PrecompileSignature::new(
                vec![],
                vec![PrecompileValueProfile {
                    type_id: TYPE_U64_ID,
                    encoding_profile_id: tabula_profile::ENCODING_BYTES32_ID,
                }],
            ),
            semantic_hash("testing.mismatched.semantic"),
        )
    }

    fn wide_precompile_descriptor(
        id: tabula_ir::PrecompileId,
    ) -> tabula_artifact::PrecompileDescriptor {
        tabula_artifact::PrecompileDescriptor::new(
            id,
            1,
            PrecompileSignature::new(
                vec![],
                vec![PrecompileValueProfile {
                    type_id: TYPE_BYTES32_ID,
                    encoding_profile_id: tabula_profile::ENCODING_BYTES32_ID,
                }],
            ),
            semantic_hash("testing.wide.semantic"),
        )
    }

    fn semantic_hash(label: &str) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(b"tabula.compiler.tests.precompile.semantic.v1");
        hasher.update(label.as_bytes());
        hasher.finalize().into()
    }

    fn precompile_artifact() -> Artifact {
        let definition = precompile_program_sources();
        let descriptor = precompile_descriptor(tabula_ir::PrecompileId(0x0001));
        let catalogs = CompilerCatalogs::standard()
            .with_precompile_descriptor(descriptor)
            .expect("register precompile descriptor");
        register_program_definition_with_catalogs(&definition, &catalogs)
            .expect("register precompile program")
            .into_artifact()
    }

    fn extend_artifact_with_builtin_precompile_profile(
        artifact: &mut Artifact,
        type_id: tabula_core::TypeId,
        encoding_profile_id: tabula_core::EncodingProfileId,
    ) {
        let builtins = builtin_semantic_registry().expect("built-in semantic registry");
        let type_descriptor = builtins
            .catalog()
            .type_descriptor(type_id)
            .expect("builtin type descriptor")
            .clone();
        let encoding_profile = builtins
            .catalog()
            .encoding_profile(encoding_profile_id)
            .expect("builtin encoding profile")
            .clone();
        if artifact.profile_catalog.type_descriptor(type_id).is_err() {
            artifact
                .profile_catalog
                .register_type(type_descriptor)
                .expect("register builtin type descriptor");
        }
        if artifact
            .profile_catalog
            .encoding_profile(encoding_profile_id)
            .is_err()
        {
            artifact
                .profile_catalog
                .register_encoding(encoding_profile)
                .expect("register builtin encoding profile");
        }
    }

    #[test]
    fn compiler_catalogs_reject_duplicate_precompile_descriptor_ids() {
        let descriptor = precompile_descriptor(tabula_ir::PrecompileId(0x0001));
        let err = CompilerCatalogs::standard()
            .with_precompile_descriptor(descriptor.clone())
            .expect("first descriptor registration")
            .with_precompile_descriptor(descriptor)
            .expect_err("duplicate descriptor registration must fail");

        assert!(matches!(
            err,
            CompilerCatalogError::DuplicatePrecompileDescriptor { .. }
        ));
    }

    #[test]
    fn compiler_catalogs_reject_incompatible_precompile_descriptor_encoding() {
        let err = CompilerCatalogs::standard()
            .with_precompile_descriptor(mismatched_precompile_descriptor(tabula_ir::PrecompileId(
                0x0001,
            )))
            .expect_err("incompatible descriptor registration must fail");

        match err {
            CompilerCatalogError::InvalidPrecompileDescriptor { detail } => {
                assert!(detail.contains("incompatible encoding profile"));
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn compiler_catalogs_reject_precompile_descriptor_wider_than_execution_width() {
        let err = CompilerCatalogs::standard()
            .with_precompile_descriptor(wide_precompile_descriptor(tabula_ir::PrecompileId(0x0001)))
            .expect_err("wide precompile descriptor registration must fail");

        match err {
            CompilerCatalogError::InvalidPrecompileDescriptor { detail } => {
                assert!(detail.contains("generic execution lane only supports width 3"));
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn json_artifact_requires_contract_metadata() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("tabula_compiler_test_{nonce}.json"));
        std::fs::write(&path, r#"{"table_schemas":[],"tx_types":[]}"#)
            .expect("write temp program file");
        let err = load_and_register_program(&path).expect_err("metadata missing should fail");
        let compiler_err = err
            .downcast_ref::<CompilerError>()
            .expect("expected CompilerError for malformed artifact path");
        assert!(matches!(compiler_err, CompilerError::ParseJson { .. }));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn json_artifact_metadata_mismatch_fails_closed() {
        let program_sources = simple_valid_program_sources();
        let mut program = register_program_definition(&program_sources)
            .expect("compile sources")
            .as_artifact();
        program.contract_metadata = tabula_contract::ContractMetadataEnvelope {
            profile_hash: [0x99; 32],
            contract_schema_version: CONTRACT_SCHEMA_VERSION_V1,
            binding_version: BINDING_VERSION_V1,
            statement_schema_version: STATEMENT_SCHEMA_VERSION_V1,
            verifier_profile_version: VERIFIER_PROFILE_VERSION_V1,
            semantic_hash_stub: Some([0x55; 32]),
        };
        let path = write_temp_program_file(&program);
        let err = load_and_register_program(&path).expect_err("metadata mismatch should fail");
        let compiler_err = err
            .downcast_ref::<CompilerError>()
            .expect("expected CompilerError for metadata-mismatch path");
        match compiler_err {
            CompilerError::ContractMetadataMismatch(source) => {
                assert!(source.to_string().contains("profile hash mismatch"));
            }
            other => panic!("unexpected compiler error: {other}"),
        }
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn json_artifact_invalid_profile_catalog_fails_closed() {
        let program_sources = simple_valid_program_sources();
        let mut program = register_program_definition(&program_sources)
            .expect("compile sources")
            .as_artifact();
        program.profile_catalog.columns.clear();
        let path = write_temp_program_file(&program);
        let err =
            load_and_register_program(&path).expect_err("invalid profile catalog should fail");
        let compiler_err = err
            .downcast_ref::<CompilerError>()
            .expect("expected CompilerError for artifact-mismatch path");
        match compiler_err {
            CompilerError::InvalidProgram(source) => {
                assert!(source.to_string().contains("invalid column profile"));
            }
            other => panic!("unexpected compiler error: {other}"),
        }
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn json_artifact_rejects_profile_catalog_override_without_metadata_update() {
        let program_sources = simple_valid_program_sources();
        let mut program = register_program_definition(&program_sources)
            .expect("compile sources")
            .as_artifact();

        let smt_profile = builtin_smt_scheme_profile().expect("builtin smt scheme profile");
        if program
            .profile_catalog
            .scheme_profile(smt_profile.scheme_profile_id)
            .is_err()
        {
            program
                .profile_catalog
                .register_scheme(smt_profile.clone())
                .expect("register builtin smt scheme profile");
        }

        let type_id = program.profile_catalog.columns[0].type_id;
        let encoding_profile_id = program.profile_catalog.columns[0].encoding_profile_id;
        let type_descriptor = program
            .profile_catalog
            .type_descriptor(type_id)
            .expect("column type descriptor")
            .clone();
        let encoding_profile = program
            .profile_catalog
            .encoding_profile(encoding_profile_id)
            .expect("column encoding profile")
            .clone();
        let mut overridden_profile = program.profile_catalog.columns[0].clone();
        overridden_profile.scheme_profile_id = SCHEME_PROFILE_SMT_ID;
        overridden_profile.profile_hash = overridden_profile
            .compute_profile_hash(&type_descriptor, &encoding_profile, &smt_profile)
            .expect("column profile hash");
        program.profile_catalog.columns[0] = overridden_profile;

        let err = register_artifact(&program)
            .expect_err("profile-catalog-only override without metadata update must fail");
        match err {
            CompilerError::ContractMetadataMismatch(_) | CompilerError::ArtifactMismatch { .. } => {
            }
            other => panic!("unexpected compiler error: {other}"),
        }
    }

    #[test]
    fn artifact_with_missing_referenced_precompile_descriptor_fails_registration() {
        let artifact = precompile_artifact();
        let mut malformed = artifact.clone();
        malformed.precompile_manifest.clear();

        let err = register_artifact(&malformed).expect_err("missing precompile manifest must fail");
        match err {
            CompilerError::InvalidProgram(source) => {
                let detail = source.to_string();
                assert!(
                    detail.contains("precompile 0x0001 has no sealed signature")
                        || detail.contains("precompile manifest ids"),
                    "unexpected error detail: {detail}",
                );
            }
            other => panic!("unexpected compiler error: {other}"),
        }
    }

    #[test]
    fn artifact_with_extra_unreferenced_precompile_descriptor_fails_registration() {
        let mut artifact = precompile_artifact();
        artifact
            .precompile_manifest
            .push(precompile_descriptor(PrecompileId(0x00ff)));

        let err =
            register_artifact(&artifact).expect_err("extra precompile manifest entry must fail");
        match err {
            CompilerError::InvalidProgram(source) => {
                assert!(source.to_string().contains("precompile manifest ids"));
            }
            other => panic!("unexpected compiler error: {other}"),
        }
    }

    #[test]
    fn artifact_with_incompatible_precompile_signature_fails_registration() {
        let mut artifact = precompile_artifact();
        extend_artifact_with_builtin_precompile_profile(
            &mut artifact,
            TYPE_BYTES32_ID,
            tabula_profile::ENCODING_BYTES32_ID,
        );
        artifact.precompile_manifest[0] = mismatched_precompile_descriptor(PrecompileId(0x0001));

        let err = register_artifact(&artifact)
            .expect_err("artifact with incompatible precompile signature must fail");
        match err {
            CompilerError::ArtifactMismatch { detail } => {
                assert!(detail.contains("incompatible encoding profile"));
            }
            other => panic!("unexpected compiler error: {other}"),
        }
    }

    #[test]
    fn artifact_with_wide_precompile_signature_fails_registration() {
        let mut artifact = precompile_artifact();
        extend_artifact_with_builtin_precompile_profile(
            &mut artifact,
            TYPE_BYTES32_ID,
            tabula_profile::ENCODING_BYTES32_ID,
        );
        artifact.precompile_manifest[0] = wide_precompile_descriptor(PrecompileId(0x0001));

        let err = register_artifact(&artifact)
            .expect_err("artifact with wide precompile signature must fail");
        match err {
            CompilerError::ArtifactMismatch { detail } => {
                assert!(detail.contains("generic execution lane only supports width 3"));
            }
            other => panic!("unexpected compiler error: {other}"),
        }
    }

    #[test]
    fn semantic_hash_stub_changes_with_precompile_manifest() {
        let artifact = precompile_artifact();
        let first = register_artifact(&artifact).expect("register original artifact");

        let mut modified = artifact.clone();
        modified.precompile_manifest[0] = tabula_artifact::PrecompileDescriptor::new(
            modified.precompile_manifest[0].precompile_id,
            modified.precompile_manifest[0].precompile_version + 1,
            modified.precompile_manifest[0].signature.clone(),
            semantic_hash("testing.constant_one.semantic.v2"),
        );
        modified.contract_metadata.semantic_hash_stub = Some([0u8; 32]);

        let err = register_artifact(&modified).expect_err("stale semantic hash must fail closed");
        match err {
            CompilerError::ContractMetadataMismatch(source) => {
                assert!(source.to_string().contains("semantic hash"));
            }
            other => panic!("unexpected compiler error: {other}"),
        }

        let second = first.metadata_envelope().semantic_hash_stub;
        assert!(
            second.is_some(),
            "sealed programs must carry semantic hash stubs"
        );
    }

    #[test]
    fn compile_program_source_returns_structured_diagnostics() {
        let bad_source = "table t { v: u64 }\n tx x() { let a = unknown[0].v }";
        let err = compile_program_source(bad_source).expect_err("compile should fail");
        let CompilerError::Compile { diagnostics } = err else {
            panic!("expected compile diagnostics error");
        };
        assert!(!diagnostics.is_empty(), "diagnostics should be present");
        assert!(diagnostics.iter().all(|d| d.line >= 1 && d.col >= 1));
    }

    #[test]
    fn register_program_definition_accepts_source_without_metadata() {
        let program = simple_valid_program_sources();
        let compiled =
            register_program_definition(&program).expect("source registration should work");
        assert_eq!(compiled.table_schemas().len(), 1);
    }

    #[test]
    fn compile_program_source_preserves_column_scheme_annotations() {
        let source = "table t { a: u64 @smt, b: u64 @scheme(42) }\ntx noop() {}";
        let program = compile_program_source(source).expect("compile source");
        assert_eq!(program.column_schemes.len(), 2);
        assert_eq!(program.column_schemes[0].table_id, TableId(0));
        assert_eq!(program.column_schemes[0].col_id, tabula_core::ColId(0));
        assert_eq!(program.column_schemes[0].scheme_id, SchemeId::SMT);
        assert_eq!(program.column_schemes[1].col_id, tabula_core::ColId(1));
        assert_eq!(program.column_schemes[1].scheme_id, SchemeId(42));

        let err = register_program_definition(&program).expect_err("custom scheme needs catalog");
        match err {
            CompilerError::InvalidProgram(source) => {
                assert!(
                    source
                        .to_string()
                        .contains("has no registered default scheme profile")
                );
            }
            other => panic!("unexpected compiler error: {other}"),
        }

        let compiled = register_program_definition_with_catalogs(
            &program,
            &CompilerCatalogs::standard()
                .with_semantic_registry(custom_smt_like_registry(SchemeId(42)))
                .expect("custom registry"),
        )
        .expect("register source");
        let col0 = compiled
            .resolve_column_profile(TableId(0), tabula_core::ColId(0))
            .expect("resolve first column");
        let col1 = compiled
            .resolve_column_profile(TableId(0), tabula_core::ColId(1))
            .expect("resolve second column");
        assert_eq!(col0.scheme_profile.scheme_family_id, SchemeId::SMT);
        assert_eq!(col1.scheme_profile.scheme_family_id, SchemeId(42));
    }

    #[test]
    fn register_program_definition_rejects_missing_custom_scheme_semantics() {
        let source = "table t { a: u64 @scheme(42) }\ntx noop() {}";
        let program = compile_program_source(source).expect("compile source");

        let err =
            register_program_definition_with_catalogs(&program, &CompilerCatalogs::standard())
                .expect_err("catalog mismatch should fail");
        match err {
            CompilerError::InvalidProgram(source) => {
                assert!(
                    source
                        .to_string()
                        .contains("has no registered default scheme profile")
                );
            }
            other => panic!("unexpected compiler error: {other}"),
        }
    }

    #[test]
    fn transfer_example_bundle_contains_compiled_metadata() {
        let bundle = transfer_example_bundle().expect("example bundle");
        assert_eq!(bundle.program.tx_types.len(), 1);
        assert_eq!(bundle.state.cells.len(), 3);
        assert_eq!(bundle.batch.transactions.len(), 3);
        assert_eq!(
            bundle.program.contract_metadata.contract_schema_version,
            CONTRACT_SCHEMA_VERSION_V1
        );
        assert_eq!(bundle.program.table_schemas.len(), 1);
        let resolved = bundle
            .program
            .resolve_column_profile(TableId(0), tabula_core::ColId(0))
            .expect("resolve example column");
        assert_eq!(resolved.scheme_profile.scheme_family_id, SchemeId::SSMC);
    }
}
