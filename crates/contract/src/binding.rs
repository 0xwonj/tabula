//! Public input types and binding registry.

use std::collections::BTreeMap;

use tabula_core::{Digest, ProgramBudgets};

use crate::BINDING_VERSION_V1;
use crate::policy::ContractValidationError;

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

/// Default binding registry for schema v1.
pub fn binding_registry_v1() -> BindingRegistry {
    BindingRegistry::new(
        BINDING_VERSION_V1,
        [
            (PublicInputField::OldStateRoot, BindingStatus::BoundInAir),
            (PublicInputField::NewStateRoot, BindingStatus::BoundInAir),
            (
                PublicInputField::ProgramRoot,
                BindingStatus::Deferred(DeferredBinding {
                    reason: DeferredReasonCode::ProgramRootDeferred,
                    owner: "proof",
                    milestone: "M12",
                    expiry: Some("2026-06-30"),
                }),
            ),
            (
                PublicInputField::AppliedTxDigest,
                BindingStatus::Deferred(DeferredBinding {
                    reason: DeferredReasonCode::AppliedTxDigestDeferred,
                    owner: "proof",
                    milestone: "M12",
                    expiry: Some("2026-06-30"),
                }),
            ),
            (
                PublicInputField::StaticTableRoot,
                BindingStatus::Deferred(DeferredBinding {
                    reason: DeferredReasonCode::StaticTableRootDeferred,
                    owner: "proof",
                    milestone: "M12",
                    expiry: Some("2026-06-30"),
                }),
            ),
            (
                PublicInputField::Budgets,
                BindingStatus::Deferred(DeferredBinding {
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
