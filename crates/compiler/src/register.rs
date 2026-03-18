//! Program registration: schema validation and IR program construction.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::bail;

use tabula_artifact::{ColumnProofPlan, ProgramArtifact};
use tabula_contract::{
    BINDING_VERSION_V1, CONTRACT_SCHEMA_VERSION_V1, ContractMetadataEnvelope,
    STATEMENT_SCHEMA_VERSION_V1, VERIFIER_PROFILE_VERSION_V1, binding_registry_v1,
};
use tabula_core::{ColId, SchemeId, TableId, TableSchema};
use tabula_ir::{PrecompileId, Program, PropertyRequirement, TxTypeDef};

use crate::error::{CompilerError, CompilerResult};
use crate::profile::compute_profile_hash;
use crate::program::CompiledProgram;
use crate::sources::{ColumnSchemeSelection, ProgramDefinition};

const DEFAULT_COLUMN_SCHEME_ID: SchemeId = SchemeId::SSMC;

/// Register source-derived program definitions.
pub fn register_program_definition(
    definition: &ProgramDefinition,
) -> CompilerResult<CompiledProgram> {
    let column_proof_plan =
        derive_column_proof_plan(&definition.table_schemas, &definition.column_schemes)
            .map_err(|err| CompilerError::InvalidProgram(anyhow::Error::msg(err)))?;
    register_program_with_plan(
        &definition.table_schemas,
        &definition.tx_types,
        column_proof_plan,
    )
    .map_err(CompilerError::InvalidProgram)
}

/// Register a sealed program artifact and validate its contract metadata.
pub fn register_program_artifact(artifact: &ProgramArtifact) -> CompilerResult<CompiledProgram> {
    let compiled = register_program_with_plan(
        &artifact.table_schemas,
        &artifact.tx_types,
        artifact.column_proof_plan.clone(),
    )
    .map_err(CompilerError::InvalidProgram)?;
    compiled
        .compatibility_policy()
        .validate(&artifact.contract_metadata)
        .map_err(CompilerError::ContractMetadataMismatch)?;
    validate_artifact_shape(artifact, &compiled)?;
    Ok(compiled)
}

/// Register schemas and tx types into a semantic artifact.
pub fn register_program(
    schemas: &[TableSchema],
    tx_types: &[TxTypeDef],
) -> anyhow::Result<CompiledProgram> {
    register_program_with_plan(
        schemas,
        tx_types,
        derive_column_proof_plan(schemas, &[]).map_err(anyhow::Error::msg)?,
    )
}

fn register_program_with_plan(
    schemas: &[TableSchema],
    tx_types: &[TxTypeDef],
    column_proof_plan: Vec<ColumnProofPlan>,
) -> anyhow::Result<CompiledProgram> {
    validate_schema_coverage(schemas, tx_types)?;

    let mut program = Program::new();
    for schema in schemas {
        program.add_schema(schema.clone());
    }
    for def in tx_types {
        program.register(def.clone())?;
    }

    // Gate: binding registry must remain complete.
    let registry = binding_registry_v1();
    registry
        .validate_completeness()
        .map_err(|e| anyhow::anyhow!(e))?;

    let profile_hash = compute_profile_hash(schemas, tx_types)?;
    let required_precompile_ids = derive_required_precompile_ids(&program);
    let required_property_requirements = derive_required_property_requirements(&program);
    let metadata_envelope = ContractMetadataEnvelope {
        profile_hash,
        contract_schema_version: CONTRACT_SCHEMA_VERSION_V1,
        binding_version: BINDING_VERSION_V1,
        statement_schema_version: STATEMENT_SCHEMA_VERSION_V1,
        verifier_profile_version: VERIFIER_PROFILE_VERSION_V1,
        semantic_hash_stub: None,
    };

    CompiledProgram::new(
        program,
        schemas.to_vec(),
        tx_types.to_vec(),
        required_precompile_ids,
        required_property_requirements,
        column_proof_plan,
        metadata_envelope,
    )
    .map_err(anyhow::Error::msg)
}

fn validate_artifact_shape(
    artifact: &ProgramArtifact,
    compiled: &CompiledProgram,
) -> CompilerResult<()> {
    if artifact.required_precompile_ids != compiled.required_precompile_ids() {
        return Err(CompilerError::ArtifactMismatch {
            detail: "required_precompile_ids do not match compiler-derived capabilities"
                .to_string(),
        });
    }
    if artifact.required_property_requirements != compiled.required_property_requirements() {
        return Err(CompilerError::ArtifactMismatch {
            detail: "required_property_requirements do not match compiler-derived capabilities"
                .to_string(),
        });
    }
    if artifact.column_proof_plan != compiled.column_proof_plan() {
        return Err(CompilerError::ArtifactMismatch {
            detail: "column_proof_plan does not match compiler-derived proof plan".to_string(),
        });
    }
    Ok(())
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
                    ensure_table_col_exists(&columns_by_table, tx, instr_idx, *table, *col)?;
                }
                tabula_ir::Instruction::Lookup {
                    static_table, col, ..
                } => {
                    ensure_table_col_exists(&columns_by_table, tx, instr_idx, *static_table, *col)?;
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

fn derive_required_precompile_ids(program: &Program) -> Vec<PrecompileId> {
    program.referenced_precompile_ids().into_iter().collect()
}

fn derive_required_property_requirements(program: &Program) -> Vec<PropertyRequirement> {
    program
        .referenced_property_requirements()
        .into_iter()
        .collect()
}

fn derive_column_proof_plan(
    schemas: &[TableSchema],
    overrides: &[ColumnSchemeSelection],
) -> Result<Vec<ColumnProofPlan>, String> {
    let mut columns: Vec<_> = schemas
        .iter()
        .flat_map(|schema| {
            schema.columns.iter().map(move |column| ColumnProofPlan {
                table_id: schema.id,
                col_id: column.id,
                scheme_id: DEFAULT_COLUMN_SCHEME_ID,
                receives_commitment: true,
            })
        })
        .collect();
    let mut plan_index: BTreeMap<(TableId, ColId), usize> = columns
        .iter()
        .enumerate()
        .map(|(idx, plan)| ((plan.table_id, plan.col_id), idx))
        .collect();
    let mut seen_overrides = BTreeSet::new();
    for override_entry in overrides {
        let key = (override_entry.table_id, override_entry.col_id);
        if !seen_overrides.insert(key) {
            return Err(format!(
                "column scheme selection contains duplicate entry for table {} col {}",
                override_entry.table_id.0, override_entry.col_id.0
            ));
        }
        let Some(idx) = plan_index.remove(&key) else {
            return Err(format!(
                "column scheme selection references unknown table {} col {}",
                override_entry.table_id.0, override_entry.col_id.0
            ));
        };
        columns[idx].scheme_id = override_entry.scheme_id;
    }
    columns.sort_by_key(|plan| (plan.table_id, plan.col_id));
    Ok(columns)
}
