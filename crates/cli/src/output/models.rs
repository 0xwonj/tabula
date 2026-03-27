//! Versioned JSON contracts exposed by the CLI.

use serde::{Deserialize, Serialize};

/// Stable JSON contract for `tabula check --json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckOutputV1 {
    /// Contract version tag.
    pub version: String,
    /// Whether the input was source or artifact.
    pub input_kind: String,
    /// Canonical artifact digest.
    pub artifact_digest: String,
    /// Declared state table names.
    pub tables: Vec<String>,
    /// Declared transaction names.
    pub transactions: Vec<String>,
    /// Declared query names.
    pub queries: Vec<String>,
    /// Declared public context field names.
    pub context_fields: Vec<String>,
}

/// Stable JSON contract for `tabula schema --json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchemaOutputV1 {
    /// Contract version tag.
    pub version: String,
    /// Canonical artifact digest.
    pub artifact_digest: String,
    /// State tables in declaration order.
    pub tables: Vec<TableOutputV1>,
    /// Transactions in declaration order.
    pub transactions: Vec<EntryOutputV1>,
    /// Queries in declaration order.
    pub queries: Vec<QueryOutputV1>,
    /// Public context fields in declaration order.
    pub context_fields: Vec<NamedTypeOutputV1>,
}

/// Stable JSON contract for `tabula query --json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryRunOutputV1 {
    /// Contract version tag.
    pub version: String,
    /// Canonical artifact digest.
    pub artifact_digest: String,
    /// Query symbol as requested by the caller.
    pub query: String,
    /// Returned values in declaration order.
    pub returns: Vec<ValueOutputV1>,
}

/// Stable JSON contract for `tabula execute --json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionReportV1 {
    /// Contract version tag.
    pub version: String,
    /// Canonical artifact digest.
    pub artifact_digest: String,
    /// Per-transaction outcomes in batch order.
    pub outcomes: Vec<TxOutcomeOutputV1>,
    /// Number of distinct state cells read from pre-state.
    pub read_count: usize,
    /// Number of distinct state cells written into post-state.
    pub write_count: usize,
    /// Final committed state snapshot rendered for automation.
    pub state_after: StateInspectOutputV1,
}

/// Stable JSON contract for `tabula state inspect --json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateInspectOutputV1 {
    /// Contract version tag.
    pub version: String,
    /// Number of rendered cells.
    pub cell_count: usize,
    /// Rendered cells in canonical order.
    pub cells: Vec<StateCellOutputV1>,
}

/// Stable JSON contract for `tabula env doctor --json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvDoctorOutputV1 {
    /// Contract version tag.
    pub version: String,
    /// Resolved config file path, when one was loaded.
    pub config_path: Option<String>,
    /// Whether the SDK environment could be constructed successfully.
    pub sdk_ready: bool,
    /// Build failure detail when the environment is not usable.
    pub build_error: Option<String>,
    /// Parsed extension bundle reports.
    pub extensions: Vec<ExtensionBundleOutputV1>,
    /// Whether this build enables verifier support.
    pub verify_feature_enabled: bool,
    /// Whether this build enables prover support.
    pub prove_feature_enabled: bool,
}

/// Stable JSON contract for `tabula prove --json`.
#[cfg(feature = "prove")]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProveOutputV1 {
    /// Contract version tag.
    pub version: String,
    /// Canonical artifact digest.
    pub artifact_digest: String,
    /// Statement digest bound into the proof transcript.
    pub statement_hash_hex: String,
    /// Proof system identifier.
    pub proof_system: String,
    /// Proof encoding identifier.
    pub proof_encoding: String,
    /// Total number of chips present in the proof summary.
    pub chip_count: usize,
}

/// Stable JSON contract for `tabula verify --json`.
#[cfg(feature = "verify")]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifyOutputV1 {
    /// Contract version tag.
    pub version: String,
    /// Canonical artifact digest.
    pub artifact_digest: String,
    /// Statement digest bound into the proof transcript.
    pub statement_hash_hex: String,
    /// Whether verification succeeded.
    pub verified: bool,
}

/// One named schema value carrying a type reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamedTypeOutputV1 {
    /// Source-level symbol.
    pub symbol: String,
    /// Stable type metadata.
    pub ty: TypeOutputV1,
}

/// One field schema in a state table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableFieldOutputV1 {
    /// Stable numeric field identifier.
    pub id: u32,
    /// Source-level field symbol.
    pub symbol: String,
    /// Stable type metadata.
    pub ty: TypeOutputV1,
}

/// One state table schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableOutputV1 {
    /// Stable numeric table identifier.
    pub id: u32,
    /// Source-level table symbol.
    pub symbol: String,
    /// Number of key columns.
    pub key_arity: usize,
    /// Declared fields in source order.
    pub fields: Vec<TableFieldOutputV1>,
}

/// One transaction entry schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntryOutputV1 {
    /// Stable numeric entry identifier.
    pub id: u32,
    /// Source-level entry symbol.
    pub symbol: String,
    /// Parameters in declaration order.
    pub params: Vec<NamedTypeOutputV1>,
}

/// One query entry schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryOutputV1 {
    /// Stable numeric entry identifier.
    pub id: u32,
    /// Source-level entry symbol.
    pub symbol: String,
    /// Parameters in declaration order.
    pub params: Vec<NamedTypeOutputV1>,
    /// Return values in declaration order.
    pub returns: Vec<TypeOutputV1>,
}

/// Stable type metadata used by CLI JSON output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypeOutputV1 {
    /// Stable numeric type identifier.
    pub id: u32,
    /// Human-friendly display name when known.
    pub display: String,
}

/// Structured CLI rendering of one portable value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ValueOutputV1 {
    /// Boolean value.
    Bool { value: bool },
    /// Unsigned 64-bit integer.
    U64 { value: u64 },
    /// Signed 64-bit integer.
    I64 { value: i64 },
    /// Fixed 32-byte digest rendered as hex.
    Bytes32 { hex: String },
    /// Fallback portable payload for unsupported or unknown runtime types.
    Portable { type_id: u32, payload_hex: String },
}

/// One rendered state cell.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateCellOutputV1 {
    /// Numeric table identifier.
    pub table_id: u32,
    /// Source-level table symbol when available.
    pub table: Option<String>,
    /// Concrete row key.
    pub row: u64,
    /// Numeric field identifier.
    pub field_id: u32,
    /// Source-level field symbol when available.
    pub field: Option<String>,
    /// Rendered cell value.
    pub value: ValueOutputV1,
}

/// One executed transaction outcome.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TxOutcomeOutputV1 {
    /// Batch-local transaction index.
    pub tx_index: u32,
    /// Numeric entry identifier.
    pub entry_id: u32,
    /// Source-level transaction symbol when available.
    pub entry: Option<String>,
    /// Outcome status.
    pub status: TxOutcomeStatusV1,
    /// Number of state effects on success.
    pub state_effect_count: usize,
    /// Number of event effects on success.
    pub event_effect_count: usize,
    /// Number of capability effects on success.
    pub capability_effect_count: usize,
    /// Number of relation effects on success.
    pub relation_effect_count: usize,
}

/// Success/failure discriminator for one executed transaction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TxOutcomeStatusV1 {
    /// Transaction completed successfully.
    Success,
    /// Transaction failed with an optional failing op index.
    Failed {
        /// Human-readable failure reason.
        reason: String,
        /// Failing operation index when the runtime reported one.
        failed_op_index: Option<usize>,
    },
}

/// One parsed extension bundle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionBundleOutputV1 {
    /// Absolute bundle path.
    pub path: String,
    /// Human-readable bundle name.
    pub name: String,
    /// Capability paths contributed by this bundle.
    pub capability_paths: Vec<String>,
    /// Unsupported declarative sections discovered in the bundle.
    pub unsupported_entries: Vec<String>,
}
