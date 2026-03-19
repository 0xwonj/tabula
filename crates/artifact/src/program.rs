//! Program artifact models.

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use tabula_contract::ContractMetadataEnvelope;
use tabula_core::{ColId, ColumnLayoutKind, RootProfileId, SchemeId, TableId, TableSchema};
use tabula_ir::{PrecompileId, PropertyQueryKind, PropertyRequirement, TxTypeDef};

use crate::ArtifactError;
use crate::canonical::{bytes_to_hex, canonical_json_bytes, canonical_json_digest};

const SCHEME_DESCRIPTOR_HASH_DOMAIN: &[u8] = b"tabula.artifact.scheme_descriptor.v1";
const BUILTIN_SCHEME_VERSION_V1: u16 = 1;

/// Canonical descriptor binding one column scheme to a verifier-visible contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SchemeDescriptor {
    /// Portable scheme identifier.
    pub scheme_id: SchemeId,
    /// Scheme implementation/profile version.
    pub scheme_version: u16,
    /// Verifier-relevant commitment layout/backend used by this scheme profile.
    pub layout_kind: ColumnLayoutKind,
    /// Canonical hash of the scheme parameter profile.
    pub params_hash: [u8; 32],
    /// Compatible root-proof profile identifier.
    pub root_profile_id: RootProfileId,
    /// Structural property query kinds this scheme supports.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub supported_property_query_kinds: Vec<PropertyQueryKind>,
}

impl SchemeDescriptor {
    /// Built-in descriptor for the SSMC column scheme.
    pub fn builtin_ssmc() -> Self {
        Self {
            scheme_id: SchemeId::SSMC,
            scheme_version: BUILTIN_SCHEME_VERSION_V1,
            layout_kind: ColumnLayoutKind::SSMC_V1,
            params_hash: hash_scheme_params("builtin:ssmc:v1:hash_chain:max_value_fes=5"),
            root_profile_id: RootProfileId::SMT_V1,
            supported_property_query_kinds: vec![
                PropertyQueryKind::Successor,
                PropertyQueryKind::Predecessor,
            ],
        }
    }

    /// Built-in descriptor for the SMT column scheme.
    pub fn builtin_smt() -> Self {
        Self {
            scheme_id: SchemeId::SMT,
            scheme_version: BUILTIN_SCHEME_VERSION_V1,
            layout_kind: ColumnLayoutKind::SMT_V1,
            params_hash: hash_scheme_params("builtin:smt:v1:col_data_depth=32:unordered"),
            root_profile_id: RootProfileId::SMT_V1,
            supported_property_query_kinds: vec![],
        }
    }
}

fn hash_scheme_params(label: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(SCHEME_DESCRIPTOR_HASH_DOMAIN);
    hasher.update(label.as_bytes());
    hasher.finalize().into()
}

/// Canonical proof-planning metadata for one committed column.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColumnProofPlan {
    /// Table identifier.
    pub table_id: TableId,
    /// Column identifier.
    pub col_id: ColId,
    /// Commitment scheme tag chosen by the compiler.
    pub scheme_id: SchemeId,
    /// Sealed scheme descriptor expected by runtime and verifier.
    pub scheme_descriptor: SchemeDescriptor,
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

    use super::{ColumnProofPlan, ProgramArtifact, SchemeDescriptor};

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
                scheme_descriptor: SchemeDescriptor::builtin_ssmc(),
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
