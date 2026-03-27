//! Public input types and binding registry.

use std::collections::BTreeMap;

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use tabula_core::{Digest, ProgramBudgets};

use crate::BINDING_REGISTRY_VERSION;
use crate::compatibility::ContractValidationError;

/// Public inputs for the ApplyBatch proof.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicInputs {
    /// The state root before batch execution.
    pub old_state_root: Digest,
    /// The state root after batch execution.
    pub new_state_root: Digest,
    /// Commitment to the program (set of tx type definitions).
    pub program_root: Digest,
    /// Commitment to the batch of applied transactions.
    pub applied_tx_digest: Digest,
    /// Commitment to the static lookup tables.
    pub static_table_root: Digest,
    /// Program resource budgets (DoS prevention).
    pub budgets: ProgramBudgets,
}

/// Canonical binding for one sealed program artifact plus contract metadata.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ProgramBinding {
    program_hash: Digest,
    metadata_hash: Digest,
}

impl ProgramBinding {
    /// Build one binding from canonical artifact and metadata hashes.
    pub const fn new(program_hash: Digest, metadata_hash: Digest) -> Self {
        Self {
            program_hash,
            metadata_hash,
        }
    }

    /// Canonical digest of the sealed artifact backing this binding.
    pub const fn program_hash(&self) -> &Digest {
        &self.program_hash
    }

    /// Canonical digest of the contract metadata backing this binding.
    pub const fn metadata_hash(&self) -> &Digest {
        &self.metadata_hash
    }

    /// Canonical digest of the sealed artifact backing this binding as lowercase hex.
    pub fn program_hash_hex(&self) -> String {
        hex_encode(&self.program_hash)
    }

    /// Canonical digest of the contract metadata backing this binding as lowercase hex.
    pub fn metadata_hash_hex(&self) -> String {
        hex_encode(&self.metadata_hash)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProgramBindingJson {
    program_hash: String,
    metadata_hash: String,
}

impl Serialize for ProgramBinding {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        ProgramBindingJson {
            program_hash: self.program_hash_hex(),
            metadata_hash: self.metadata_hash_hex(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ProgramBinding {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let json = ProgramBindingJson::deserialize(deserializer)?;
        Ok(Self {
            program_hash: decode_hex_digest(&json.program_hash)
                .map_err(serde::de::Error::custom)?,
            metadata_hash: decode_hex_digest(&json.metadata_hash)
                .map_err(serde::de::Error::custom)?,
        })
    }
}

fn hex_encode(bytes: &Digest) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

fn decode_hex_digest(input: &str) -> Result<Digest, String> {
    if input.len() != 64 {
        return Err(format!(
            "expected 64 lowercase hex characters, got length {}",
            input.len()
        ));
    }
    if input.as_bytes().iter().any(u8::is_ascii_uppercase) {
        return Err("expected lowercase hex characters".to_string());
    }
    let mut out = [0u8; 32];
    for (index, chunk) in input.as_bytes().chunks_exact(2).enumerate() {
        let chunk = std::str::from_utf8(chunk).map_err(|error| error.to_string())?;
        out[index] = u8::from_str_radix(chunk, 16)
            .map_err(|error| format!("invalid hex digest byte at offset {}: {error}", index * 2))?;
    }
    Ok(out)
}

/// Public input fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PublicInputField {
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
pub const PUBLIC_INPUT_FIELDS: [PublicInputField; 6] = [
    PublicInputField::OldStateRoot,
    PublicInputField::NewStateRoot,
    PublicInputField::ProgramRoot,
    PublicInputField::AppliedTxDigest,
    PublicInputField::StaticTableRoot,
    PublicInputField::Budgets,
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

/// Binding status for a public input field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingStatus {
    /// Public input is bound by AIR.
    BoundInAir,
    /// Public input is bound by the higher-level semantic statement digest in the transcript.
    BoundInTranscript,
    /// Public input is intentionally deferred with governance metadata.
    Deferred(DeferredBinding),
}

/// Registry of public input binding states.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindingRegistry {
    /// Registry version.
    pub version: u32,
    /// Field binding map.
    pub bindings: BTreeMap<PublicInputField, BindingStatus>,
}

impl BindingRegistry {
    /// Build a registry from explicit entries.
    pub fn new<I>(version: u32, bindings: I) -> Self
    where
        I: IntoIterator<Item = (PublicInputField, BindingStatus)>,
    {
        Self {
            version,
            bindings: bindings.into_iter().collect(),
        }
    }

    /// Enforce completeness: every `PublicInputs` field must be bound or deferred.
    pub fn validate_completeness(&self) -> Result<(), ContractValidationError> {
        let missing_fields = PUBLIC_INPUT_FIELDS
            .iter()
            .copied()
            .filter(|field| !self.bindings.contains_key(field))
            .collect::<Vec<_>>();
        if missing_fields.is_empty() {
            Ok(())
        } else {
            Err(ContractValidationError::IncompleteBinding { missing_fields })
        }
    }
}

/// Default binding registry for the current contract schema.
pub fn binding_registry() -> BindingRegistry {
    BindingRegistry::new(
        BINDING_REGISTRY_VERSION,
        [
            (PublicInputField::OldStateRoot, BindingStatus::BoundInAir),
            (PublicInputField::NewStateRoot, BindingStatus::BoundInAir),
            (
                PublicInputField::ProgramRoot,
                BindingStatus::BoundInTranscript,
            ),
            (
                PublicInputField::AppliedTxDigest,
                BindingStatus::BoundInTranscript,
            ),
            (
                PublicInputField::StaticTableRoot,
                BindingStatus::BoundInTranscript,
            ),
            (
                PublicInputField::Budgets,
                BindingStatus::Deferred(DeferredBinding {
                    reason: DeferredReasonCode::BudgetsDeferred,
                    owner: "proof",
                    milestone: "M13",
                    expiry: Some("2026-09-30"),
                }),
            ),
        ],
    )
}

/// Return access-bus tuple field names for snapshot tests.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binding_registry_is_complete_and_marks_transcript_fields() {
        let registry = binding_registry();
        registry
            .validate_completeness()
            .expect("binding registry should be complete");
        assert_eq!(
            registry.bindings.get(&PublicInputField::OldStateRoot),
            Some(&BindingStatus::BoundInAir)
        );
        assert_eq!(
            registry.bindings.get(&PublicInputField::NewStateRoot),
            Some(&BindingStatus::BoundInAir)
        );
        assert_eq!(
            registry.bindings.get(&PublicInputField::ProgramRoot),
            Some(&BindingStatus::BoundInTranscript)
        );
        assert_eq!(
            registry.bindings.get(&PublicInputField::AppliedTxDigest),
            Some(&BindingStatus::BoundInTranscript)
        );
        assert_eq!(
            registry.bindings.get(&PublicInputField::StaticTableRoot),
            Some(&BindingStatus::BoundInTranscript)
        );
        assert!(matches!(
            registry.bindings.get(&PublicInputField::Budgets),
            Some(BindingStatus::Deferred(_))
        ));
    }
}
