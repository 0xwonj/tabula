//! Compiler: canonical semantic ownership for program loading,
//! registration, and contract metadata generation.
//!
//! CLI commands and daemon service should call this crate instead of
//! duplicating semantic checks.

mod compile;
mod error;
mod example;
mod load;
mod profile;
mod program;
mod register;
mod sources;

pub use compile::compile_program_source;
pub use error::{CompileDiagnostic, CompilerError, CompilerResult};
pub use example::{ExampleBundle, TRANSFER_EXAMPLE_TAB_SOURCE, transfer_example_bundle};
pub use load::{
    load_and_register_program, load_artifact, load_artifact_strict, load_program_definition,
    load_program_definition_strict, parse_artifact, parse_program_definition,
};
pub use program::SealedProgram;
pub use register::{
    CompilerCatalogs, PrecompileDescriptorCatalog, SchemeDescriptorCatalog, register_artifact,
    register_program, register_program_definition, register_program_definition_with_catalogs,
};
pub use sources::{ColumnSchemeSelection, ProgramDefinition};

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    use tabula_artifact::{Artifact, SchemeDescriptor};
    use tabula_core::{
        ColumnLayoutKind, RootProfileId, SchemeId, TableId, TableSchema, TxTypeId, Value, ValueType,
    };
    use tabula_ir::{Instruction, ParamDef, RowExpr, ValueExpr};

    use tabula_contract::{
        BINDING_VERSION_V1, CONTRACT_SCHEMA_VERSION_V1, STATEMENT_SCHEMA_VERSION_V1,
        VERIFIER_PROFILE_VERSION_V1,
    };

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
                src_val: ValueExpr::Literal(Value::U64(7)),
                src_is_null: ValueExpr::Literal(Value::Bool(false)),
            }],
        }
    }

    #[test]
    fn register_program_rejects_missing_schema_for_accessed_table() {
        let err = register_program(&[], &[tx_missing_schema()]).unwrap_err();
        assert!(err.to_string().contains("no schema"));
    }

    #[test]
    fn register_program_rejects_missing_column_in_schema() {
        let schemas = vec![TableSchema {
            id: TableId(1),
            name: "t".to_string(),
            columns: vec![tabula_core::ColumnDef {
                id: tabula_core::ColId(0),
                name: "x".to_string(),
                value_type: ValueType::U64,
            }],
        }];
        let mut tx = tx_missing_col();
        tx.param_schema = vec![ParamDef {
            name: "x".to_string(),
            value_type: ValueType::U64,
        }];

        let err = register_program(&schemas, &[tx]).unwrap_err();
        assert!(err.to_string().contains("missing in schema"));
    }

    #[test]
    fn profile_hash_is_deterministic() {
        let schemas = vec![TableSchema {
            id: TableId(1),
            name: "accounts".to_string(),
            columns: vec![
                tabula_core::ColumnDef {
                    id: tabula_core::ColId(2),
                    name: "nonce".to_string(),
                    value_type: ValueType::U64,
                },
                tabula_core::ColumnDef {
                    id: tabula_core::ColId(1),
                    name: "balance".to_string(),
                    value_type: ValueType::U64,
                },
            ],
        }];
        let tx_types = vec![tabula_ir::TxTypeDef {
            id: TxTypeId(2),
            name: "noop".to_string(),
            param_schema: vec![],
            body: vec![],
        }];

        let h1 = profile::compute_profile_hash(&schemas, &tx_types).expect("hash");
        let h2 = profile::compute_profile_hash(&schemas, &tx_types).expect("hash");
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
            table_schemas: vec![TableSchema {
                id: TableId(1),
                name: "accounts".to_string(),
                columns: vec![tabula_core::ColumnDef {
                    id: tabula_core::ColId(0),
                    name: "balance".to_string(),
                    value_type: ValueType::U64,
                }],
            }],
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
        tabula_artifact::PrecompileDescriptor::from_labels(
            id,
            1,
            "testing.constant_one.params",
            "testing.constant_one.semantic",
        )
    }

    fn precompile_artifact() -> Artifact {
        let definition = precompile_program_sources();
        let mut precompiles = PrecompileDescriptorCatalog::new();
        let descriptor = precompile_descriptor(tabula_ir::PrecompileId(0x0001));
        precompiles.insert(descriptor.precompile_id, descriptor);
        register_program_definition_with_catalogs(
            &definition,
            &CompilerCatalogs {
                schemes: SchemeDescriptorCatalog::new(),
                precompiles,
            },
        )
        .expect("register precompile program")
        .into_artifact()
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
    fn json_artifact_proof_plan_mismatch_fails_closed() {
        let program_sources = simple_valid_program_sources();
        let mut program = register_program_definition(&program_sources)
            .expect("compile sources")
            .as_artifact();
        program.column_proof_plan.clear();
        let path = write_temp_program_file(&program);
        let err = load_and_register_program(&path).expect_err("proof plan mismatch should fail");
        let compiler_err = err
            .downcast_ref::<CompilerError>()
            .expect("expected CompilerError for artifact-mismatch path");
        match compiler_err {
            CompilerError::InvalidProgram(source) => {
                assert!(source.to_string().contains("column proof plan"));
            }
            other => panic!("unexpected compiler error: {other}"),
        }
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn json_artifact_can_override_column_scheme_plan_with_updated_metadata() {
        let program_sources = simple_valid_program_sources();
        let mut program = register_program_definition(&program_sources)
            .expect("compile sources")
            .as_artifact();
        program.column_proof_plan[0].scheme_id = SchemeId::SMT;
        program.column_proof_plan[0].scheme_descriptor = SchemeDescriptor::builtin_smt();
        program.contract_metadata.semantic_hash_stub = Some(
            profile::compute_semantic_hash_stub(
                &program.precompile_manifest,
                &program.required_property_requirements,
                &program.column_proof_plan,
            )
            .expect("semantic hash"),
        );

        let compiled = register_artifact(&program).expect("artifact with explicit scheme plan");

        assert_eq!(compiled.column_proof_plan()[0].scheme_id, SchemeId::SMT);
    }

    #[test]
    fn artifact_with_missing_referenced_precompile_descriptor_fails_registration() {
        let artifact = precompile_artifact();
        let mut malformed = artifact.clone();
        malformed.precompile_manifest.clear();

        let err = register_artifact(&malformed).expect_err("missing precompile manifest must fail");
        match err {
            CompilerError::InvalidProgram(source) => {
                assert!(source.to_string().contains("precompile manifest ids"));
            }
            other => panic!("unexpected compiler error: {other}"),
        }
    }

    #[test]
    fn artifact_with_extra_unreferenced_precompile_descriptor_fails_registration() {
        let mut artifact = precompile_artifact();
        artifact
            .precompile_manifest
            .push(tabula_artifact::PrecompileDescriptor::from_labels(
                tabula_ir::PrecompileId(0x00ff),
                1,
                "extra.params",
                "extra.semantic",
            ));

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
    fn semantic_hash_stub_changes_with_precompile_manifest() {
        let artifact = precompile_artifact();
        let first = register_artifact(&artifact).expect("register original artifact");

        let mut modified = artifact.clone();
        modified.precompile_manifest[0] = tabula_artifact::PrecompileDescriptor::from_labels(
            modified.precompile_manifest[0].precompile_id,
            modified.precompile_manifest[0].precompile_version + 1,
            "testing.constant_one.params.v2",
            "testing.constant_one.semantic.v2",
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
                assert!(source.to_string().contains("custom scheme id 42"));
            }
            other => panic!("unexpected compiler error: {other}"),
        }

        let mut scheme_catalog = SchemeDescriptorCatalog::new();
        scheme_catalog.insert(
            SchemeId(42),
            SchemeDescriptor {
                scheme_id: SchemeId(42),
                scheme_version: 1,
                layout_kind: ColumnLayoutKind::SMT_V1,
                params_hash: [0x42; 32],
                root_profile_id: RootProfileId::SMT_V1,
                supported_property_query_kinds: vec![],
            },
        );
        let compiled = register_program_definition_with_catalogs(
            &program,
            &CompilerCatalogs {
                schemes: scheme_catalog,
                precompiles: Default::default(),
            },
        )
        .expect("register source");
        assert_eq!(compiled.column_proof_plan()[0].scheme_id, SchemeId::SMT);
        assert_eq!(compiled.column_proof_plan()[1].scheme_id, SchemeId(42));
    }

    #[test]
    fn register_program_definition_rejects_mismatched_custom_scheme_catalog_entry() {
        let source = "table t { a: u64 @scheme(42) }\ntx noop() {}";
        let program = compile_program_source(source).expect("compile source");

        let mut scheme_catalog = SchemeDescriptorCatalog::new();
        scheme_catalog.insert(
            SchemeId(42),
            SchemeDescriptor {
                scheme_id: SchemeId(99),
                scheme_version: 1,
                layout_kind: ColumnLayoutKind::SMT_V1,
                params_hash: [0x42; 32],
                root_profile_id: RootProfileId::SMT_V1,
                supported_property_query_kinds: vec![],
            },
        );

        let err = register_program_definition_with_catalogs(
            &program,
            &CompilerCatalogs {
                schemes: scheme_catalog,
                precompiles: Default::default(),
            },
        )
        .expect_err("catalog mismatch should fail");
        match err {
            CompilerError::InvalidProgram(source) => {
                assert!(source.to_string().contains("catalog mismatch"));
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
        assert_eq!(bundle.program.column_proof_plan.len(), 1);
    }
}
