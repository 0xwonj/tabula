//! Program registration: schema validation and IR program construction.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::bail;

use tabula_artifact::{Artifact, PrecompileDescriptor};
use tabula_contract::{
    BINDING_VERSION_V1, CONTRACT_SCHEMA_VERSION_V1, ContractMetadataEnvelope,
    STATEMENT_SCHEMA_VERSION_V1, VERIFIER_PROFILE_VERSION_V1, binding_registry_v1,
};
use tabula_core::{ColId, ColumnDef, ColumnProfileId, SchemeId, TableId, TableSchema};
use tabula_ir::{
    GENERIC_EXECUTION_VALUE_WIDTH, PrecompileId, Program, PropertyRequirement, TxTypeDef,
};
use tabula_profile::{
    ColumnProfile, CommitmentRole, ProfileCatalog, SchemeProfile, SemanticRegistry, TypeDescriptor,
    builtin_semantic_registry,
};

use crate::error::{CompilerCatalogError, CompilerError, CompilerResult};
use crate::profile::{compute_profile_hash, compute_semantic_hash_stub};
use crate::program::SealedProgram;
use crate::sources::{ColumnSchemeSelection, ProgramDefinition, SourceTableSchema};

const DEFAULT_COLUMN_SCHEME_ID: SchemeId = SchemeId::SSMC;

/// Source-registration catalog for custom precompile descriptors.
pub type PrecompileDescriptorCatalog = BTreeMap<PrecompileId, PrecompileDescriptor>;

/// Compiler-owned semantic catalogs used during sealing.
#[derive(Debug, Clone)]
pub struct CompilerCatalogs {
    /// Semantic registry used for source authoring name resolution and default profile lookup.
    semantics: SemanticRegistry,
    /// Precompile descriptors available to source-level precompile references.
    precompiles: PrecompileDescriptorCatalog,
}

impl Default for CompilerCatalogs {
    fn default() -> Self {
        Self {
            semantics: builtin_semantic_registry()
                .expect("built-in semantic registry must remain valid"),
            precompiles: PrecompileDescriptorCatalog::new(),
        }
    }
}

impl CompilerCatalogs {
    /// Build compiler catalogs seeded with the built-in semantic registry.
    pub fn standard() -> Self {
        Self::default()
    }

    /// Build compiler catalogs without any seeded semantic definitions.
    pub fn empty() -> Self {
        Self {
            semantics: SemanticRegistry::new(),
            precompiles: PrecompileDescriptorCatalog::new(),
        }
    }

    /// Borrow the semantic registry used during sealing.
    pub fn semantics(&self) -> &SemanticRegistry {
        &self.semantics
    }

    /// Borrow the registered precompile descriptors.
    pub fn precompile_descriptors(&self) -> &PrecompileDescriptorCatalog {
        &self.precompiles
    }

    /// Replace the semantic registry used during sealing.
    pub fn with_semantic_registry(
        mut self,
        semantics: SemanticRegistry,
    ) -> Result<Self, CompilerCatalogError> {
        semantics
            .validate()
            .map_err(CompilerCatalogError::InvalidSemanticRegistry)?;
        validate_precompile_descriptor_catalog(&self.precompiles, semantics.catalog())
            .map_err(|detail| CompilerCatalogError::InvalidPrecompileDescriptor { detail })?;
        self.semantics = semantics;
        Ok(self)
    }

    /// Register one precompile descriptor available to source-level precompile references.
    pub fn with_precompile_descriptor(
        mut self,
        descriptor: PrecompileDescriptor,
    ) -> Result<Self, CompilerCatalogError> {
        self.insert_precompile_descriptor(descriptor)?;
        Ok(self)
    }

    /// Insert one precompile descriptor available during sealing.
    pub fn insert_precompile_descriptor(
        &mut self,
        descriptor: PrecompileDescriptor,
    ) -> Result<(), CompilerCatalogError> {
        if self.precompiles.contains_key(&descriptor.precompile_id) {
            return Err(CompilerCatalogError::DuplicatePrecompileDescriptor {
                precompile_id: descriptor.precompile_id,
            });
        }
        validate_precompile_descriptor(&descriptor, self.semantics.catalog())
            .map_err(|detail| CompilerCatalogError::InvalidPrecompileDescriptor { detail })?;
        self.precompiles
            .insert(descriptor.precompile_id, descriptor);
        Ok(())
    }
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
    let (sealed_schemas, mut profile_catalog) = seal_source_schemas(
        &definition.table_schemas,
        &definition.column_schemes,
        catalogs.semantics(),
    )
    .map_err(|err| CompilerError::InvalidProgram(anyhow::Error::msg(err)))?;
    let precompile_manifest =
        derive_precompile_manifest(&definition.tx_types, catalogs.precompile_descriptors())
            .map_err(|err| CompilerError::InvalidProgram(anyhow::Error::msg(err)))?;
    extend_profile_catalog_with_precompile_descriptors(
        &mut profile_catalog,
        &precompile_manifest,
        catalogs.semantics().catalog(),
    )
    .map_err(|err| CompilerError::InvalidProgram(anyhow::Error::msg(err)))?;
    register_program_with_plan(
        &sealed_schemas,
        &definition.tx_types,
        profile_catalog,
        precompile_manifest,
    )
    .map_err(CompilerError::InvalidProgram)
}

/// Register a sealed artifact and validate its contract metadata.
pub fn register_artifact(artifact: &Artifact) -> CompilerResult<SealedProgram> {
    validate_precompile_descriptors(&artifact.precompile_manifest, &artifact.profile_catalog)
        .map_err(|detail| CompilerError::ArtifactMismatch { detail })?;
    let compiled = register_program_with_plan(
        &artifact.table_schemas,
        &artifact.tx_types,
        artifact.profile_catalog.clone(),
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

fn validate_precompile_descriptor_catalog(
    descriptors: &PrecompileDescriptorCatalog,
    catalog: &ProfileCatalog,
) -> Result<(), String> {
    for descriptor in descriptors.values() {
        validate_precompile_descriptor(descriptor, catalog)?;
    }
    Ok(())
}

fn validate_precompile_descriptors(
    descriptors: &[PrecompileDescriptor],
    catalog: &ProfileCatalog,
) -> Result<(), String> {
    let mut seen = BTreeSet::new();
    for descriptor in descriptors {
        if !seen.insert(descriptor.precompile_id) {
            return Err(format!(
                "duplicate precompile descriptor registration for id 0x{:04x}",
                descriptor.precompile_id.0,
            ));
        }
        validate_precompile_descriptor(descriptor, catalog)?;
    }
    Ok(())
}

fn validate_precompile_descriptor(
    descriptor: &PrecompileDescriptor,
    catalog: &ProfileCatalog,
) -> Result<(), String> {
    for (kind, values) in [
        ("input", descriptor.signature.inputs.as_slice()),
        ("output", descriptor.signature.outputs.as_slice()),
    ] {
        for (idx, value_profile) in values.iter().enumerate() {
            catalog
                .type_descriptor(value_profile.type_id)
                .map_err(|err| {
                    format!(
                        "precompile 0x{:04x} {kind} {} references unknown type id {}: {err}",
                        descriptor.precompile_id.0, idx, value_profile.type_id.0,
                    )
                })?;
            let encoding = catalog
                .encoding_profile(value_profile.encoding_profile_id)
                .map_err(|err| {
                    format!(
                        "precompile 0x{:04x} {kind} {} references unknown encoding profile {}: {err}",
                        descriptor.precompile_id.0,
                        idx,
                        value_profile.encoding_profile_id.0,
                    )
                })?;
            if encoding.type_id != value_profile.type_id {
                return Err(format!(
                    "precompile 0x{:04x} {kind} {} declares type {} with incompatible encoding profile {} (encoding type {})",
                    descriptor.precompile_id.0,
                    idx,
                    value_profile.type_id.0,
                    value_profile.encoding_profile_id.0,
                    encoding.type_id.0,
                ));
            }
            if usize::from(encoding.width) > GENERIC_EXECUTION_VALUE_WIDTH {
                return Err(format!(
                    "precompile 0x{:04x} {kind} {} uses execution width {} but the generic execution lane only supports width {}",
                    descriptor.precompile_id.0, idx, encoding.width, GENERIC_EXECUTION_VALUE_WIDTH,
                ));
            }
        }
    }
    Ok(())
}

fn register_program_with_plan(
    schemas: &[TableSchema],
    tx_types: &[TxTypeDef],
    profile_catalog: ProfileCatalog,
    precompile_manifest: Vec<PrecompileDescriptor>,
) -> anyhow::Result<SealedProgram> {
    validate_schema_coverage(schemas, tx_types)?;

    let mut program = Program::with_profile_catalog_and_precompiles(
        profile_catalog.clone(),
        precompile_manifest
            .iter()
            .map(|descriptor| (descriptor.precompile_id, descriptor.signature.clone()))
            .collect(),
    );
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

    let profile_hash = compute_profile_hash(schemas, tx_types, &profile_catalog)?;
    let required_property_requirements = derive_required_property_requirements(&program);
    let semantic_hash_stub = compute_semantic_hash_stub(
        &precompile_manifest,
        &required_property_requirements,
        &profile_catalog,
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
        profile_catalog,
        precompile_manifest,
        required_property_requirements,
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
    if artifact.profile_catalog != *compiled.profile_catalog() {
        return Err(CompilerError::ArtifactMismatch {
            detail: "profile_catalog does not match compiler-derived sealed catalog".to_string(),
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

fn extend_profile_catalog_with_precompile_descriptors(
    profile_catalog: &mut ProfileCatalog,
    descriptors: &[PrecompileDescriptor],
    semantic_catalog: &ProfileCatalog,
) -> Result<(), String> {
    for descriptor in descriptors {
        for profile in descriptor
            .signature
            .inputs
            .iter()
            .chain(descriptor.signature.outputs.iter())
        {
            if profile_catalog.type_descriptor(profile.type_id).is_err() {
                let descriptor =
                    semantic_catalog
                        .type_descriptor(profile.type_id)
                        .map_err(|_| {
                            format!(
                                "precompile 0x{:04x} references unknown type id {}",
                                descriptor.precompile_id.0, profile.type_id.0
                            )
                        })?;
                profile_catalog
                    .register_type(descriptor.clone())
                    .map_err(|err| err.to_string())?;
            }
            if profile_catalog
                .encoding_profile(profile.encoding_profile_id)
                .is_err()
            {
                let encoding = semantic_catalog
                    .encoding_profile(profile.encoding_profile_id)
                    .map_err(|_| {
                        format!(
                            "precompile 0x{:04x} references unknown encoding profile id {}",
                            descriptor.precompile_id.0, profile.encoding_profile_id.0
                        )
                    })?;
                profile_catalog
                    .register_encoding(encoding.clone())
                    .map_err(|err| err.to_string())?;
            }
        }
    }
    Ok(())
}

fn derive_required_property_requirements(program: &Program) -> Vec<PropertyRequirement> {
    program
        .referenced_property_requirements()
        .into_iter()
        .collect()
}

fn seal_source_schemas(
    schemas: &[SourceTableSchema],
    overrides: &[ColumnSchemeSelection],
    registry: &SemanticRegistry,
) -> Result<(Vec<TableSchema>, ProfileCatalog), String> {
    let mut override_by_key = BTreeMap::new();
    for override_entry in overrides {
        let key = (override_entry.table_id, override_entry.col_id);
        if override_by_key
            .insert(key, override_entry.scheme_id)
            .is_some()
        {
            return Err(format!(
                "column scheme selection contains duplicate entry for table {} col {}",
                override_entry.table_id.0, override_entry.col_id.0
            ));
        }
    }

    let registry_catalog = registry.catalog();
    let mut sealed_catalog = ProfileCatalog::new();
    let mut sealed_schemas = Vec::with_capacity(schemas.len());
    let mut next_column_profile_id = 0u32;

    for schema in schemas {
        let mut sealed_columns = Vec::with_capacity(schema.columns.len());
        for column in &schema.columns {
            let type_descriptor = registry_catalog
                .type_descriptor(column.type_id)
                .cloned()
                .map_err(|_| {
                    format!(
                        "schema column table {} col {} references unknown type id {}",
                        schema.id.0, column.id.0, column.type_id.0
                    )
                })?;
            let encoding_profile_id = registry
                .resolve_default_encoding(column.type_id)
                .map_err(|err| err.to_string())?;
            let encoding_profile = registry_catalog
                .encoding_profile(encoding_profile_id)
                .cloned()
                .map_err(|_| {
                    format!(
                        "type id {} resolved default encoding {} that is missing from the registry catalog",
                        column.type_id.0, encoding_profile_id.0
                    )
                })?;
            let scheme_family_id = override_by_key
                .remove(&(schema.id, column.id))
                .unwrap_or(DEFAULT_COLUMN_SCHEME_ID);
            let scheme_profile_id = registry
                .resolve_default_scheme_profile(scheme_family_id, encoding_profile_id)
                .map_err(|err| err.to_string())?;
            let scheme_profile = registry_catalog
                .scheme_profile(scheme_profile_id)
                .cloned()
                .map_err(|_| {
                    format!(
                        "scheme family {} + encoding {} resolved missing scheme profile {}",
                        scheme_family_id.0, encoding_profile_id.0, scheme_profile_id.0
                    )
                })?;

            register_reused_definitions(
                &mut sealed_catalog,
                &type_descriptor,
                &encoding_profile,
                &scheme_profile,
            )
            .map_err(|err| err.to_string())?;

            let column_profile = ColumnProfile::new(
                ColumnProfileId(next_column_profile_id),
                format!("{}.{}", schema.name, column.name),
                None,
                &type_descriptor,
                &encoding_profile,
                &scheme_profile,
                CommitmentRole::IncludedInRoot,
            )
            .map_err(|err| err.to_string())?;
            next_column_profile_id += 1;
            let column_profile_id = column_profile.column_profile_id;
            sealed_catalog
                .register_column(column_profile)
                .map_err(|err| err.to_string())?;
            sealed_columns.push(ColumnDef {
                id: column.id,
                name: column.name.clone(),
                column_profile_id,
            });
        }
        sealed_schemas.push(TableSchema {
            id: schema.id,
            name: schema.name.clone(),
            columns: sealed_columns,
        });
    }

    if let Some(((table_id, col_id), _)) = override_by_key.first_key_value() {
        return Err(format!(
            "column scheme selection references unknown table {} col {}",
            table_id.0, col_id.0
        ));
    }

    sealed_catalog.validate().map_err(|err| err.to_string())?;
    Ok((sealed_schemas, sealed_catalog))
}

fn register_reused_definitions(
    catalog: &mut ProfileCatalog,
    type_descriptor: &TypeDescriptor,
    encoding_profile: &tabula_profile::EncodingProfile,
    scheme_profile: &SchemeProfile,
) -> Result<(), tabula_profile::ProfileError> {
    if !catalog
        .types
        .iter()
        .any(|descriptor| descriptor.type_id == type_descriptor.type_id)
    {
        catalog.register_type(type_descriptor.clone())?;
    }
    if !catalog
        .encodings
        .iter()
        .any(|profile| profile.encoding_profile_id == encoding_profile.encoding_profile_id)
    {
        catalog.register_encoding(encoding_profile.clone())?;
    }
    if !catalog
        .schemes
        .iter()
        .any(|profile| profile.scheme_profile_id == scheme_profile.scheme_profile_id)
    {
        catalog.register_scheme(scheme_profile.clone())?;
    }
    Ok(())
}
