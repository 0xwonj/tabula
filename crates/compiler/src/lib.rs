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
    load_and_register_program, load_program_artifact, load_program_artifact_strict,
    load_program_definition, load_program_definition_strict, parse_program_artifact,
    parse_program_definition,
};
pub use program::CompiledProgram;
pub use register::{register_program, register_program_artifact, register_program_definition};
pub use sources::{ColumnSchemeSelection, ProgramDefinition};

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    use tabula_artifact::ProgramArtifact;
    use tabula_core::{SchemeId, TableId, TableSchema, TxTypeId, Value, ValueType};
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

    fn write_temp_program_file(program: &ProgramArtifact) -> std::path::PathBuf {
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
            .as_program_artifact();
        program.contract_metadata = tabula_contract::ContractMetadataEnvelope {
            profile_hash: [0x99; 32],
            contract_schema_version: CONTRACT_SCHEMA_VERSION_V1,
            binding_version: BINDING_VERSION_V1,
            statement_schema_version: STATEMENT_SCHEMA_VERSION_V1,
            verifier_profile_version: VERIFIER_PROFILE_VERSION_V1,
            semantic_hash_stub: None,
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
            .as_program_artifact();
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
    fn json_artifact_can_override_column_scheme_plan() {
        let program_sources = simple_valid_program_sources();
        let mut program = register_program_definition(&program_sources)
            .expect("compile sources")
            .as_program_artifact();
        program.column_proof_plan[0].scheme_id = SchemeId::SMT;

        let compiled =
            register_program_artifact(&program).expect("artifact with explicit scheme plan");

        assert_eq!(compiled.column_proof_plan()[0].scheme_id, SchemeId::SMT);
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

        let compiled = register_program_definition(&program).expect("register source");
        assert_eq!(compiled.column_proof_plan()[0].scheme_id, SchemeId::SMT);
        assert_eq!(compiled.column_proof_plan()[1].scheme_id, SchemeId(42));
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
