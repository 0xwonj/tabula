//! Program binding metadata and access-bus naming helpers.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use tabula_core::Digest;
use tabula_core::execution::NATIVE_MAX_KEY_FES;

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

/// Return access-bus tuple field names for snapshot tests.
pub fn access_bus_field_names(value_width: usize) -> Vec<String> {
    let mut names = vec!["table_id".to_string(), "col_id".to_string()];
    names.extend((0..usize::from(NATIVE_MAX_KEY_FES)).map(|index| format!("key_payload[{index}]")));
    names.push("tx_index".to_string());
    for i in 0..value_width {
        names.push(format!("value[{i}]"));
    }
    names.push("is_null".to_string());
    names
}
