//! Program artifact models.

use serde::{Deserialize, Serialize};

#[cfg(test)]
use sha2::{Digest as _, Sha256};

use tabula_contract::ContractMetadataEnvelope;
use tabula_core::{ColId, ColumnProfileId, TableId, TableSchema};
use tabula_ir::{PrecompileId, PrecompileSignature, PropertyRequirement, TxTypeDef};
use tabula_profile::{ProfileCatalog, ResolvedColumnProfileRef};

use crate::ArtifactError;
use crate::canonical::{bytes_to_hex, canonical_json_bytes, canonical_json_digest};

#[cfg(test)]
const PRECOMPILE_DESCRIPTOR_HASH_DOMAIN: &[u8] = b"tabula.artifact.precompile_descriptor.v1";

/// Canonical descriptor binding one precompile capability to a verifier-visible contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrecompileDescriptor {
    /// Portable precompile identifier.
    pub precompile_id: PrecompileId,
    /// Precompile implementation/profile version.
    pub precompile_version: u16,
    /// Explicit typed I/O contract sealed into the artifact.
    pub signature: PrecompileSignature,
    /// Canonical hash of the precompile semantic contract.
    pub semantic_hash: [u8; 32],
}

impl PrecompileDescriptor {
    /// Build one explicit sealed descriptor.
    #[must_use]
    pub fn new(
        precompile_id: PrecompileId,
        precompile_version: u16,
        signature: PrecompileSignature,
        semantic_hash: [u8; 32],
    ) -> Self {
        Self {
            precompile_id,
            precompile_version,
            signature,
            semantic_hash,
        }
    }
}

#[cfg(test)]
fn hash_precompile_descriptor(domain_suffix: &str, label: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(PRECOMPILE_DESCRIPTOR_HASH_DOMAIN);
    hasher.update(domain_suffix.as_bytes());
    hasher.update(label.as_bytes());
    hasher.finalize().into()
}

/// Sealed artifact used for storage, transport, and verification.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    /// Table schema definitions.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub table_schemas: Vec<TableSchema>,
    /// Canonical semantic profile catalog sealed for this program.
    pub profile_catalog: ProfileCatalog,
    /// Transaction type definitions.
    pub tx_types: Vec<TxTypeDef>,
    /// Capability manifest: precompiles required by this program.
    pub precompile_manifest: Vec<PrecompileDescriptor>,
    /// Capability manifest: exact structural property requirements required by this program.
    pub required_property_requirements: Vec<PropertyRequirement>,
    /// Canonical contract metadata envelope for compatibility checks.
    pub contract_metadata: ContractMetadataEnvelope,
}

impl Artifact {
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

    /// Resolve one sealed column `(table_id, col_id)` into its canonical profile-backed view.
    pub fn resolve_column_profile(
        &self,
        table_id: TableId,
        col_id: ColId,
    ) -> Result<ResolvedColumnProfileRef<'_>, ArtifactError> {
        let column_profile_id =
            column_profile_id_for(self.table_schemas.as_slice(), table_id, col_id)?;
        self.profile_catalog
            .resolve_column_profile(column_profile_id)
            .map_err(|err| ArtifactError::InvalidProfileProjection {
                detail: format!(
                    "table {} col {} profile {} could not be resolved: {err}",
                    table_id.0, col_id.0, column_profile_id.0
                ),
            })
    }
}

fn column_profile_id_for(
    table_schemas: &[TableSchema],
    table_id: TableId,
    col_id: ColId,
) -> Result<ColumnProfileId, ArtifactError> {
    let Some(column_profile_id) = table_schemas
        .iter()
        .find(|schema| schema.id == table_id)
        .and_then(|schema| schema.columns.iter().find(|column| column.id == col_id))
        .map(|column| column.column_profile_id)
    else {
        return Err(ArtifactError::InvalidProfileProjection {
            detail: format!(
                "table {} col {} is missing from sealed schema",
                table_id.0, col_id.0
            ),
        });
    };
    Ok(column_profile_id)
}

#[cfg(test)]
mod tests {
    use tabula_contract::{
        BINDING_VERSION_V1, CONTRACT_SCHEMA_VERSION_V1, ContractMetadataEnvelope,
        STATEMENT_SCHEMA_VERSION_V1, VERIFIER_PROFILE_VERSION_V1,
    };
    use tabula_core::{ColId, ColumnProfileId, TableId, TableSchema, TxTypeId};
    use tabula_ir::TxTypeDef;
    use tabula_ir::{PrecompileId, PrecompileSignature, PrecompileValueProfile};
    use tabula_profile::{
        ColumnProfile, CommitmentRole, ENCODING_U64_ID, SCHEME_PROFILE_SSMC_ID, TYPE_U64_ID,
        builtin_catalog,
    };

    use super::{Artifact, PrecompileDescriptor, hash_precompile_descriptor};

    #[test]
    fn artifact_canonical_digest_is_deterministic() {
        let mut profile_catalog = builtin_catalog().expect("built-in catalog");
        let type_descriptor = profile_catalog
            .types
            .iter()
            .find(|descriptor| descriptor.type_id == TYPE_U64_ID)
            .cloned()
            .expect("u64 type");
        let encoding_profile = profile_catalog
            .encodings
            .iter()
            .find(|profile| profile.encoding_profile_id == ENCODING_U64_ID)
            .cloned()
            .expect("u64 encoding");
        let scheme_profile = profile_catalog
            .schemes
            .iter()
            .find(|profile| profile.scheme_profile_id == SCHEME_PROFILE_SSMC_ID)
            .cloned()
            .expect("ssmc scheme");
        let column_profile = ColumnProfile::new(
            ColumnProfileId(0),
            "accounts.balance",
            None,
            &type_descriptor,
            &encoding_profile,
            &scheme_profile,
            CommitmentRole::IncludedInRoot,
        )
        .expect("column profile");
        profile_catalog
            .register_column(column_profile)
            .expect("register column profile");

        let artifact = Artifact {
            table_schemas: vec![TableSchema {
                id: TableId(1),
                name: "accounts".to_string(),
                columns: vec![tabula_core::ColumnDef {
                    id: ColId(0),
                    name: "balance".to_string(),
                    column_profile_id: ColumnProfileId(0),
                }],
            }],
            profile_catalog,
            tx_types: vec![TxTypeDef {
                id: TxTypeId(1),
                name: "touch".to_string(),
                param_schema: vec![],
                body: vec![],
            }],
            precompile_manifest: vec![PrecompileDescriptor::new(
                PrecompileId(0x0001),
                1,
                PrecompileSignature::new(
                    vec![PrecompileValueProfile {
                        type_id: TYPE_U64_ID,
                        encoding_profile_id: ENCODING_U64_ID,
                    }],
                    vec![],
                ),
                hash_precompile_descriptor("semantic", "builtin:test:semantic"),
            )],
            required_property_requirements: vec![],
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
