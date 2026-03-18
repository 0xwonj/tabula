//! Program artifact models.

use serde::{Deserialize, Serialize};

use tabula_contract::ContractMetadataEnvelope;
use tabula_core::{ColId, SchemeId, TableId, TableSchema};
use tabula_ir::{PrecompileId, PropertyRequirement, TxTypeDef};

use crate::ArtifactError;
use crate::canonical::{bytes_to_hex, canonical_json_bytes, canonical_json_digest};

/// Canonical proof-planning metadata for one committed column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColumnProofPlan {
    /// Table identifier.
    pub table_id: TableId,
    /// Column identifier.
    pub col_id: ColId,
    /// Commitment scheme tag chosen by the compiler.
    pub scheme_id: SchemeId,
    /// Whether this column participates in the root commitment.
    pub receives_commitment: bool,
}

/// Sealed program artifact used for storage, transport, and verification.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProgramArtifact {
    /// Table schema definitions.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub table_schemas: Vec<TableSchema>,
    /// Transaction type definitions.
    pub tx_types: Vec<TxTypeDef>,
    /// Capability manifest: precompiles required by this program.
    pub required_precompile_ids: Vec<PrecompileId>,
    /// Capability manifest: exact structural property requirements required by this program.
    pub required_property_requirements: Vec<PropertyRequirement>,
    /// Compiler-owned proof plan for all committed columns.
    pub column_proof_plan: Vec<ColumnProofPlan>,
    /// Canonical contract metadata envelope for compatibility checks.
    pub contract_metadata: ContractMetadataEnvelope,
}

impl ProgramArtifact {
    /// Serialize this sealed artifact into its canonical byte representation.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, ArtifactError> {
        canonical_json_bytes(self)
    }

    /// Compute the canonical digest bytes for this artifact.
    pub fn canonical_digest_bytes(&self) -> Result<[u8; 32], ArtifactError> {
        canonical_json_digest("program", self)
    }

    /// Compute the canonical digest hex string for this artifact.
    pub fn canonical_digest(&self) -> Result<String, ArtifactError> {
        Ok(bytes_to_hex(&self.canonical_digest_bytes()?))
    }
}

#[cfg(test)]
mod tests {
    use tabula_contract::{
        BINDING_VERSION_V1, CONTRACT_SCHEMA_VERSION_V1, ContractMetadataEnvelope,
        STATEMENT_SCHEMA_VERSION_V1, VERIFIER_PROFILE_VERSION_V1,
    };
    use tabula_core::{ColId, SchemeId, TableId, TableSchema, TxTypeId, ValueType};
    use tabula_ir::TxTypeDef;

    use super::{ColumnProofPlan, ProgramArtifact};

    #[test]
    fn program_artifact_canonical_digest_is_deterministic() {
        let artifact = ProgramArtifact {
            table_schemas: vec![TableSchema {
                id: TableId(1),
                name: "accounts".to_string(),
                columns: vec![tabula_core::ColumnDef {
                    id: ColId(0),
                    name: "balance".to_string(),
                    value_type: ValueType::U64,
                }],
            }],
            tx_types: vec![TxTypeDef {
                id: TxTypeId(1),
                name: "touch".to_string(),
                param_schema: vec![],
                body: vec![],
            }],
            required_precompile_ids: vec![],
            required_property_requirements: vec![],
            column_proof_plan: vec![ColumnProofPlan {
                table_id: TableId(1),
                col_id: ColId(0),
                scheme_id: SchemeId::SSMC,
                receives_commitment: true,
            }],
            contract_metadata: ContractMetadataEnvelope {
                profile_hash: [7; 32],
                contract_schema_version: CONTRACT_SCHEMA_VERSION_V1,
                binding_version: BINDING_VERSION_V1,
                statement_schema_version: STATEMENT_SCHEMA_VERSION_V1,
                verifier_profile_version: VERIFIER_PROFILE_VERSION_V1,
                semantic_hash_stub: None,
            },
        };

        assert_eq!(
            artifact.canonical_digest().expect("first digest"),
            artifact.canonical_digest().expect("second digest")
        );
    }
}
