//! Program artifact model.

use serde::{Deserialize, Serialize};

use tabula_contract::ContractMetadataEnvelope;
use tabula_core::TableSchema;
use tabula_ir::TxTypeDef;

/// Program artifact used by compile/check/execute interfaces.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProgramArtifact {
    /// Table schema definitions.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub table_schemas: Vec<TableSchema>,
    /// Transaction type definitions.
    pub tx_types: Vec<TxTypeDef>,
    /// Optional metadata envelope (required for JSON artifact mode).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contract_metadata: Option<ContractMetadataEnvelope>,
}
