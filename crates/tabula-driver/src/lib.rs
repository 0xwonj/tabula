//! Compiler driver: canonical semantic ownership for program loading,
//! registration, and contract metadata generation.
//!
//! CLI commands should call this crate instead of duplicating semantic checks.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use tabula_artifact::{BatchFile, ProgramArtifact, StateCell, StateFile, TxInput};
use tabula_contract::{
    CONTRACT_SCHEMA_VERSION_V1, ContractCompatibilityPolicy, ContractMetadataEnvelope,
    STATEMENT_BINDING_VERSION_V1, apply_batch_binding_registry_v1,
};
use tabula_core::{ColId, TableId, TableSchema, Value};
use tabula_ir::{Program, TxTypeDef};

const PROFILE_HASH_DOMAIN: &[u8] = b"tabula.driver.profile_hash.v1";

/// Program file format used by compile/check/execute commands.
pub type ProgramSourceFile = ProgramArtifact;

/// Driver result type.
pub type DriverResult<T> = Result<T, DriverError>;

/// Program source format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgramSourceFormat {
    /// `.tab` source program.
    TabSource,
    /// JSON artifact program.
    JsonArtifact,
}

/// Contract metadata validation policy for program sources.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetadataPolicy {
    /// Metadata is optional (e.g., source program that will be freshly registered).
    Optional,
    /// Metadata is required and must validate (e.g., precompiled artifact input).
    Required,
}

/// Structured compile diagnostic for adapters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompileDiagnostic {
    /// Compile error kind.
    pub kind: String,
    /// Human-readable diagnostic message.
    pub message: String,
    /// Byte span start.
    pub span_start: usize,
    /// Byte span end.
    pub span_end: usize,
    /// 1-based line.
    pub line: usize,
    /// 1-based column.
    pub col: usize,
}

/// Driver-level error type shared across adapters/orchestration.
#[derive(Debug, Error)]
pub enum DriverError {
    /// Program source read failed.
    #[error("failed to read {path}: {source}")]
    ReadFile {
        /// File path.
        path: String,
        /// Source error.
        #[source]
        source: std::io::Error,
    },
    /// Program JSON parse failed.
    #[error("failed to parse {path}: {source}")]
    ParseJson {
        /// File path or logical label.
        path: String,
        /// Source error.
        #[source]
        source: serde_json::Error,
    },
    /// Program compile failed.
    #[error("program compilation failed")]
    Compile {
        /// Structured diagnostics.
        diagnostics: Vec<CompileDiagnostic>,
    },
    /// Program failed semantic registration.
    #[error("invalid program: {message}")]
    InvalidProgram {
        /// Validation error text.
        message: String,
    },
    /// Compiled artifact is missing contract metadata.
    #[error(
        "compiled program JSON is missing contract_metadata; regenerate with the current driver"
    )]
    MissingContractMetadata,
    /// Compiled artifact metadata mismatched current semantic policy.
    #[error("contract metadata mismatch: {message}")]
    ContractMetadataMismatch {
        /// Validation mismatch details.
        message: String,
    },
}

/// Built-in transfer example source used by adapter commands.
pub const TRANSFER_EXAMPLE_TAB_SOURCE: &str = "\
table balances {
    balance: u64,
}

tx transfer(from: u64, to: u64, amount: u64) {
    let sender_bal = balances[from].balance
    let recv_bal = balances[to].balance
    assert sender_bal >= amount
    balances[from].balance = sender_bal - amount
    balances[to].balance = recv_bal + amount
    emit \"transfer\" (from, to, amount)
}
";

/// Program/state/batch bundle for sample scenarios.
#[derive(Debug, Clone)]
pub struct ExampleBundle {
    /// `.tab` source text.
    pub program_tab_source: String,
    /// Program artifact JSON payload.
    pub program: ProgramSourceFile,
    /// Initial state payload.
    pub state: StateFile,
    /// Batch payload.
    pub batch: BatchFile,
}

/// Registered program artifact produced by the driver.
#[derive(Debug, Clone)]
pub struct RegisteredProgram {
    /// Registered IR program.
    pub program: Program,
    /// Canonical table schemas consumed during registration.
    pub table_schemas: Vec<TableSchema>,
    /// Canonical tx definitions consumed during registration.
    pub tx_types: Vec<TxTypeDef>,
    /// Canonical metadata envelope for proof compatibility checks.
    pub metadata_envelope: ContractMetadataEnvelope,
}

impl RegisteredProgram {
    /// Build strict compatibility policy pinned to this artifact's metadata.
    pub fn compatibility_policy(&self) -> ContractCompatibilityPolicy {
        ContractCompatibilityPolicy {
            expected_profile_hash: self.metadata_envelope.profile_hash,
            expected_contract_schema_version: self.metadata_envelope.contract_schema_version,
            expected_statement_binding_version: self.metadata_envelope.statement_binding_version,
            expected_semantic_hash_stub: self.metadata_envelope.semantic_hash_stub,
        }
    }

    /// Materialize a portable compiled artifact file.
    pub fn into_program_file(self) -> ProgramSourceFile {
        ProgramSourceFile {
            table_schemas: self.table_schemas,
            tx_types: self.tx_types,
            contract_metadata: Some(self.metadata_envelope),
        }
    }
}

/// Load program sources from `.tab` or `.json`.
pub fn load_program_sources(path: &Path) -> anyhow::Result<ProgramSourceFile> {
    load_program_sources_strict(path).map_err(anyhow::Error::new)
}

/// Strict variant of [`load_program_sources`] that returns typed driver errors.
pub fn load_program_sources_strict(path: &Path) -> DriverResult<ProgramSourceFile> {
    let source = std::fs::read_to_string(path).map_err(|source| DriverError::ReadFile {
        path: path.display().to_string(),
        source,
    })?;

    let format = if path.extension().and_then(|e| e.to_str()) == Some("tab") {
        ProgramSourceFormat::TabSource
    } else {
        ProgramSourceFormat::JsonArtifact
    };
    parse_program_sources(&source, format, &path.display().to_string())
}

/// Parse/compile program sources from in-memory text using the given source format.
pub fn parse_program_sources(
    content: &str,
    format: ProgramSourceFormat,
    source_label: &str,
) -> DriverResult<ProgramSourceFile> {
    match format {
        ProgramSourceFormat::TabSource => compile_program_source(content),
        ProgramSourceFormat::JsonArtifact => {
            serde_json::from_str(content).map_err(|source| DriverError::ParseJson {
                path: source_label.to_string(),
                source,
            })
        }
    }
}

/// Compile a `.tab` source string into a program artifact source file.
pub fn compile_program_source(source: &str) -> DriverResult<ProgramSourceFile> {
    match tabula_lang::compile(source) {
        Ok(compiled) => Ok(ProgramSourceFile {
            table_schemas: compiled.schemas,
            tx_types: compiled.tx_types,
            contract_metadata: None,
        }),
        Err(errors) => Err(DriverError::Compile {
            diagnostics: compile_diagnostics(source, &errors),
        }),
    }
}

/// Register program sources using explicit metadata policy.
pub fn register_program_sources(
    sources: &ProgramSourceFile,
    metadata_policy: MetadataPolicy,
) -> DriverResult<RegisteredProgram> {
    let artifact = register_program(&sources.table_schemas, &sources.tx_types).map_err(|e| {
        DriverError::InvalidProgram {
            message: e.to_string(),
        }
    })?;

    match (metadata_policy, sources.contract_metadata.as_ref()) {
        (MetadataPolicy::Required, None) => Err(DriverError::MissingContractMetadata),
        (_, Some(provided)) => artifact
            .compatibility_policy()
            .validate(provided)
            .map_err(|e| DriverError::ContractMetadataMismatch {
                message: e.to_string(),
            })
            .map(|_| artifact),
        (_, None) => Ok(artifact),
    }
}

/// Build the canonical transfer example bundle.
pub fn transfer_example_bundle() -> anyhow::Result<ExampleBundle> {
    let mut program =
        compile_program_source(TRANSFER_EXAMPLE_TAB_SOURCE).map_err(anyhow::Error::new)?;
    let artifact =
        register_program_sources(&program, MetadataPolicy::Optional).map_err(anyhow::Error::new)?;
    program.contract_metadata = Some(artifact.metadata_envelope);

    let state = StateFile {
        cells: vec![
            StateCell {
                table: 0,
                row: 0,
                col: 0,
                value: Some(Value::U64(1000)),
            },
            StateCell {
                table: 0,
                row: 1,
                col: 0,
                value: Some(Value::U64(500)),
            },
            StateCell {
                table: 0,
                row: 2,
                col: 0,
                value: Some(Value::U64(200)),
            },
        ],
    };

    let batch = BatchFile {
        transactions: vec![
            TxInput {
                tx_type: 0,
                params: vec![Value::U64(0), Value::U64(1), Value::U64(300)],
                sender: "01".repeat(32),
                nonce: 0,
            },
            TxInput {
                tx_type: 0,
                params: vec![Value::U64(1), Value::U64(2), Value::U64(200)],
                sender: "01".repeat(32),
                nonce: 1,
            },
            TxInput {
                tx_type: 0,
                params: vec![Value::U64(2), Value::U64(0), Value::U64(50)],
                sender: "01".repeat(32),
                nonce: 2,
            },
        ],
    };

    Ok(ExampleBundle {
        program_tab_source: TRANSFER_EXAMPLE_TAB_SOURCE.to_string(),
        program,
        state,
        batch,
    })
}

/// Register schemas and tx types into a semantic artifact.
pub fn register_program(
    schemas: &[TableSchema],
    tx_types: &[TxTypeDef],
) -> anyhow::Result<RegisteredProgram> {
    validate_schema_coverage(schemas, tx_types)?;

    let mut program = Program::new();
    for schema in schemas {
        program.add_schema(schema.clone());
    }
    for def in tx_types {
        program.register(def.clone())?;
    }

    // Gate: statement binding registry must remain complete.
    let registry = apply_batch_binding_registry_v1();
    registry
        .validate_completeness()
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;

    let profile_hash = compute_profile_hash(schemas, tx_types)?;
    let metadata_envelope = ContractMetadataEnvelope {
        profile_hash,
        contract_schema_version: CONTRACT_SCHEMA_VERSION_V1,
        statement_binding_version: STATEMENT_BINDING_VERSION_V1,
        semantic_hash_stub: None,
    };

    Ok(RegisteredProgram {
        program,
        table_schemas: schemas.to_vec(),
        tx_types: tx_types.to_vec(),
        metadata_envelope,
    })
}

/// Convenience helper: load sources from a path and register in one step.
pub fn load_and_register_program(path: &Path) -> anyhow::Result<RegisteredProgram> {
    let sources = load_program_sources_strict(path).map_err(anyhow::Error::new)?;
    let metadata_policy = if path.extension().and_then(|e| e.to_str()) == Some("tab") {
        MetadataPolicy::Optional
    } else {
        MetadataPolicy::Required
    };
    register_program_sources(&sources, metadata_policy).map_err(anyhow::Error::new)
}

/// Validate that every state/static-table access has a declared schema+column.
fn validate_schema_coverage(schemas: &[TableSchema], tx_types: &[TxTypeDef]) -> anyhow::Result<()> {
    let mut columns_by_table: BTreeMap<TableId, BTreeSet<ColId>> = BTreeMap::new();
    for schema in schemas {
        let cols = columns_by_table.entry(schema.id).or_default();
        for col in &schema.columns {
            cols.insert(col.id);
        }
    }

    for tx in tx_types {
        for (instr_idx, instr) in tx.body.iter().enumerate() {
            match instr {
                tabula_ir::Instruction::Read { table, col, .. }
                | tabula_ir::Instruction::Write { table, col, .. } => {
                    ensure_table_col_exists(&columns_by_table, tx, instr_idx, *table, *col)?
                }
                tabula_ir::Instruction::Lookup {
                    static_table, col, ..
                } => {
                    ensure_table_col_exists(&columns_by_table, tx, instr_idx, *static_table, *col)?
                }
                _ => {}
            }
        }
    }

    Ok(())
}

fn ensure_table_col_exists(
    columns_by_table: &BTreeMap<TableId, BTreeSet<ColId>>,
    tx: &TxTypeDef,
    instr_idx: usize,
    table: TableId,
    col: ColId,
) -> anyhow::Result<()> {
    let Some(cols) = columns_by_table.get(&table) else {
        bail!(
            "tx '{}' (id {}), instruction {} references table {} but no schema is declared for it",
            tx.name,
            tx.id.0,
            instr_idx,
            table.0
        );
    };
    if !cols.contains(&col) {
        bail!(
            "tx '{}' (id {}), instruction {} references table {} col {} but that column is missing in schema",
            tx.name,
            tx.id.0,
            instr_idx,
            table.0,
            col.0
        );
    }
    Ok(())
}

fn compute_profile_hash(
    schemas: &[TableSchema],
    tx_types: &[TxTypeDef],
) -> anyhow::Result<[u8; 32]> {
    let canonical_schemas = canonicalize_schemas(schemas);
    let canonical_tx_types = canonicalize_tx_types(tx_types);

    let mut hasher = blake3::Hasher::new();
    hasher.update(PROFILE_HASH_DOMAIN);
    hasher.update(&(canonical_schemas.len() as u32).to_be_bytes());
    for schema in &canonical_schemas {
        hasher.update(&borsh::to_vec(schema).context("failed to borsh-encode table schema")?);
    }

    hasher.update(&(canonical_tx_types.len() as u32).to_be_bytes());
    for tx in &canonical_tx_types {
        hasher.update(&borsh::to_vec(tx).context("failed to borsh-encode tx type")?);
    }

    Ok(*hasher.finalize().as_bytes())
}

fn canonicalize_schemas(schemas: &[TableSchema]) -> Vec<TableSchema> {
    let mut out = schemas.to_vec();
    out.sort_by_key(|s| s.id);
    for schema in &mut out {
        schema.columns.sort_by_key(|c| c.id);
    }
    out
}

fn canonicalize_tx_types(tx_types: &[TxTypeDef]) -> Vec<TxTypeDef> {
    let mut out = tx_types.to_vec();
    out.sort_by_key(|tx| tx.id);
    out
}

fn compile_diagnostics(
    source: &str,
    errors: &[tabula_lang::error::CompileError],
) -> Vec<CompileDiagnostic> {
    errors
        .iter()
        .map(|err| {
            let (line, col) = tabula_lang::span::line_col(source, err.span.start);
            CompileDiagnostic {
                kind: format!("{:?}", err.kind),
                message: err.message.clone(),
                span_start: err.span.start,
                span_end: err.span.end,
                line,
                col,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    use tabula_core::{TableSchema, TxTypeId, Value, ValueType};
    use tabula_ir::{Instruction, ParamDef, RowExpr, ValueExpr};

    fn tx_missing_schema() -> TxTypeDef {
        TxTypeDef {
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

    fn tx_missing_col() -> TxTypeDef {
        TxTypeDef {
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
        let tx_types = vec![TxTypeDef {
            id: TxTypeId(2),
            name: "noop".to_string(),
            param_schema: vec![],
            body: vec![],
        }];

        let h1 = compute_profile_hash(&schemas, &tx_types).expect("hash");
        let h2 = compute_profile_hash(&schemas, &tx_types).expect("hash");
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
            tx_types: vec![TxTypeDef {
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
        program.contract_metadata = Some(ContractMetadataEnvelope {
            profile_hash: [0x99; 32],
            contract_schema_version: CONTRACT_SCHEMA_VERSION_V1,
            statement_binding_version: STATEMENT_BINDING_VERSION_V1,
            semantic_hash_stub: None,
        });
        let path = write_temp_program_file(&program);
        let err = load_and_register_program(&path).expect_err("metadata mismatch should fail");
        let driver_err = err
            .downcast_ref::<DriverError>()
            .expect("expected DriverError for metadata-mismatch path");
        match driver_err {
            DriverError::ContractMetadataMismatch { message } => {
                assert!(message.contains("profile hash mismatch"));
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
