//! Compiler driver: canonical semantic ownership for program loading,
//! registration, execution, and contract metadata generation.
//!
//! CLI commands and daemon service should call this crate instead of
//! duplicating semantic checks and execution pipeline assembly.

mod compile;
mod error;
mod example;
pub mod execute;
mod load;
mod profile;
mod register;

// --- Public API re-exports (preserves all existing public symbols) ---

/// Program file format used by compile/check/execute commands.
pub type ProgramSourceFile = tabula_artifact::ProgramArtifact;

pub use compile::compile_program_source;
pub use error::{CompileDiagnostic, DriverError, DriverResult};
pub use example::{ExampleBundle, TRANSFER_EXAMPLE_TAB_SOURCE, transfer_example_bundle};
pub use load::{
    ProgramSourceFormat, load_and_register_program, load_program_sources,
    load_program_sources_strict, parse_program_sources,
};
pub use register::{MetadataPolicy, RegisteredProgram, register_program, register_program_sources};

pub use execute::{BatchInput, ExecutedBatch, run_batch};

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    use tabula_core::{TableId, TableSchema, TxTypeId, Value, ValueType};
    use tabula_ir::{Instruction, ParamDef, RowExpr, ValueExpr};

    use tabula_contract::{BINDING_VERSION_V1, CONTRACT_SCHEMA_VERSION_V1};

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

    fn write_temp_program_file(program: &ProgramSourceFile) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("tabula_driver_test_{nonce}.json"));
        let body = serde_json::to_string_pretty(program).expect("serialize");
        std::fs::write(&path, body).expect("write temp program file");
        path
    }

    fn simple_valid_program_sources() -> ProgramSourceFile {
        ProgramSourceFile {
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
            contract_metadata: None,
        }
    }

    #[test]
    fn json_artifact_requires_contract_metadata() {
        let program = simple_valid_program_sources();
        let path = write_temp_program_file(&program);
        let err = load_and_register_program(&path).expect_err("metadata missing should fail");
        let driver_err = err
            .downcast_ref::<DriverError>()
            .expect("expected DriverError for metadata-missing path");
        assert!(matches!(driver_err, DriverError::MissingContractMetadata));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn json_artifact_metadata_mismatch_fails_closed() {
        let mut program = simple_valid_program_sources();
        program.contract_metadata = Some(tabula_contract::ContractMetadataEnvelope {
            profile_hash: [0x99; 32],
            contract_schema_version: CONTRACT_SCHEMA_VERSION_V1,
            binding_version: BINDING_VERSION_V1,
            semantic_hash_stub: None,
        });
        let path = write_temp_program_file(&program);
        let err = load_and_register_program(&path).expect_err("metadata mismatch should fail");
        let driver_err = err
            .downcast_ref::<DriverError>()
            .expect("expected DriverError for metadata-mismatch path");
        match driver_err {
            DriverError::ContractMetadataMismatch(source) => {
                assert!(source.to_string().contains("profile hash mismatch"));
            }
            other => panic!("unexpected driver error: {other}"),
        }
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn compile_program_source_returns_structured_diagnostics() {
        let bad_source = "table t { v: u64 }\n tx x() { let a = unknown[0].v }";
        let err = compile_program_source(bad_source).expect_err("compile should fail");
        let DriverError::Compile { diagnostics } = err else {
            panic!("expected compile diagnostics error");
        };
        assert!(!diagnostics.is_empty(), "diagnostics should be present");
        assert!(diagnostics.iter().all(|d| d.line >= 1 && d.col >= 1));
    }

    #[test]
    fn register_program_sources_required_metadata_rejects_missing() {
        let program = simple_valid_program_sources();
        let err = register_program_sources(&program, MetadataPolicy::Required)
            .expect_err("required metadata should fail");
        assert!(matches!(err, DriverError::MissingContractMetadata));
    }

    #[test]
    fn transfer_example_bundle_contains_registered_metadata() {
        let bundle = transfer_example_bundle().expect("example bundle");
        assert_eq!(bundle.program.tx_types.len(), 1);
        assert_eq!(bundle.state.cells.len(), 3);
        assert_eq!(bundle.batch.transactions.len(), 3);
        assert!(bundle.program.contract_metadata.is_some());
    }
}
