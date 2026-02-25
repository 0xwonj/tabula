//! Proof contract metadata, versioning, and binding policy.
//!
//! This module is the M11 "Contract Spine V1" source of truth.

#![warn(missing_docs)]

mod binding;
mod envelope;
mod policy;
mod rules;

// Version constants
/// Current contract schema version.
pub const CONTRACT_SCHEMA_VERSION_V1: u32 = 1;
/// Current binding registry version.
pub const BINDING_VERSION_V1: u32 = 1;
/// C10 ReadAccess bus schema version (v2 includes `tx_index`).
pub const C10_READ_ACCESS_SCHEMA_VERSION_V2: u32 = 2;
/// C11 WriteAccess bus schema version (v2 includes `tx_index`).
pub const C11_WRITE_ACCESS_SCHEMA_VERSION_V2: u32 = 2;

/// Validate contract schema version with fail-closed policy.
pub fn validate_contract_schema_version(version: u32) -> Result<(), ContractValidationError> {
    if version != CONTRACT_SCHEMA_VERSION_V1 {
        return Err(ContractValidationError::UnknownContractSchemaVersion { got: version });
    }
    Ok(())
}

// Envelope
pub use envelope::ContractMetadataEnvelope;

// Policy
pub use policy::{ContractCompatibilityPolicy, ContractValidationError};

// Binding
pub use binding::{
    BindingRegistry, BindingStatus, DeferredBinding, DeferredReasonCode, PUBLIC_INPUT_FIELDS,
    PublicInputField, PublicInputs, access_bus_field_names, binding_registry_v1,
};

// Rules
pub use rules::{CONTRACT_RULES_V1, ContractRule, ContractRuleCode};
