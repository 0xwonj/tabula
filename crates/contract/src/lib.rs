//! Proof contract metadata, versioning, binding policy, and proof-visible
//! format schema.
//!
//! This module is the canonical contract source of truth.

mod binding;
mod compatibility;
mod error;
pub mod format;
mod metadata_envelope;
mod proof_envelope;
pub mod public_statement;
mod sealed_relation_policy;
mod verification;
mod versions;

// Envelope
pub use error::ProofContractError;
pub use metadata_envelope::ContractMetadataEnvelope;
pub use proof_envelope::{
    ProofEncodingId, ProofEnvelope, ProofSystemId, decode_proof_envelope, encode_proof_envelope,
};
pub use verification::{ArtifactContext, BoundStatement};

// Compatibility
pub use compatibility::{
    CONTRACT_RULES, ContractCompatibilityPolicy, ContractRule, ContractRuleCode,
    ContractValidationError,
};

// Binding
pub use binding::{ProgramBinding, access_bus_field_names};
pub use format::static_tables::{StaticTableArtifact, StaticTableArtifactRow};
pub use format::typed_tuple::{TupleEncodingDefaults, TupleEncodingSelection};
pub use public_statement::{PublicStatement, PublicStatementError};

// Sealed policy types
pub use sealed_relation_policy::SealedRelationPolicy;

// Versions
pub use versions::{
    CONTRACT_SCHEMA_VERSION, PROOF_ENVELOPE_VERSION, STATEMENT_SCHEMA_VERSION,
    VERIFIER_PROFILE_VERSION, validate_contract_schema_version, validate_proof_envelope_version,
    validate_statement_schema_version, validate_verifier_profile_version,
};
