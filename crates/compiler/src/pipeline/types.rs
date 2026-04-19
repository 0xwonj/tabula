use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use sha2::Digest as _;

use tabula_contract::{ContractCompatibilityPolicy, SealedArtifact};
use tabula_core::SchemeId;
use tabula_ir as ir;

use crate::error::CompilerError;
use crate::registration::{
    RegistrationContext, build_static_table_artifact, compute_program_binding,
    compute_semantic_hash,
};

pub const REGISTERED_PROGRAM_SCHEMA_VERSION: u32 = 2;

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
    pub(crate) sealed: SealedArtifact,
    pub(crate) validated: ir::ValidatedProgram,
    pub(crate) capability_manifest: Vec<ir::CapabilityDescriptor>,
}

impl RegisteredProgram {
    // ── IR-side accessors ────────────────────────────────────────────────────

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

    /// Borrow the sealed capability manifest.
    pub fn capability_manifest(&self) -> &[ir::CapabilityDescriptor] {
        &self.capability_manifest
    }

    // ── Sealed artifact accessor ─────────────────────────────────────────────

    /// Borrow the contract-layer sealed artifact embedded in this registered program.
    pub fn sealed(&self) -> &SealedArtifact {
        &self.sealed
    }

    // ── Contract-field proxy accessors ───────────────────────────────────────

    /// Borrow the single sealed execution contract used by runtime, proof, and public schema projection.
    pub fn execution_contract(&self) -> &tabula_core::ProgramExecutionContract {
        self.sealed.execution_contract()
    }

    /// Borrow the sealed profile catalog used for runtime materialization.
    pub fn profile_catalog(&self) -> &tabula_profile::ProfileCatalog {
        self.sealed.profile_catalog()
    }

    /// Borrow the sealed tuple-encoding defaults used by relation/static-table
    /// digests.
    pub fn tuple_encoding_defaults(&self) -> &tabula_contract::TupleEncodingDefaults {
        self.sealed.tuple_encoding_defaults()
    }

    /// Borrow the compiler-sealed static relation table artifact.
    pub fn static_table_artifact(&self) -> &tabula_contract::StaticTableArtifact {
        self.sealed.static_table_artifact()
    }

    /// Borrow the sealed metadata envelope.
    pub fn metadata_envelope(&self) -> &tabula_contract::ContractMetadataEnvelope {
        self.sealed.metadata_envelope()
    }

    /// Borrow the compiler-sealed native binding.
    pub fn binding(&self) -> &tabula_contract::ProgramBinding {
        self.sealed.binding()
    }

    // ── Canonical bytes / digest ─────────────────────────────────────────────

    /// Serialize this registered program canonically for hashing or persistence.
    ///
    /// The magic prefix is versioned to the struct layout; it changed from
    /// `v1` to `v2` when `RegisteredProgram` was restructured around
    /// `SealedArtifact` (T4/SP-1.5).
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, CompilerError> {
        let mut bytes = b"tabula.compiler.registered_program.v2".to_vec();
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

    // ── Compatibility policy ─────────────────────────────────────────────────

    /// Build the strict compatibility policy pinned to this registered program.
    pub fn compatibility_policy(&self) -> ContractCompatibilityPolicy {
        let envelope = self.sealed.metadata_envelope();
        ContractCompatibilityPolicy {
            expected_profile_hash: envelope.profile_hash,
            expected_contract_schema_version: envelope.contract_schema_version,
            expected_statement_schema_version: envelope.statement_schema_version,
            expected_verifier_profile_version: envelope.verifier_profile_version,
            expected_semantic_hash: envelope.semantic_hash,
        }
    }

    // ── Validation ───────────────────────────────────────────────────────────

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
    ///
    /// Delegates sealed-only checks (schema version, tuple canonicality,
    /// profile hash, metadata envelope policy) to
    /// [`SealedArtifact::validate()`], then runs the IR-requiring checks
    /// (semantic hash recomputation, static-table rebuild, binding
    /// recomputation) that the contract layer cannot perform.
    pub fn validate_sealed_artifact(&self) -> Result<(), CompilerError> {
        self.validate_artifact_schema_version()?;

        self.sealed
            .validate()
            .map_err(|e| CompilerError::ArtifactMismatch {
                detail: e.to_string(),
            })?;

        // IR-requiring check: semantic hash recomputation.
        let expected_semantic_hash = compute_semantic_hash(
            self.program(),
            self.sealed.execution_contract(),
            self.sealed.profile_catalog(),
        )
        .map_err(|error| CompilerError::ArtifactMismatch {
            detail: format!("failed to recompute semantic hash: {error}"),
        })?;
        if expected_semantic_hash != self.sealed.metadata_envelope().semantic_hash {
            return Err(CompilerError::ArtifactMismatch {
                detail: "semantic hash mismatch".to_string(),
            });
        }

        // IR-requiring check: static table artifact rebuild.
        let registration_context =
            RegistrationContext::builtin().map_err(|error| CompilerError::ArtifactMismatch {
                detail: format!("failed to seed builtin registration context: {error}"),
            })?;
        let expected_static_table_artifact = build_static_table_artifact(
            self.program(),
            &registration_context,
            self.sealed.tuple_encoding_defaults(),
        )
        .map_err(|error| CompilerError::ArtifactMismatch {
            detail: format!("failed to rebuild static table artifact: {error}"),
        })?;
        if expected_static_table_artifact != *self.sealed.static_table_artifact() {
            return Err(CompilerError::ArtifactMismatch {
                detail: "static table artifact does not match the compiler-derived relation digest"
                    .to_string(),
            });
        }

        // IR-requiring check: binding recomputation.
        let expected_binding = compute_program_binding(
            self.program(),
            self.sealed.execution_contract(),
            self.sealed.metadata_envelope(),
        )
        .map_err(|error| CompilerError::ArtifactMismatch {
            detail: format!("failed to recompute program binding: {error}"),
        })?;
        if expected_binding != *self.sealed.binding() {
            return Err(CompilerError::ArtifactMismatch {
                detail: "program binding does not match the compiler-derived artifact binding"
                    .to_string(),
            });
        }

        Ok(())
    }
}
