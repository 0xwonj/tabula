//! Statement binding types and registry.

use std::collections::BTreeMap;

use crate::STATEMENT_BINDING_VERSION_V1;
use crate::policy::ContractValidationError;

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
