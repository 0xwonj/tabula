//! Contract rule identifiers and constants.

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
    description:
        "C6 Com_new multiplicity is gated by is_touched * (1 - is_empty_new) in ColumnMeta",
}];
