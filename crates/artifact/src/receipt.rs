//! Execution receipt and proof summary models.

use serde::{Deserialize, Serialize};

use tabula_core::ExecutionConsistencyStatus;

/// Execution receipt model (non-STARK placeholder backend).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionReceipt {
    /// Receipt version.
    pub version: u32,
    /// Scheme identifier.
    pub scheme: String,
    /// Statement hash.
    pub statement_hash: String,
    /// Program hash.
    pub program_hash: String,
    /// State hash.
    pub state_hash: String,
    /// Batch hash.
    pub batch_hash: String,
    /// Output state hash.
    pub state_after_hash: String,
    /// Metadata hash.
    pub metadata_hash: String,
    /// Generation timestamp.
    pub generated_at_ms: u64,
    /// Number of transactions.
    pub tx_count: usize,
    /// Number of emitted events.
    pub emitted_count: usize,
    /// Consistency status.
    pub consistency: ExecutionConsistencyStatus,
}

/// Summary of one chip in a STARK proof.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChipSummary {
    /// Chip name.
    pub name: String,
    /// Trace height (number of rows).
    pub trace_height: usize,
}

/// Serializable summary of a STARK proof (the actual proof is not serializable).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StarkProofSummary {
    /// Scheme identifier.
    pub scheme: String,
    /// Whether the proof was verified.
    pub verified: bool,
    /// Number of chips.
    pub chip_count: usize,
    /// Per-chip summaries.
    pub chips: Vec<ChipSummary>,
    /// Old state root (8 KoalaBear field elements as hex strings).
    pub old_state_root: Vec<String>,
    /// New state root (8 KoalaBear field elements as hex strings).
    pub new_state_root: Vec<String>,
    /// Proving time in milliseconds.
    pub prove_time_ms: u64,
    /// Verification time in milliseconds.
    pub verify_time_ms: u64,
    /// Statement hash (hex).
    pub statement_hash: String,
    /// Program hash (hex).
    pub program_hash: String,
    /// Batch hash (hex).
    pub batch_hash: String,
}
