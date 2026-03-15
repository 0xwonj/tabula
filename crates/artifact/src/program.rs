//! Program artifact models.

use serde::{Deserialize, Serialize};

use tabula_contract::{ContractCompatibilityPolicy, ContractMetadataEnvelope};
use tabula_core::TableSchema;
use tabula_ir::{Program, TxTypeDef};

/// In-memory semantic artifact produced by the compiler/registration phase.
///
/// This is the canonical handoff type between the compiler-side pipeline
/// and the runtime-side pipeline. It carries both the registered IR program
/// and the canonical metadata used for compatibility checks.
#[derive(Debug, Clone)]
pub struct CompiledProgram {
    /// Registered IR program.
    pub program: Program,
    /// Canonical table schemas consumed during registration.
    pub table_schemas: Vec<TableSchema>,
    /// Canonical transaction definitions consumed during registration.
    pub tx_types: Vec<TxTypeDef>,
    /// Canonical metadata envelope for proof compatibility checks.
    pub metadata_envelope: ContractMetadataEnvelope,
}

impl CompiledProgram {
    /// Build a strict compatibility policy pinned to this artifact's metadata.
    pub fn compatibility_policy(&self) -> ContractCompatibilityPolicy {
        ContractCompatibilityPolicy {
            expected_profile_hash: self.metadata_envelope.profile_hash,
            expected_contract_schema_version: self.metadata_envelope.contract_schema_version,
            expected_binding_version: self.metadata_envelope.binding_version,
            expected_semantic_hash_stub: self.metadata_envelope.semantic_hash_stub,
        }
    }

    /// Clone into a portable JSON artifact.
    pub fn as_program_artifact(&self) -> ProgramArtifact {
        ProgramArtifact::from(self)
    }

    /// Convert into a portable JSON artifact.
    pub fn into_program_artifact(self) -> ProgramArtifact {
        ProgramArtifact::from(self)
    }

    /// Backward-compatible alias for older compile/CLI call sites.
    pub fn into_program_file(self) -> ProgramArtifact {
        self.into_program_artifact()
    }
}

/// Program artifact used by compile/check/execute interfaces.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProgramArtifact {
    /// Table schema definitions.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub table_schemas: Vec<TableSchema>,
    /// Transaction type definitions.
    pub tx_types: Vec<TxTypeDef>,
    /// Optional metadata envelope (required for JSON artifact mode).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contract_metadata: Option<ContractMetadataEnvelope>,
}

impl From<&CompiledProgram> for ProgramArtifact {
    fn from(value: &CompiledProgram) -> Self {
        Self {
            table_schemas: value.table_schemas.clone(),
            tx_types: value.tx_types.clone(),
            contract_metadata: Some(value.metadata_envelope.clone()),
        }
    }
}

impl From<CompiledProgram> for ProgramArtifact {
    fn from(value: CompiledProgram) -> Self {
        Self {
            table_schemas: value.table_schemas,
            tx_types: value.tx_types,
            contract_metadata: Some(value.metadata_envelope),
        }
    }
}
