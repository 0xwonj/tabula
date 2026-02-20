//! Proof contract metadata, versioning, and binding policy.
//!
//! This module is the M11 "Contract Spine V1" source of truth.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Current contract schema version.
pub const CONTRACT_SCHEMA_VERSION_V1: u32 = 1;
/// Current statement binding registry version.
pub const STATEMENT_BINDING_VERSION_V1: u32 = 1;
/// C10 ReadAccess bus schema version (v2 includes `tx_index`).
pub const C10_READ_ACCESS_SCHEMA_VERSION_V2: u32 = 2;
/// C11 WriteAccess bus schema version (v2 includes `tx_index`).
pub const C11_WRITE_ACCESS_SCHEMA_VERSION_V2: u32 = 2;

const METADATA_MAGIC: [u8; 4] = *b"TCME";
const METADATA_SERIALIZATION_VERSION: u8 = 1;
const METADATA_HASH_DOMAIN: &[u8] = b"tabula.contract_metadata_envelope.v1";

/// Canonical metadata envelope used for proof compatibility checks.
///
/// Field order, binary encoding, and hashing are fixed by
/// [`ContractMetadataEnvelope::to_canonical_bytes`] and
/// [`ContractMetadataEnvelope::canonical_hash`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractMetadataEnvelope {
    /// Compiler/runtime profile fingerprint.
    pub profile_hash: [u8; 32],
    /// Contract schema version.
    pub contract_schema_version: u32,
    /// Statement binding registry version.
    pub statement_binding_version: u32,
    /// Optional semantic hash stub (reserved for staged rollout).
    pub semantic_hash_stub: Option<[u8; 32]>,
}

impl ContractMetadataEnvelope {
    /// Serialize to canonical binary format.
    ///
    /// Encoding:
    /// 1. magic (`TCME`, 4 bytes)
    /// 2. serialization version (u8)
    /// 3. `profile_hash` (32 bytes)
    /// 4. `contract_schema_version` (u32 big-endian)
    /// 5. `statement_binding_version` (u32 big-endian)
    /// 6. semantic flag (u8; 0/1)
    /// 7. `semantic_hash_stub` (32 bytes, only if flag=1)
    pub fn to_canonical_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(78);
        out.extend_from_slice(&METADATA_MAGIC);
        out.push(METADATA_SERIALIZATION_VERSION);
        out.extend_from_slice(&self.profile_hash);
        out.extend_from_slice(&self.contract_schema_version.to_be_bytes());
        out.extend_from_slice(&self.statement_binding_version.to_be_bytes());
        match self.semantic_hash_stub {
            Some(hash) => {
                out.push(1);
                out.extend_from_slice(&hash);
            }
            None => out.push(0),
        }
        out
    }

    /// Hash the canonical bytes with fixed domain separation.
    pub fn canonical_hash(&self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(METADATA_HASH_DOMAIN);
        hasher.update(&self.to_canonical_bytes());
        *hasher.finalize().as_bytes()
    }
}

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

/// Validate contract schema version with fail-closed policy.
pub fn validate_contract_schema_version(version: u32) -> Result<(), ContractValidationError> {
    if version != CONTRACT_SCHEMA_VERSION_V1 {
        return Err(ContractValidationError::UnknownContractSchemaVersion { got: version });
    }
    Ok(())
}

/// ApplyBatch statement fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ApplyBatchField {
    /// `old_state_root`.
    OldStateRoot,
    /// `new_state_root`.
    NewStateRoot,
    /// `program_root`.
    ProgramRoot,
    /// `applied_tx_digest`.
    AppliedTxDigest,
    /// `static_table_root`.
    StaticTableRoot,
    /// `budgets`.
    Budgets,
}

/// Ordered list of fields required in the binding registry.
pub const APPLY_BATCH_FIELDS: [ApplyBatchField; 6] = [
    ApplyBatchField::OldStateRoot,
    ApplyBatchField::NewStateRoot,
    ApplyBatchField::ProgramRoot,
    ApplyBatchField::AppliedTxDigest,
    ApplyBatchField::StaticTableRoot,
    ApplyBatchField::Budgets,
];

/// Deferred reason codes (free-text is forbidden).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeferredReasonCode {
    /// Program root binding is pending.
    ProgramRootDeferred,
    /// Applied tx digest binding is pending.
    AppliedTxDigestDeferred,
    /// Static table root binding is pending.
    StaticTableRootDeferred,
    /// Budgets binding is pending.
    BudgetsDeferred,
}

/// Deferred binding metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeferredBinding {
    /// Stable reason code.
    pub reason: DeferredReasonCode,
    /// Owning module/crate.
    pub owner: &'static str,
    /// Target milestone.
    pub milestone: &'static str,
    /// Optional expiry/deadline marker.
    pub expiry: Option<&'static str>,
}

/// Statement binding state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatementBindingStatus {
    /// Public input is bound by AIR.
    BoundInAir,
    /// Public input is intentionally deferred with governance metadata.
    Deferred(DeferredBinding),
}

/// Registry of statement binding states.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatementBindingRegistry {
    /// Registry version.
    pub version: u32,
    /// Field binding map.
    pub bindings: BTreeMap<ApplyBatchField, StatementBindingStatus>,
}

impl StatementBindingRegistry {
    /// Build a registry from explicit entries.
    pub fn new<I>(version: u32, bindings: I) -> Self
    where
        I: IntoIterator<Item = (ApplyBatchField, StatementBindingStatus)>,
    {
        Self {
            version,
            bindings: bindings.into_iter().collect(),
        }
    }

    /// Enforce completeness: every `ApplyBatchStatement` field must be bound or deferred.
    pub fn validate_completeness(&self) -> Result<(), ContractValidationError> {
        let missing_fields = APPLY_BATCH_FIELDS
            .iter()
            .copied()
            .filter(|field| !self.bindings.contains_key(field))
            .collect::<Vec<_>>();
        if missing_fields.is_empty() {
            Ok(())
        } else {
            Err(ContractValidationError::IncompleteStatementBinding { missing_fields })
        }
    }
}

/// Default ApplyBatch binding registry for schema v1.
pub fn apply_batch_binding_registry_v1() -> StatementBindingRegistry {
    StatementBindingRegistry::new(
        STATEMENT_BINDING_VERSION_V1,
        [
            (
                ApplyBatchField::OldStateRoot,
                StatementBindingStatus::BoundInAir,
            ),
            (
                ApplyBatchField::NewStateRoot,
                StatementBindingStatus::BoundInAir,
            ),
            (
                ApplyBatchField::ProgramRoot,
                StatementBindingStatus::Deferred(DeferredBinding {
                    reason: DeferredReasonCode::ProgramRootDeferred,
                    owner: "proof",
                    milestone: "M12",
                    expiry: Some("2026-06-30"),
                }),
            ),
            (
                ApplyBatchField::AppliedTxDigest,
                StatementBindingStatus::Deferred(DeferredBinding {
                    reason: DeferredReasonCode::AppliedTxDigestDeferred,
                    owner: "proof",
                    milestone: "M12",
                    expiry: Some("2026-06-30"),
                }),
            ),
            (
                ApplyBatchField::StaticTableRoot,
                StatementBindingStatus::Deferred(DeferredBinding {
                    reason: DeferredReasonCode::StaticTableRootDeferred,
                    owner: "proof",
                    milestone: "M12",
                    expiry: Some("2026-06-30"),
                }),
            ),
            (
                ApplyBatchField::Budgets,
                StatementBindingStatus::Deferred(DeferredBinding {
                    reason: DeferredReasonCode::BudgetsDeferred,
                    owner: "proof",
                    milestone: "M12",
                    expiry: Some("2026-06-30"),
                }),
            ),
        ],
    )
}

/// Return C10/C11 v2 tuple field names for snapshot tests.
pub fn access_bus_field_names(value_width: usize) -> Vec<String> {
    let mut names = vec![
        "table_id".to_string(),
        "col_id".to_string(),
        "key_limb0".to_string(),
        "key_limb1".to_string(),
        "key_limb2".to_string(),
        "tx_index".to_string(),
    ];
    for i in 0..value_width {
        names.push(format!("value[{i}]"));
    }
    names.push("is_null".to_string());
    names
}

/// Contract rule identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContractRuleCode {
    /// Com_new is only materialized when the new set is non-empty.
    ComNewRequiresNonEmptyNewSet,
}

/// Contract rule metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContractRule {
    /// Rule code.
    pub code: ContractRuleCode,
    /// Rule summary.
    pub description: &'static str,
}

/// Rules required in schema v1.
pub const CONTRACT_RULES_V1: [ContractRule; 1] = [ContractRule {
    code: ContractRuleCode::ComNewRequiresNonEmptyNewSet,
    description: "C6 Com_new multiplicity is gated by is_touched * (1 - is_empty_new) in ColumnMeta",
}];
