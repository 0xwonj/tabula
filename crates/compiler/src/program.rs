//! Canonical in-memory program produced by the compiler.

use tabula_artifact::{Artifact, PrecompileDescriptor};
use tabula_contract::{ContractCompatibilityPolicy, ContractMetadataEnvelope};
use tabula_core::{ColId, ColumnProfileId, TableId, TableSchema};
use tabula_ir::{Program, PropertyRequirement, TxTypeDef};
use tabula_profile::{ProfileCatalog, ResolvedColumnProfileRef};

/// In-memory semantic artifact produced by the compiler/registration phase.
#[derive(Debug, Clone)]
pub struct SealedProgram {
    /// Registered IR program.
    program: Program,
    /// Canonical table schemas consumed during registration.
    table_schemas: Vec<TableSchema>,
    /// Canonical transaction definitions consumed during registration.
    tx_types: Vec<TxTypeDef>,
    /// Canonical semantic profile catalog sealed for this program.
    profile_catalog: ProfileCatalog,
    /// Capability manifest: precompiles required by the program.
    precompile_manifest: Vec<PrecompileDescriptor>,
    /// Capability manifest: exact structural property requirements required by the program.
    required_property_requirements: Vec<PropertyRequirement>,
    /// Canonical metadata envelope for proof compatibility checks.
    metadata_envelope: ContractMetadataEnvelope,
}

impl SealedProgram {
    /// Create a compiler-owned semantic artifact after invariant checks.
    pub(crate) fn new(
        program: Program,
        table_schemas: Vec<TableSchema>,
        tx_types: Vec<TxTypeDef>,
        profile_catalog: ProfileCatalog,
        precompile_manifest: Vec<PrecompileDescriptor>,
        required_property_requirements: Vec<PropertyRequirement>,
        metadata_envelope: ContractMetadataEnvelope,
    ) -> Result<Self, String> {
        let compiled = Self {
            program,
            table_schemas,
            tx_types,
            profile_catalog,
            precompile_manifest,
            required_property_requirements,
            metadata_envelope,
        };
        compiled.validate_precompile_manifest()?;
        compiled.validate_column_profiles()?;
        Ok(compiled)
    }

    /// Registered IR program.
    pub fn program(&self) -> &Program {
        &self.program
    }

    /// Canonical table schemas consumed during registration.
    pub fn table_schemas(&self) -> &[TableSchema] {
        &self.table_schemas
    }

    /// Canonical transaction definitions consumed during registration.
    pub fn tx_types(&self) -> &[TxTypeDef] {
        &self.tx_types
    }

    /// Canonical semantic profile catalog sealed for this program.
    pub fn profile_catalog(&self) -> &ProfileCatalog {
        &self.profile_catalog
    }

    /// Capability manifest: precompiles required by the program.
    pub fn precompile_manifest(&self) -> &[PrecompileDescriptor] {
        &self.precompile_manifest
    }

    /// Capability manifest: exact structural property requirements required by the program.
    pub fn required_property_requirements(&self) -> &[PropertyRequirement] {
        &self.required_property_requirements
    }

    /// Canonical metadata envelope for proof compatibility checks.
    pub fn metadata_envelope(&self) -> &ContractMetadataEnvelope {
        &self.metadata_envelope
    }

    /// Build a strict compatibility policy pinned to this program's metadata.
    pub fn compatibility_policy(&self) -> ContractCompatibilityPolicy {
        ContractCompatibilityPolicy {
            expected_profile_hash: self.metadata_envelope.profile_hash,
            expected_contract_schema_version: self.metadata_envelope.contract_schema_version,
            expected_binding_version: self.metadata_envelope.binding_version,
            expected_statement_schema_version: self.metadata_envelope.statement_schema_version,
            expected_verifier_profile_version: self.metadata_envelope.verifier_profile_version,
            expected_semantic_hash_stub: self.metadata_envelope.semantic_hash_stub,
        }
    }

    /// Validate that the sealed precompile manifest is sorted, unique, and
    /// exactly matches the precompile IDs referenced from the IR body.
    pub fn validate_precompile_manifest(&self) -> Result<(), String> {
        let manifest_ids: Vec<_> = self
            .precompile_manifest
            .iter()
            .map(|descriptor| descriptor.precompile_id)
            .collect();

        let mut sorted_unique_manifest_ids = manifest_ids.clone();
        sorted_unique_manifest_ids.sort();
        sorted_unique_manifest_ids.dedup();
        if manifest_ids != sorted_unique_manifest_ids {
            return Err(
                "precompile manifest must be sorted by precompile_id and contain no duplicates"
                    .to_string(),
            );
        }

        let referenced_ids: Vec<_> = derive_referenced_precompile_ids(&self.program)
            .into_iter()
            .collect();
        if manifest_ids != referenced_ids {
            return Err(format!(
                "precompile manifest ids {manifest_ids:?} do not match IR-referenced precompile ids {referenced_ids:?}",
            ));
        }

        Ok(())
    }

    /// Validate that every sealed column resolves through the canonical profile catalog.
    pub fn validate_column_profiles(&self) -> Result<(), String> {
        for schema in &self.table_schemas {
            for column in &schema.columns {
                self.resolve_column_profile(schema.id, column.id)?;
            }
        }
        Ok(())
    }

    /// Resolve one sealed column `(table_id, col_id)` into its canonical profile-backed view.
    pub fn resolve_column_profile(
        &self,
        table_id: TableId,
        col_id: ColId,
    ) -> Result<ResolvedColumnProfileRef<'_>, String> {
        let column_profile_id = self
            .table_schemas
            .iter()
            .find(|schema| schema.id == table_id)
            .and_then(|schema| schema.columns.iter().find(|column| column.id == col_id))
            .map(|column| column.column_profile_id)
            .ok_or_else(|| {
                format!(
                    "table {} col {} is missing from sealed schema",
                    table_id.0, col_id.0
                )
            })?;
        self.resolve_column_profile_by_id(column_profile_id)
    }

    /// Resolve one sealed column profile id into its canonical profile-backed view.
    pub fn resolve_column_profile_by_id(
        &self,
        column_profile_id: ColumnProfileId,
    ) -> Result<ResolvedColumnProfileRef<'_>, String> {
        self.profile_catalog
            .resolve_column_profile(column_profile_id)
            .map_err(|err| err.to_string())
    }

    /// Clone into a sealed portable artifact.
    pub fn as_artifact(&self) -> Artifact {
        Artifact {
            table_schemas: self.table_schemas.clone(),
            profile_catalog: self.profile_catalog.clone(),
            tx_types: self.tx_types.clone(),
            precompile_manifest: self.precompile_manifest.clone(),
            required_property_requirements: self.required_property_requirements.clone(),
            contract_metadata: self.metadata_envelope.clone(),
        }
    }

    /// Convert into a sealed portable artifact.
    pub fn into_artifact(self) -> Artifact {
        Artifact {
            table_schemas: self.table_schemas,
            profile_catalog: self.profile_catalog,
            tx_types: self.tx_types,
            precompile_manifest: self.precompile_manifest,
            required_property_requirements: self.required_property_requirements,
            contract_metadata: self.metadata_envelope,
        }
    }
}

fn derive_referenced_precompile_ids(program: &Program) -> Vec<tabula_ir::PrecompileId> {
    program.referenced_precompile_ids().into_iter().collect()
}
