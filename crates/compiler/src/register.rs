//! Program registration: schema validation and IR program construction.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::bail;

use tabula_artifact::{Artifact, ColumnProofPlan, PrecompileDescriptor, SchemeDescriptor};
use tabula_contract::{
    BINDING_VERSION_V1, CONTRACT_SCHEMA_VERSION_V1, ContractMetadataEnvelope,
    STATEMENT_SCHEMA_VERSION_V1, VERIFIER_PROFILE_VERSION_V1, binding_registry_v1,
};
use tabula_core::{ColId, SchemeId, TableId, TableSchema};
use tabula_ir::{PrecompileId, Program, PropertyRequirement, TxTypeDef};

use crate::error::{CompilerError, CompilerResult};
use crate::profile::{compute_profile_hash, compute_semantic_hash_stub};
use crate::program::SealedProgram;
use crate::sources::{ColumnSchemeSelection, ProgramDefinition};

const DEFAULT_COLUMN_SCHEME_ID: SchemeId = SchemeId::SSMC;

/// Source-registration catalog for custom scheme descriptors.
pub type SchemeDescriptorCatalog = BTreeMap<SchemeId, SchemeDescriptor>;
/// Source-registration catalog for custom precompile descriptors.
pub type PrecompileDescriptorCatalog = BTreeMap<PrecompileId, PrecompileDescriptor>;

/// Compiler-owned semantic catalogs used during sealing.
#[derive(Debug, Clone, Default)]
pub struct CompilerCatalogs {
    /// Scheme descriptors available to source-level scheme selection.
    pub schemes: SchemeDescriptorCatalog,
    /// Precompile descriptors available to source-level precompile references.
    pub precompiles: PrecompileDescriptorCatalog,
}

/// Register source-derived program definitions.
pub fn register_program_definition(
    definition: &ProgramDefinition,
) -> CompilerResult<SealedProgram> {
    register_program_definition_with_catalogs(definition, &CompilerCatalogs::default())
}

/// Register source-derived program definitions with custom semantic catalogs.
pub fn register_program_definition_with_catalogs(
    definition: &ProgramDefinition,
    catalogs: &CompilerCatalogs,
) -> CompilerResult<SealedProgram> {
    let column_proof_plan = derive_column_proof_plan(
        &definition.table_schemas,
        &definition.column_schemes,
        &catalogs.schemes,
    )
    .map_err(|err| CompilerError::InvalidProgram(anyhow::Error::msg(err)))?;
    let precompile_manifest =
        derive_precompile_manifest(&definition.tx_types, &catalogs.precompiles)
            .map_err(|err| CompilerError::InvalidProgram(anyhow::Error::msg(err)))?;
    register_program_with_plan(
        &definition.table_schemas,
        &definition.tx_types,
        column_proof_plan,
        precompile_manifest,
    )
    .map_err(CompilerError::InvalidProgram)
}

/// Register a sealed artifact and validate its contract metadata.
pub fn register_artifact(artifact: &Artifact) -> CompilerResult<SealedProgram> {
    let compiled = register_program_with_plan(
        &artifact.table_schemas,
        &artifact.tx_types,
        artifact.column_proof_plan.clone(),
        artifact.precompile_manifest.clone(),
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
) -> anyhow::Result<SealedProgram> {
    register_program_with_plan(
        schemas,
        tx_types,
        derive_column_proof_plan(schemas, &[], &SchemeDescriptorCatalog::new())
            .map_err(anyhow::Error::msg)?,
        derive_precompile_manifest(tx_types, &PrecompileDescriptorCatalog::new())
            .map_err(anyhow::Error::msg)?,
    )
}

fn register_program_with_plan(
    schemas: &[TableSchema],
    tx_types: &[TxTypeDef],
    column_proof_plan: Vec<ColumnProofPlan>,
    precompile_manifest: Vec<PrecompileDescriptor>,
) -> anyhow::Result<SealedProgram> {
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
    let required_property_requirements = derive_required_property_requirements(&program);
    let semantic_hash_stub = compute_semantic_hash_stub(
        &precompile_manifest,
        &required_property_requirements,
        &column_proof_plan,
    )?;
    let metadata_envelope = ContractMetadataEnvelope {
        profile_hash,
        contract_schema_version: CONTRACT_SCHEMA_VERSION_V1,
        binding_version: BINDING_VERSION_V1,
        statement_schema_version: STATEMENT_SCHEMA_VERSION_V1,
        verifier_profile_version: VERIFIER_PROFILE_VERSION_V1,
        semantic_hash_stub: Some(semantic_hash_stub),
    };

    SealedProgram::new(
        program,
        schemas.to_vec(),
        tx_types.to_vec(),
        precompile_manifest,
        required_property_requirements,
        column_proof_plan,
        metadata_envelope,
    )
    .map_err(anyhow::Error::msg)
}

fn validate_artifact_shape(artifact: &Artifact, compiled: &SealedProgram) -> CompilerResult<()> {
    if artifact.precompile_manifest != compiled.precompile_manifest() {
        return Err(CompilerError::ArtifactMismatch {
            detail: "precompile_manifest does not match compiler-derived capabilities".to_string(),
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

fn derive_precompile_manifest(
    tx_types: &[TxTypeDef],
    catalog: &PrecompileDescriptorCatalog,
) -> Result<Vec<PrecompileDescriptor>, String> {
    let mut referenced = BTreeSet::new();
    for tx in tx_types {
        for instr in &tx.body {
            if let tabula_ir::Instruction::Precompile { id, .. } = instr {
                referenced.insert(*id);
            }
        }
    }

    referenced
        .into_iter()
        .map(|id| {
            catalog.get(&id).cloned().ok_or_else(|| {
                format!(
                    "program references precompile 0x{:04x} but no descriptor is registered",
                    id.0
                )
            })
        })
        .collect()
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
    scheme_catalog: &SchemeDescriptorCatalog,
) -> Result<Vec<ColumnProofPlan>, String> {
    let default_descriptor =
        resolve_descriptor_for_scheme(DEFAULT_COLUMN_SCHEME_ID, scheme_catalog)?;
    let mut columns: Vec<_> = schemas
        .iter()
        .flat_map(|schema| {
            schema.columns.iter().map({
                let default_descriptor = default_descriptor.clone();
                move |column| ColumnProofPlan {
                    table_id: schema.id,
                    col_id: column.id,
                    scheme_id: DEFAULT_COLUMN_SCHEME_ID,
                    scheme_descriptor: default_descriptor.clone(),
                    receives_commitment: true,
                }
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
        columns[idx].scheme_descriptor =
            resolve_descriptor_for_scheme(override_entry.scheme_id, scheme_catalog)?;
    }
    columns.sort_by_key(|plan| (plan.table_id, plan.col_id));
    Ok(columns)
}

fn resolve_descriptor_for_scheme(
    scheme_id: SchemeId,
    scheme_catalog: &SchemeDescriptorCatalog,
) -> Result<SchemeDescriptor, String> {
    match scheme_id {
        SchemeId::SSMC => Ok(SchemeDescriptor::builtin_ssmc()),
        SchemeId::SMT => Ok(SchemeDescriptor::builtin_smt()),
        other => {
            let Some(descriptor) = scheme_catalog.get(&other) else {
                return Err(format!(
                    "source-derived proof planning does not know a descriptor for custom scheme id {}",
                    other.0
                ));
            };
            if descriptor.scheme_id != other {
                return Err(format!(
                    "custom scheme descriptor catalog mismatch: key {} maps to descriptor scheme id {}",
                    other.0, descriptor.scheme_id.0
                ));
            }
            Ok(descriptor.clone())
        }
    }
}
