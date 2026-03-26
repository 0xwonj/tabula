//! Proof contract metadata, versioning, binding policy, and proof-visible
//! format schema.
//!
//! This module is the M11 "Contract Spine V1" source of truth.

mod binding;
mod compatibility;
mod envelope;
pub mod format;
mod versions;

// Envelope
pub use envelope::ContractMetadataEnvelope;

// Compatibility
pub use compatibility::{
    CONTRACT_RULES_V1, ContractCompatibilityPolicy, ContractRule, ContractRuleCode,
    ContractValidationError,
};

// Binding
pub use binding::{
    BindingRegistry, BindingStatus, DeferredBinding, DeferredReasonCode, PUBLIC_INPUT_FIELDS,
    ProgramBinding, PublicInputField, PublicInputs, access_bus_field_names, binding_registry,
};
pub use format::static_tables::{StaticTableArtifact, StaticTableArtifactRow};
pub use format::typed_tuple::{TupleEncodingDefaults, TupleEncodingSelection};

// Versions
pub use versions::{
    BINDING_REGISTRY_VERSION, CONTRACT_SCHEMA_VERSION, STATEMENT_SCHEMA_VERSION,
    VERIFIER_PROFILE_VERSION, validate_binding_registry_version, validate_contract_schema_version,
    validate_statement_schema_version, validate_verifier_profile_version,
};
