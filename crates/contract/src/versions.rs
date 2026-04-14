//! Contract/version constants and fail-closed version validation.

use crate::compatibility::ContractValidationError;

/// Current contract schema version.
pub const CONTRACT_SCHEMA_VERSION: u32 = 1;
/// Current execution statement schema version.
pub const STATEMENT_SCHEMA_VERSION: u32 = 1;
/// Current verifier profile version.
pub const VERIFIER_PROFILE_VERSION: u32 = 1;
/// Current proof envelope schema version.
pub const PROOF_ENVELOPE_VERSION: u32 = 1;

/// Validate contract schema version with fail-closed policy.
pub fn validate_contract_schema_version(version: u32) -> Result<(), ContractValidationError> {
    if version != CONTRACT_SCHEMA_VERSION {
        return Err(ContractValidationError::UnknownContractSchemaVersion { got: version });
    }
    Ok(())
}

/// Validate execution statement schema version with fail-closed policy.
pub fn validate_statement_schema_version(version: u32) -> Result<(), ContractValidationError> {
    if version != STATEMENT_SCHEMA_VERSION {
        return Err(ContractValidationError::UnknownStatementSchemaVersion { got: version });
    }
    Ok(())
}

/// Validate verifier profile version with fail-closed policy.
pub fn validate_verifier_profile_version(version: u32) -> Result<(), ContractValidationError> {
    if version != VERIFIER_PROFILE_VERSION {
        return Err(ContractValidationError::UnknownVerifierProfileVersion { got: version });
    }
    Ok(())
}

/// Validate proof envelope version with fail-closed policy.
pub fn validate_proof_envelope_version(version: u32) -> Result<(), ContractValidationError> {
    if version != PROOF_ENVELOPE_VERSION {
        return Err(ContractValidationError::UnknownProofEnvelopeVersion { got: version });
    }
    Ok(())
}
