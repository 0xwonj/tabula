//! Compiler driver: canonical semantic ownership for program loading,
//! registration, and contract metadata generation.
//!
//! CLI commands should call this crate instead of duplicating semantic checks.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};

use tabula_contract::{
    CONTRACT_SCHEMA_VERSION_V1, ContractCompatibilityPolicy, ContractMetadataEnvelope,
    STATEMENT_BINDING_VERSION_V1, apply_batch_binding_registry_v1,
};
use tabula_core::{ColId, TableId, TableSchema};
use tabula_ir::{Program, TxTypeDef};

const PROFILE_HASH_DOMAIN: &[u8] = b"tabula.driver.profile_hash.v1";

/// Program file format used by compile/check/execute commands.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgramSourceFile {
    /// Table schema definitions.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub table_schemas: Vec<TableSchema>,
    /// Transaction type definitions.
    pub tx_types: Vec<TxTypeDef>,
    /// Optional metadata envelope (required for JSON artifact mode).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contract_metadata: Option<ContractMetadataEnvelope>,
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
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    if ext == "tab" {
        let source = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        match tabula_lang::compile(&source) {
            Ok(compiled) => Ok(ProgramSourceFile {
                table_schemas: compiled.schemas,
                tx_types: compiled.tx_types,
                contract_metadata: None,
            }),
            Err(errors) => {
                let mut msg = String::new();
                for err in &errors {
                    if !msg.is_empty() {
                        msg.push_str("\n\n");
                    }
                    msg.push_str(&format!("{}", err.display_with_source(&source)));
                }
                bail!(msg)
            }
        }
    } else {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let parsed: ProgramSourceFile = serde_json::from_str(&content)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        Ok(parsed)
    }
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
    let sources = load_program_sources(path)?;
    let artifact = register_program(&sources.table_schemas, &sources.tx_types)?;

    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    if ext != "tab" {
        let provided = sources.contract_metadata.as_ref().ok_or_else(|| {
            anyhow::anyhow!(
                "compiled program JSON is missing contract_metadata; regenerate with the current driver"
            )
        })?;
        artifact
            .compatibility_policy()
            .validate(provided)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    }

    Ok(artifact)
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
        assert!(err.to_string().contains("missing contract_metadata"));
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
        assert!(err.to_string().contains("profile hash mismatch"));
        let _ = std::fs::remove_file(path);
    }
}
