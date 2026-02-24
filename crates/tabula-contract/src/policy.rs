//! Compatibility policy and fail-closed validation.

use crate::binding::ApplyBatchField;
use crate::envelope::ContractMetadataEnvelope;
use crate::validate_contract_schema_version;

/// Compatibility policy applied at proof entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractCompatibilityPolicy {
    /// Expected profile hash from the canonical driver path.
    pub expected_profile_hash: [u8; 32],
    /// Expected contract schema version.
    pub expected_contract_schema_version: u32,
    /// Expected statement binding registry version.
    pub expected_statement_binding_version: u32,
    /// Expected semantic hash stub.
    pub expected_semantic_hash_stub: Option<[u8; 32]>,
}

impl ContractCompatibilityPolicy {
    /// Validate envelope with fail-closed policy.
    pub fn validate(
        &self,
        envelope: &ContractMetadataEnvelope,
    ) -> Result<(), ContractValidationError> {
        validate_contract_schema_version(envelope.contract_schema_version)?;

        if envelope.contract_schema_version != self.expected_contract_schema_version {
            return Err(ContractValidationError::ContractSchemaVersionMismatch {
                expected: self.expected_contract_schema_version,
                got: envelope.contract_schema_version,
            });
        }
        if envelope.statement_binding_version != self.expected_statement_binding_version {
            return Err(ContractValidationError::StatementBindingVersionMismatch {
                expected: self.expected_statement_binding_version,
                got: envelope.statement_binding_version,
            });
        }
        if envelope.profile_hash != self.expected_profile_hash {
            return Err(ContractValidationError::ProfileMismatch);
        }
        if envelope.semantic_hash_stub != self.expected_semantic_hash_stub {
            return Err(ContractValidationError::SemanticHashMismatch);
        }
        Ok(())
    }
}

/// Fail-closed contract validation errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContractValidationError {
    /// Envelope references an unsupported contract schema version.
    UnknownContractSchemaVersion {
        /// Version provided by envelope.
        got: u32,
    },
    /// Known schema version does not match selected profile policy.
    ContractSchemaVersionMismatch {
        /// Version expected by policy.
        expected: u32,
        /// Version provided by envelope.
        got: u32,
    },
    /// Statement binding registry version mismatch.
    StatementBindingVersionMismatch {
        /// Version expected by policy.
        expected: u32,
        /// Version provided by envelope.
        got: u32,
    },
    /// Profile hash mismatch.
    ProfileMismatch,
    /// Semantic hash stub mismatch.
    SemanticHashMismatch,
    /// Statement binding registry is incomplete.
    IncompleteStatementBinding {
        /// Missing fields that were not classified as `BoundInAir` or `Deferred`.
        missing_fields: Vec<ApplyBatchField>,
    },
}

impl ContractValidationError {
    /// Stable machine-readable error code.
    pub fn code(&self) -> &'static str {
        match self {
            Self::UnknownContractSchemaVersion { .. } => "unknown_contract_schema_version",
            Self::ContractSchemaVersionMismatch { .. } => "contract_schema_version_mismatch",
            Self::StatementBindingVersionMismatch { .. } => "statement_binding_version_mismatch",
            Self::ProfileMismatch => "profile_mismatch",
            Self::SemanticHashMismatch => "semantic_hash_mismatch",
            Self::IncompleteStatementBinding { .. } => "binding_incomplete",
        }
    }
}

impl std::fmt::Display for ContractValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownContractSchemaVersion { got } => {
                write!(
                    f,
                    "[{}] unsupported contract schema version {}",
                    self.code(),
                    got
                )
            }
            Self::ContractSchemaVersionMismatch { expected, got } => write!(
                f,
                "[{}] contract schema version mismatch: expected {}, got {}",
                self.code(),
                expected,
                got
            ),
            Self::StatementBindingVersionMismatch { expected, got } => write!(
                f,
                "[{}] statement binding version mismatch: expected {}, got {}",
                self.code(),
                expected,
                got
            ),
            Self::ProfileMismatch => write!(
                f,
                "[{}] profile hash mismatch (fallback is forbidden)",
                self.code()
            ),
            Self::SemanticHashMismatch => {
                write!(f, "[{}] semantic hash stub mismatch", self.code())
            }
            Self::IncompleteStatementBinding { missing_fields } => write!(
                f,
                "[{}] statement binding registry missing fields: {:?}",
                self.code(),
                missing_fields
            ),
        }
    }
}

impl std::error::Error for ContractValidationError {}
