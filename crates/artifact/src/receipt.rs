//! Canonical execution statement model.

use serde::{Deserialize, Serialize};

use sha2::{Digest, Sha256};
const STATEMENT_HASH_DOMAIN: &[u8] = b"tabula.execution.statement.v2";

/// Canonical public statement for one execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionStatement {
    /// Program artifact hash.
    pub program_hash: String,
    /// Input state hash.
    pub state_hash: String,
    /// Batch hash.
    pub batch_hash: String,
    /// Output state hash.
    pub state_after_hash: String,
    /// Contract metadata hash.
    pub metadata_hash: String,
    /// AIR public old state root, encoded as 8 hex limbs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub old_state_root: Vec<String>,
    /// AIR public new state root, encoded as 8 hex limbs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub new_state_root: Vec<String>,
}

impl ExecutionStatement {
    /// Compute the canonical statement digest bytes.
    pub fn statement_hash_bytes(&self) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(STATEMENT_HASH_DOMAIN);
        hash_part(&mut hasher, b"program_hash", &self.program_hash);
        hash_part(&mut hasher, b"state_hash", &self.state_hash);
        hash_part(&mut hasher, b"batch_hash", &self.batch_hash);
        hash_part(&mut hasher, b"state_after_hash", &self.state_after_hash);
        hash_part(&mut hasher, b"metadata_hash", &self.metadata_hash);
        hash_list_part(&mut hasher, b"old_state_root", &self.old_state_root);
        hash_list_part(&mut hasher, b"new_state_root", &self.new_state_root);
        hasher.finalize().into()
    }

    /// Compute the canonical statement digest.
    pub fn statement_hash(&self) -> String {
        bytes_to_hex(&self.statement_hash_bytes())
    }
}

fn hash_part(hasher: &mut Sha256, label: &[u8], value: &str) {
    hasher.update(label);
    hasher.update([0u8]);
    hasher.update(value.as_bytes());
    hasher.update([0xffu8]);
}

fn hash_list_part(hasher: &mut Sha256, label: &[u8], values: &[String]) {
    hasher.update(label);
    hasher.update([0u8]);
    for value in values {
        hasher.update(value.as_bytes());
        hasher.update([0xffu8]);
    }
    hasher.update([0xfeu8]);
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        use std::fmt::Write as _;
        let _ = write!(out, "{b:02x}");
    }
    out
}
