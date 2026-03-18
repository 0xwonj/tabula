//! Compatibility policy and fail-closed validation.

use crate::binding::PublicInputField;
use crate::envelope::ContractMetadataEnvelope;
use crate::validate_binding_version;
use crate::validate_contract_schema_version;
use crate::validate_statement_schema_version;
use crate::validate_verifier_profile_version;

/// Compatibility policy applied at proof entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractCompatibilityPolicy {
    /// Expected profile hash from the canonical driver path.
    pub expected_profile_hash: [u8; 32],
    /// Expected contract schema version.
    pub expected_contract_schema_version: u32,
    /// Expected binding registry version.
    pub expected_binding_version: u32,
    /// Expected execution statement schema version.
    pub expected_statement_schema_version: u32,
    /// Expected verifier profile version.
    pub expected_verifier_profile_version: u32,
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
        validate_binding_version(envelope.binding_version)?;
        validate_statement_schema_version(envelope.statement_schema_version)?;
        validate_verifier_profile_version(envelope.verifier_profile_version)?;
        validate_binding_version(self.expected_binding_version)?;
        validate_statement_schema_version(self.expected_statement_schema_version)?;
        validate_verifier_profile_version(self.expected_verifier_profile_version)?;

        if envelope.contract_schema_version != self.expected_contract_schema_version {
            return Err(ContractValidationError::ContractSchemaVersionMismatch {
                expected: self.expected_contract_schema_version,
                got: envelope.contract_schema_version,
            });
        }
        if envelope.binding_version != self.expected_binding_version {
            return Err(ContractValidationError::BindingVersionMismatch {
                expected: self.expected_binding_version,
                got: envelope.binding_version,
            });
        }
        if envelope.statement_schema_version != self.expected_statement_schema_version {
            return Err(ContractValidationError::StatementSchemaVersionMismatch {
                expected: self.expected_statement_schema_version,
                got: envelope.statement_schema_version,
            });
        }
        if envelope.verifier_profile_version != self.expected_verifier_profile_version {
            return Err(ContractValidationError::VerifierProfileVersionMismatch {
                expected: self.expected_verifier_profile_version,
                got: envelope.verifier_profile_version,
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
    /// Envelope or policy references an unsupported binding registry version.
    UnknownBindingVersion {
        /// Version provided by envelope or policy.
        got: u32,
    },
    /// Envelope or policy references an unsupported statement schema version.
    UnknownStatementSchemaVersion {
        /// Version provided by envelope or policy.
        got: u32,
    },
    /// Envelope or policy references an unsupported verifier profile version.
    UnknownVerifierProfileVersion {
        /// Version provided by envelope or policy.
        got: u32,
    },
    /// Known schema version does not match selected profile policy.
    ContractSchemaVersionMismatch {
        /// Version expected by policy.
        expected: u32,
        /// Version provided by envelope.
        got: u32,
    },
    /// Binding registry version mismatch.
    BindingVersionMismatch {
        /// Version expected by policy.
        expected: u32,
        /// Version provided by envelope.
        got: u32,
    },
    /// Statement schema version mismatch.
    StatementSchemaVersionMismatch {
        /// Version expected by policy.
        expected: u32,
        /// Version provided by envelope.
        got: u32,
    },
    /// Verifier profile version mismatch.
    VerifierProfileVersionMismatch {
        /// Version expected by policy.
        expected: u32,
        /// Version provided by envelope.
        got: u32,
    },
    /// Profile hash mismatch.
    ProfileMismatch,
    /// Semantic hash stub mismatch.
    SemanticHashMismatch,
    /// Binding registry is incomplete.
    IncompleteBinding {
        /// Missing fields that were not classified as `BoundInAir` or `Deferred`.
        missing_fields: Vec<PublicInputField>,
    },
}

impl ContractValidationError {
    /// Stable machine-readable error code.
    pub fn code(&self) -> &'static str {
        match self {
            Self::UnknownContractSchemaVersion { .. } => "unknown_contract_schema_version",
            Self::UnknownBindingVersion { .. } => "unknown_binding_version",
            Self::UnknownStatementSchemaVersion { .. } => "unknown_statement_schema_version",
            Self::UnknownVerifierProfileVersion { .. } => "unknown_verifier_profile_version",
            Self::ContractSchemaVersionMismatch { .. } => "contract_schema_version_mismatch",
            Self::BindingVersionMismatch { .. } => "binding_version_mismatch",
            Self::StatementSchemaVersionMismatch { .. } => "statement_schema_version_mismatch",
            Self::VerifierProfileVersionMismatch { .. } => "verifier_profile_version_mismatch",
            Self::ProfileMismatch => "profile_mismatch",
            Self::SemanticHashMismatch => "semantic_hash_mismatch",
            Self::IncompleteBinding { .. } => "binding_incomplete",
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
            Self::UnknownBindingVersion { got } => {
                write!(
                    f,
                    "[{}] unsupported binding registry version {}",
                    self.code(),
                    got
                )
            }
            Self::UnknownStatementSchemaVersion { got } => {
                write!(
                    f,
                    "[{}] unsupported statement schema version {}",
                    self.code(),
                    got
                )
            }
            Self::UnknownVerifierProfileVersion { got } => {
                write!(
                    f,
                    "[{}] unsupported verifier profile version {}",
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
            Self::BindingVersionMismatch { expected, got } => write!(
                f,
                "[{}] binding version mismatch: expected {}, got {}",
                self.code(),
                expected,
                got
            ),
            Self::StatementSchemaVersionMismatch { expected, got } => write!(
                f,
                "[{}] statement schema version mismatch: expected {}, got {}",
                self.code(),
                expected,
                got
            ),
            Self::VerifierProfileVersionMismatch { expected, got } => write!(
                f,
                "[{}] verifier profile version mismatch: expected {}, got {}",
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
            Self::IncompleteBinding { missing_fields } => write!(
                f,
                "[{}] binding registry missing fields: {:?}",
                self.code(),
                missing_fields
            ),
        }
    }
}

impl std::error::Error for ContractValidationError {}
