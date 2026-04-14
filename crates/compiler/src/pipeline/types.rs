use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use sha2::Digest as _;

use tabula_contract::{
    CONTRACT_SCHEMA_VERSION, ContractCompatibilityPolicy, ContractMetadataEnvelope, ProgramBinding,
    STATEMENT_SCHEMA_VERSION, StaticTableArtifact, TupleEncodingDefaults, VERIFIER_PROFILE_VERSION,
};
use tabula_core::{ProgramExecutionContract, SchemeId};
use tabula_ir as ir;
use tabula_profile::ProfileCatalog;

use crate::error::CompilerError;
use crate::registration::{
    RegistrationContext, build_static_table_artifact, compute_profile_hash,
    compute_program_binding, compute_semantic_hash,
};

pub const REGISTERED_PROGRAM_SCHEMA_VERSION: u32 = 1;

/// Compiler-internal scheme selection for one state field.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub(crate) struct StateFieldSchemeBinding {
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

    /// Consume into the validated canonical program.
    pub fn into_validated_program(self) -> ir::ValidatedProgram {
        self.validated
    }

    /// Consume into the validated canonical program plus compiler-internal registration data.
    pub(crate) fn into_parts(self) -> (ir::ValidatedProgram, Vec<StateFieldSchemeBinding>) {
        (self.validated, self.field_schemes)
    }
}

/// Compiler-sealed native artifact used by runtime execution and proving.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisteredProgram {
    pub(crate) artifact_schema_version: u32,
    pub(crate) validated: ir::ValidatedProgram,
    pub(crate) execution_contract: ProgramExecutionContract,
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

    /// Registered artifact schema version.
    pub fn artifact_schema_version(&self) -> u32 {
        self.artifact_schema_version
    }

    /// Borrow the single sealed execution contract used by runtime, proof, and public schema projection.
    pub fn execution_contract(&self) -> &ProgramExecutionContract {
        &self.execution_contract
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
            expected_statement_schema_version: self.metadata_envelope.statement_schema_version,
            expected_verifier_profile_version: self.metadata_envelope.verifier_profile_version,
            expected_semantic_hash: self.metadata_envelope.semantic_hash,
        }
    }

    /// Fail closed if this artifact schema version is not supported by the current stack.
    pub fn validate_artifact_schema_version(&self) -> Result<(), CompilerError> {
        if self.artifact_schema_version != REGISTERED_PROGRAM_SCHEMA_VERSION {
            return Err(CompilerError::InvalidProgram(anyhow::anyhow!(format!(
                "unsupported registered artifact schema version {} (expected {})",
                self.artifact_schema_version, REGISTERED_PROGRAM_SCHEMA_VERSION
            ))));
        }
        Ok(())
    }

    /// Fail closed unless this artifact is self-consistent and compatible with
    /// the current stack's sealed contract policy.
    pub fn validate_sealed_artifact(&self) -> Result<(), CompilerError> {
        self.validate_artifact_schema_version()?;

        let canonical_tuple_defaults = TupleEncodingDefaults::new(
            self.tuple_encoding_defaults.entries.clone(),
        )
        .map_err(|error| CompilerError::ArtifactMismatch {
            detail: format!("tuple encoding defaults are not canonical: {error}"),
        })?;
        if canonical_tuple_defaults != self.tuple_encoding_defaults {
            return Err(CompilerError::ArtifactMismatch {
                detail: "tuple encoding defaults diverge from canonical ordering".to_string(),
            });
        }

        let expected_profile_hash =
            compute_profile_hash(&self.execution_contract, &self.profile_catalog).map_err(
                |error| CompilerError::ArtifactMismatch {
                    detail: format!("failed to recompute profile hash: {error}"),
                },
            )?;
        let expected_semantic_hash = compute_semantic_hash(
            self.program(),
            &self.execution_contract,
            &self.profile_catalog,
        )
        .map_err(|error| CompilerError::ArtifactMismatch {
            detail: format!("failed to recompute semantic hash: {error}"),
        })?;

        ContractCompatibilityPolicy {
            expected_profile_hash,
            expected_contract_schema_version: CONTRACT_SCHEMA_VERSION,
            expected_statement_schema_version: STATEMENT_SCHEMA_VERSION,
            expected_verifier_profile_version: VERIFIER_PROFILE_VERSION,
            expected_semantic_hash,
        }
        .validate(&self.metadata_envelope)
        .map_err(CompilerError::ContractMetadataMismatch)?;

        let registration_context =
            RegistrationContext::builtin().map_err(|error| CompilerError::ArtifactMismatch {
                detail: format!("failed to seed builtin registration context: {error}"),
            })?;
        let expected_static_table_artifact = build_static_table_artifact(
            self.program(),
            &registration_context,
            &self.tuple_encoding_defaults,
        )
        .map_err(|error| CompilerError::ArtifactMismatch {
            detail: format!("failed to rebuild static table artifact: {error}"),
        })?;
        if expected_static_table_artifact != self.static_table_artifact {
            return Err(CompilerError::ArtifactMismatch {
                detail: "static table artifact does not match the compiler-derived relation digest"
                    .to_string(),
            });
        }

        let expected_binding = compute_program_binding(
            self.program(),
            &self.execution_contract,
            &self.metadata_envelope,
        )
        .map_err(|error| CompilerError::ArtifactMismatch {
            detail: format!("failed to recompute program binding: {error}"),
        })?;
        if expected_binding != self.binding {
            return Err(CompilerError::ArtifactMismatch {
                detail: "program binding does not match the compiler-derived artifact binding"
                    .to_string(),
            });
        }

        Ok(())
    }
}
