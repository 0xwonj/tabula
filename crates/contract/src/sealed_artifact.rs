//! Contract-layer sealed artifact: the IR-free portion of a registered
//! program that verifier-side code and runtime preparation consume.

use serde::{Deserialize, Serialize};
use sha2::Digest as _;

use tabula_core::ProgramExecutionContract;
use tabula_profile::ProfileCatalog;

use crate::{
    ContractMetadataEnvelope, ProgramBinding, SealedRelationPolicy, StaticTableArtifact,
    TupleEncodingDefaults,
};
use crate::versions::{
    CONTRACT_SCHEMA_VERSION, STATEMENT_SCHEMA_VERSION, VERIFIER_PROFILE_VERSION,
    validate_contract_schema_version, validate_statement_schema_version,
    validate_verifier_profile_version,
};

/// Schema version for the sealed artifact wire format.
pub const SEALED_ARTIFACT_SCHEMA_VERSION: u32 = 1;

/// Contract-layer sealed artifact.
///
/// Carries every field a verifier or runtime preparation step needs
/// that does NOT require IR. Two seal-time-computed bits
/// (`relation_policy`, `uses_ir_hash`) are stored here so the verifier
/// does not need to rescan IR.
///
/// IR-requiring checks (semantic hash, static-table rebuild, binding
/// recomputation) are intentionally deferred to
/// `RegisteredProgram::validate_sealed_artifact()`, which delegates
/// here and then runs those additional checks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SealedArtifact {
    pub(crate) schema_version: u32,
    pub(crate) execution_contract: ProgramExecutionContract,
    pub(crate) profile_catalog: ProfileCatalog,
    pub(crate) tuple_encoding_defaults: TupleEncodingDefaults,
    pub(crate) static_table_artifact: StaticTableArtifact,
    pub(crate) metadata_envelope: ContractMetadataEnvelope,
    pub(crate) binding: ProgramBinding,
    pub(crate) relation_policy: SealedRelationPolicy,
    pub(crate) uses_ir_hash: bool,
}

impl SealedArtifact {
    /// Build a sealed artifact from its sealed-at-registration parts.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        execution_contract: ProgramExecutionContract,
        profile_catalog: ProfileCatalog,
        tuple_encoding_defaults: TupleEncodingDefaults,
        static_table_artifact: StaticTableArtifact,
        metadata_envelope: ContractMetadataEnvelope,
        binding: ProgramBinding,
        relation_policy: SealedRelationPolicy,
        uses_ir_hash: bool,
    ) -> Self {
        Self {
            schema_version: SEALED_ARTIFACT_SCHEMA_VERSION,
            execution_contract,
            profile_catalog,
            tuple_encoding_defaults,
            static_table_artifact,
            metadata_envelope,
            binding,
            relation_policy,
            uses_ir_hash,
        }
    }

    /// Schema version of this sealed artifact wire format.
    pub fn schema_version(&self) -> u32 {
        self.schema_version
    }

    /// Borrow the sealed execution contract.
    pub fn execution_contract(&self) -> &ProgramExecutionContract {
        &self.execution_contract
    }

    /// Borrow the sealed profile catalog.
    pub fn profile_catalog(&self) -> &ProfileCatalog {
        &self.profile_catalog
    }

    /// Borrow the sealed tuple-encoding defaults.
    pub fn tuple_encoding_defaults(&self) -> &TupleEncodingDefaults {
        &self.tuple_encoding_defaults
    }

    /// Borrow the sealed static relation table artifact.
    pub fn static_table_artifact(&self) -> &StaticTableArtifact {
        &self.static_table_artifact
    }

    /// Borrow the sealed metadata envelope.
    pub fn metadata_envelope(&self) -> &ContractMetadataEnvelope {
        &self.metadata_envelope
    }

    /// Borrow the sealed program binding.
    pub fn binding(&self) -> &ProgramBinding {
        &self.binding
    }

    /// Sealed relation-table policy derived from IR at registration time.
    pub fn relation_policy(&self) -> SealedRelationPolicy {
        self.relation_policy
    }

    /// Whether the program uses the hash chip, sealed at registration time.
    pub fn uses_ir_hash(&self) -> bool {
        self.uses_ir_hash
    }

    /// Canonical bytes for hashing or persistence.
    ///
    /// Prefixed with a versioned magic string; payload is `serde_json`
    /// serialization of the struct. Mirrors the `RegisteredProgram`
    /// canonical-bytes pattern.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, SealedArtifactError> {
        let mut bytes = b"tabula.contract.sealed_artifact.v1".to_vec();
        bytes.extend(serde_json::to_vec(self).map_err(|source| {
            SealedArtifactError::Serialize {
                detail: source.to_string(),
            }
        })?);
        Ok(bytes)
    }

    /// Canonical SHA-256 digest of the sealed artifact as lowercase hex.
    pub fn canonical_digest(&self) -> Result<String, SealedArtifactError> {
        Ok(format!("{:x}", sha2::Sha256::digest(self.canonical_bytes()?)))
    }

    /// Fail closed unless the sealed artifact is self-consistent at the
    /// contract layer.
    ///
    /// Covers sealed-only checks: schema version, tuple encoding
    /// canonicality, profile hash recomputation, and the metadata
    /// envelope schema version compatibility checks.
    ///
    /// Checks requiring `ir::Program` (semantic hash recomputation,
    /// static-table rebuild, binding recomputation) are deferred to
    /// `RegisteredProgram::validate_sealed_artifact()`, which calls
    /// this method first and then runs those IR-requiring checks.
    pub fn validate(&self) -> Result<(), SealedArtifactError> {
        if self.schema_version != SEALED_ARTIFACT_SCHEMA_VERSION {
            return Err(SealedArtifactError::UnsupportedSchemaVersion {
                found: self.schema_version,
                expected: SEALED_ARTIFACT_SCHEMA_VERSION,
            });
        }

        // Tuple encoding canonicality: re-build from entries and verify
        // the result matches the stored defaults (catches unsorted or
        // duplicate entries).
        let canonical_tuple_defaults =
            TupleEncodingDefaults::new(self.tuple_encoding_defaults.entries.clone()).map_err(
                |error| SealedArtifactError::TupleEncodingNotCanonical {
                    detail: error.to_string(),
                },
            )?;
        if canonical_tuple_defaults != self.tuple_encoding_defaults {
            return Err(SealedArtifactError::TupleEncodingNotCanonical {
                detail: "tuple encoding defaults diverge from canonical ordering".to_string(),
            });
        }

        // Profile hash: recompute from sealed fields and compare to the
        // stored envelope value.
        let recomputed_profile_hash =
            compute_profile_hash(&self.execution_contract, &self.profile_catalog).map_err(
                |error| SealedArtifactError::ProfileHashMismatch {
                    detail: format!("failed to recompute profile hash: {error}"),
                },
            )?;
        if recomputed_profile_hash != self.metadata_envelope.profile_hash {
            return Err(SealedArtifactError::ProfileHashMismatch {
                detail: "recomputed profile hash does not match the sealed envelope".to_string(),
            });
        }

        // Metadata envelope schema version compatibility (sealed-only
        // subset — semantic hash equality requires IR and is deferred).
        validate_contract_schema_version(self.metadata_envelope.contract_schema_version)
            .map_err(|e| SealedArtifactError::ContractMetadataMismatch {
                detail: e.to_string(),
            })?;
        validate_statement_schema_version(self.metadata_envelope.statement_schema_version)
            .map_err(|e| SealedArtifactError::ContractMetadataMismatch {
                detail: e.to_string(),
            })?;
        validate_verifier_profile_version(self.metadata_envelope.verifier_profile_version)
            .map_err(|e| SealedArtifactError::ContractMetadataMismatch {
                detail: e.to_string(),
            })?;

        // Pin the stored schema versions against the current contract
        // constants (separate from "is the version known?" — also checks
        // exact match).
        if self.metadata_envelope.contract_schema_version != CONTRACT_SCHEMA_VERSION {
            return Err(SealedArtifactError::ContractMetadataMismatch {
                detail: format!(
                    "contract schema version mismatch: expected {CONTRACT_SCHEMA_VERSION}, \
                     found {}",
                    self.metadata_envelope.contract_schema_version
                ),
            });
        }
        if self.metadata_envelope.statement_schema_version != STATEMENT_SCHEMA_VERSION {
            return Err(SealedArtifactError::ContractMetadataMismatch {
                detail: format!(
                    "statement schema version mismatch: expected {STATEMENT_SCHEMA_VERSION}, \
                     found {}",
                    self.metadata_envelope.statement_schema_version
                ),
            });
        }
        if self.metadata_envelope.verifier_profile_version != VERIFIER_PROFILE_VERSION {
            return Err(SealedArtifactError::ContractMetadataMismatch {
                detail: format!(
                    "verifier profile version mismatch: expected {VERIFIER_PROFILE_VERSION}, \
                     found {}",
                    self.metadata_envelope.verifier_profile_version
                ),
            });
        }

        Ok(())
    }
}

/// Compute the profile hash from sealed execution contract and profile catalog.
///
/// Relocated from `tabula-compiler::registration::binding::compute_profile_hash`.
/// Uses the same domain-separated blake3 hash to ensure identical output.
fn compute_profile_hash(
    execution_contract: &ProgramExecutionContract,
    profile_catalog: &ProfileCatalog,
) -> Result<[u8; 32], SealedArtifactError> {
    let execution_contract_bytes =
        borsh::to_vec(execution_contract).map_err(|error| SealedArtifactError::Serialize {
            detail: format!("failed to borsh-serialize execution contract: {error}"),
        })?;
    let profile_catalog_bytes =
        serde_json::to_vec(profile_catalog).map_err(|error| SealedArtifactError::Serialize {
            detail: format!("failed to serialize profile catalog: {error}"),
        })?;

    let mut hasher = blake3::Hasher::new();
    hasher.update(b"tabula.driver.profile_hash.v1");
    hasher.update(&execution_contract_bytes);
    hasher.update(&(profile_catalog_bytes.len() as u32).to_be_bytes());
    hasher.update(&profile_catalog_bytes);
    Ok(*hasher.finalize().as_bytes())
}

/// Errors produced by [`SealedArtifact`] operations.
#[derive(Debug, thiserror::Error)]
pub enum SealedArtifactError {
    /// Sealed artifact schema version is not supported by this stack.
    #[error("unsupported sealed artifact schema version {found} (expected {expected})")]
    UnsupportedSchemaVersion {
        /// Schema version found in the artifact.
        found: u32,
        /// Schema version expected by this stack.
        expected: u32,
    },
    /// Tuple encoding defaults are not in canonical form.
    #[error("tuple encoding defaults are not canonical: {detail}")]
    TupleEncodingNotCanonical {
        /// Detail describing the canonicality violation.
        detail: String,
    },
    /// Serialization of the artifact failed.
    #[error("failed to serialize sealed artifact: {detail}")]
    Serialize {
        /// Detail describing the serialization failure.
        detail: String,
    },
    /// Recomputed profile hash does not match the sealed envelope value.
    #[error("profile hash recomputation mismatch: {detail}")]
    ProfileHashMismatch {
        /// Detail describing the mismatch.
        detail: String,
    },
    /// Metadata envelope schema version or version pin check failed.
    #[error("contract metadata mismatch: {detail}")]
    ContractMetadataMismatch {
        /// Detail describing the mismatch.
        detail: String,
    },
}
