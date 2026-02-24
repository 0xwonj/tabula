//! Program registration: schema validation and IR program construction.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::bail;

use tabula_contract::{
    CONTRACT_SCHEMA_VERSION_V1, ContractCompatibilityPolicy, ContractMetadataEnvelope,
    STATEMENT_BINDING_VERSION_V1, apply_batch_binding_registry_v1,
};
use tabula_core::{ColId, TableId, TableSchema};
use tabula_ir::{Program, TxTypeDef};

use crate::ProgramSourceFile;
use crate::error::{DriverError, DriverResult};
use crate::profile::compute_profile_hash;

/// Contract metadata validation policy for program sources.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetadataPolicy {
    /// Metadata is optional (e.g., source program that will be freshly registered).
    Optional,
    /// Metadata is required and must validate (e.g., precompiled artifact input).
    Required,
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

/// Register program sources using explicit metadata policy.
pub fn register_program_sources(
    sources: &ProgramSourceFile,
    metadata_policy: MetadataPolicy,
) -> DriverResult<RegisteredProgram> {
    let artifact = register_program(&sources.table_schemas, &sources.tx_types)
        .map_err(DriverError::InvalidProgram)?;

    match (metadata_policy, sources.contract_metadata.as_ref()) {
        (MetadataPolicy::Required, None) => Err(DriverError::MissingContractMetadata),
        (_, Some(provided)) => artifact
            .compatibility_policy()
            .validate(provided)
            .map_err(DriverError::ContractMetadataMismatch)
            .map(|_| artifact),
        (_, None) => Ok(artifact),
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
        .map_err(|e| anyhow::anyhow!(e))?;

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
