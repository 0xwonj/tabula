use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use sha2::Digest as _;

use tabula_contract::{
    ContractCompatibilityPolicy, ContractMetadataEnvelope, ProgramBinding, StaticTableArtifact,
    TupleEncodingDefaults,
};
use tabula_core::SchemeId;
use tabula_core::TableSchema;
use tabula_ir as ir;
use tabula_profile::ProfileCatalog;

use crate::error::CompilerError;

/// Compiler-owned scheme sidecar for one state field.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub struct StateFieldSchemeBinding {
    /// Canonical table identifier.
    pub table: ir::TableId,
    /// Canonical field identifier.
    pub field: ir::FieldId,
    /// Selected scheme family identifier.
    pub scheme_id: SchemeId,
}

/// Compiler-owned compiled artifact wrapper.
#[derive(Debug, Clone)]
pub struct CompiledProgram {
    pub(crate) validated: ir::ValidatedProgram,
    pub(crate) field_schemes: Vec<StateFieldSchemeBinding>,
}

impl CompiledProgram {
    /// Borrow the validated canonical program.
    pub fn validated_program(&self) -> &ir::ValidatedProgram {
        &self.validated
    }

    /// Borrow the underlying canonical program.
    pub fn program(&self) -> &ir::Program {
        self.validated.as_program()
    }

    /// Borrow the ordered field-scheme sidecar.
    pub fn field_schemes(&self) -> &[StateFieldSchemeBinding] {
        &self.field_schemes
    }

    /// Consume into the validated canonical program.
    pub fn into_validated_program(self) -> ir::ValidatedProgram {
        self.validated
    }

    /// Consume into the validated canonical program plus field-scheme sidecar.
    pub fn into_parts(self) -> (ir::ValidatedProgram, Vec<StateFieldSchemeBinding>) {
        (self.validated, self.field_schemes)
    }
}

/// Compiler-sealed native artifact used by runtime execution and proving.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisteredProgram {
    pub(crate) validated: ir::ValidatedProgram,
    pub(crate) field_schemes: Vec<StateFieldSchemeBinding>,
    pub(crate) table_schemas: Vec<TableSchema>,
    pub(crate) profile_catalog: ProfileCatalog,
    pub(crate) tuple_encoding_defaults: TupleEncodingDefaults,
    pub(crate) capability_manifest: Vec<ir::CapabilityDescriptor>,
    pub(crate) static_table_artifact: StaticTableArtifact,
    pub(crate) metadata_envelope: ContractMetadataEnvelope,
    pub(crate) binding: ProgramBinding,
}

impl RegisteredProgram {
    /// Borrow the validated canonical program.
    pub fn validated_program(&self) -> &ir::ValidatedProgram {
        &self.validated
    }

    /// Borrow the canonical program.
    pub fn program(&self) -> &ir::Program {
        self.validated.as_program()
    }

    /// Borrow the ordered state-field scheme sidecar.
    pub fn field_schemes(&self) -> &[StateFieldSchemeBinding] {
        &self.field_schemes
    }

    /// Borrow the sealed table schemas used for runtime materialization.
    pub fn table_schemas(&self) -> &[TableSchema] {
        &self.table_schemas
    }

    /// Borrow the sealed profile catalog used for runtime materialization.
    pub fn profile_catalog(&self) -> &ProfileCatalog {
        &self.profile_catalog
    }

    /// Borrow the sealed tuple-encoding defaults used by relation/static-table
    /// digests.
    pub fn tuple_encoding_defaults(&self) -> &TupleEncodingDefaults {
        &self.tuple_encoding_defaults
    }

    /// Borrow the sealed capability manifest.
    pub fn capability_manifest(&self) -> &[ir::CapabilityDescriptor] {
        &self.capability_manifest
    }

    /// Borrow the compiler-sealed static relation table artifact.
    pub fn static_table_artifact(&self) -> &StaticTableArtifact {
        &self.static_table_artifact
    }

    /// Borrow the sealed metadata envelope.
    pub fn metadata_envelope(&self) -> &ContractMetadataEnvelope {
        &self.metadata_envelope
    }

    /// Borrow the compiler-sealed native binding.
    pub fn binding(&self) -> &ProgramBinding {
        &self.binding
    }

    /// Serialize this registered program canonically for hashing or persistence.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, CompilerError> {
        let mut bytes = b"tabula.compiler.registered_program.v1".to_vec();
        bytes.extend(
            serde_json::to_vec(self).map_err(|source| CompilerError::ParseJson {
                path: "<registered-program>".to_string(),
                source,
            })?,
        );
        Ok(bytes)
    }

    /// Canonical digest of the sealed registered program payload.
    pub fn canonical_digest(&self) -> Result<String, CompilerError> {
        Ok(format!(
            "{:x}",
            sha2::Sha256::digest(self.canonical_bytes()?)
        ))
    }

    /// Build the strict compatibility policy pinned to this registered program.
    pub fn compatibility_policy(&self) -> ContractCompatibilityPolicy {
        ContractCompatibilityPolicy {
            expected_profile_hash: self.metadata_envelope.profile_hash,
            expected_contract_schema_version: self.metadata_envelope.contract_schema_version,
            expected_binding_registry_version: self.metadata_envelope.binding_registry_version,
            expected_statement_schema_version: self.metadata_envelope.statement_schema_version,
            expected_verifier_profile_version: self.metadata_envelope.verifier_profile_version,
            expected_semantic_hash_stub: self.metadata_envelope.semantic_hash_stub,
        }
    }

    /// Resolve one sealed field profile through the compiler-owned profile catalog.
    pub fn resolve_field_profile(
        &self,
        table_id: ir::TableId,
        field_id: ir::FieldId,
    ) -> Result<tabula_profile::ResolvedColumnProfileRef<'_>, String> {
        let column_profile_id = self
            .table_schemas
            .iter()
            .find(|schema| schema.id == table_id.into())
            .and_then(|schema| {
                schema
                    .columns
                    .iter()
                    .find(|column| column.id == field_id.into())
            })
            .map(|column| column.column_profile_id)
            .ok_or_else(|| {
                format!(
                    "table {} field {} is missing from registered schema",
                    table_id.0, field_id.0
                )
            })?;
        self.profile_catalog
            .resolve_column_profile(column_profile_id)
            .map_err(|err| err.to_string())
    }
}
