use serde::{Deserialize, Serialize};
use tabula_commitment::NativeDigest;
use tabula_contract::PublicStatement;

/// Stable JSON file contract for `public_statement.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicStatementFile {
    /// Contract version tag.
    pub version: String,
    /// Pre-state root commitment.
    pub old_root_hex: String,
    /// Post-state root commitment.
    pub new_root_hex: String,
    /// Public-context source commitment.
    pub public_context_digest_hex: String,
    /// Transaction-batch source commitment.
    pub applied_tx_digest_hex: String,
    /// Event-log source commitment.
    pub event_digest_hex: String,
}

impl PublicStatementFile {
    /// Stable file contract version for `public_statement.json`.
    pub const VERSION: &str = "tabula.public_statement.v1";

    /// Build the stable file contract from one proved public statement.
    pub fn from_public_statement(public_statement: &PublicStatement) -> Self {
        Self {
            version: Self::VERSION.to_string(),
            old_root_hex: hex_encode(&public_statement.old_root.to_bytes()),
            new_root_hex: hex_encode(&public_statement.new_root.to_bytes()),
            public_context_digest_hex: hex_encode(
                &public_statement.public_context_digest.to_bytes(),
            ),
            applied_tx_digest_hex: hex_encode(&public_statement.applied_tx_digest.to_bytes()),
            event_digest_hex: hex_encode(&public_statement.event_digest.to_bytes()),
        }
    }

    /// Decode and validate one `public_statement.json` payload.
    pub fn from_json_bytes(bytes: &[u8]) -> Result<Self, PublicStatementFileError> {
        let file: Self = serde_json::from_slice(bytes).map_err(|error| {
            PublicStatementFileError::JsonDecode {
                detail: error.to_string(),
            }
        })?;
        file.validate()?;
        Ok(file)
    }

    /// Validate the version tag and digest encoding without changing representation.
    pub fn validate(&self) -> Result<(), PublicStatementFileError> {
        self.validated_digests().map(|_| ())
    }

    /// Reconstruct the proved public statement from the stable file contract.
    pub fn to_public_statement(&self) -> Result<PublicStatement, PublicStatementFileError> {
        let (old_root, new_root, public_context_digest, applied_tx_digest, event_digest) =
            self.validated_digests()?;
        Ok(PublicStatement {
            old_root,
            new_root,
            public_context_digest,
            applied_tx_digest,
            event_digest,
        })
    }

    fn validated_digests(
        &self,
    ) -> Result<
        (
            NativeDigest,
            NativeDigest,
            NativeDigest,
            NativeDigest,
            NativeDigest,
        ),
        PublicStatementFileError,
    > {
        if self.version != Self::VERSION {
            return Err(PublicStatementFileError::UnsupportedVersion {
                got: self.version.clone(),
            });
        }
        Ok((
            parse_native_digest_hex(&self.old_root_hex, "old_root_hex")?,
            parse_native_digest_hex(&self.new_root_hex, "new_root_hex")?,
            parse_native_digest_hex(&self.public_context_digest_hex, "public_context_digest_hex")?,
            parse_native_digest_hex(&self.applied_tx_digest_hex, "applied_tx_digest_hex")?,
            parse_native_digest_hex(&self.event_digest_hex, "event_digest_hex")?,
        ))
    }
}

/// Errors when decoding or validating one `public_statement.json` payload.
#[derive(Debug, thiserror::Error)]
pub enum PublicStatementFileError {
    /// JSON decoding failed.
    #[error("failed to decode public statement JSON: {detail}")]
    JsonDecode {
        /// Decoder error detail from `serde_json`.
        detail: String,
    },
    /// Unsupported version tag.
    #[error("unsupported public statement JSON version {got}")]
    UnsupportedVersion {
        /// Version string found in the decoded file payload.
        got: String,
    },
    /// One field did not contain valid hex.
    #[error("invalid hex in {field}: {detail}")]
    InvalidHex {
        /// Field label that failed validation.
        field: &'static str,
        /// Human-readable validation detail.
        detail: String,
    },
    /// One field did not decode to a valid native digest.
    #[error("invalid native digest bytes in {field}: {detail}")]
    InvalidDigest {
        /// Field label that failed validation.
        field: &'static str,
        /// Human-readable validation detail.
        detail: String,
    },
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

fn parse_native_digest_hex(
    input: &str,
    field: &'static str,
) -> Result<NativeDigest, PublicStatementFileError> {
    let bytes = decode_hex(input, field)?;
    let bytes: [u8; 32] = bytes
        .try_into()
        .map_err(|_| PublicStatementFileError::InvalidHex {
            field,
            detail: "value must decode to exactly 32 bytes".to_string(),
        })?;
    NativeDigest::from_bytes(&bytes).map_err(|error| PublicStatementFileError::InvalidDigest {
        field,
        detail: error.to_string(),
    })
}

fn decode_hex(input: &str, field: &'static str) -> Result<Vec<u8>, PublicStatementFileError> {
    let input = input.as_bytes();
    if !input.len().is_multiple_of(2) {
        return Err(PublicStatementFileError::InvalidHex {
            field,
            detail: "hex input must have even length".to_string(),
        });
    }
    let mut bytes = Vec::with_capacity(input.len() / 2);
    for pair in input.chunks_exact(2) {
        let hi =
            decode_hex_nibble(pair[0]).ok_or_else(|| PublicStatementFileError::InvalidHex {
                field,
                detail: format!("invalid hex byte 0x{:02x}{:02x}", pair[0], pair[1]),
            })?;
        let lo =
            decode_hex_nibble(pair[1]).ok_or_else(|| PublicStatementFileError::InvalidHex {
                field,
                detail: format!("invalid hex byte 0x{:02x}{:02x}", pair[0], pair[1]),
            })?;
        bytes.push((hi << 4) | lo);
    }
    Ok(bytes)
}

fn decode_hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use tabula_commitment::NativeDigest;

    use super::{PublicStatementFile, PublicStatementFileError};

    #[test]
    fn public_statement_file_round_trips() {
        let statement = tabula_contract::PublicStatement {
            old_root: NativeDigest::from_bytes(&[0x11; 32]).expect("old root"),
            new_root: NativeDigest::from_bytes(&[0x22; 32]).expect("new root"),
            public_context_digest: NativeDigest::from_bytes(&[0x33; 32]).expect("context digest"),
            applied_tx_digest: NativeDigest::from_bytes(&[0x44; 32]).expect("tx digest"),
            event_digest: NativeDigest::from_bytes(&[0x55; 32]).expect("event digest"),
        };

        let file = PublicStatementFile::from_public_statement(&statement);
        let json = serde_json::to_vec_pretty(&file).expect("encode statement file");
        let decoded = PublicStatementFile::from_json_bytes(&json).expect("decode statement file");

        assert_eq!(decoded, file);
        assert_eq!(
            decoded
                .to_public_statement()
                .expect("reconstruct statement"),
            statement
        );
    }

    #[test]
    fn public_statement_file_rejects_wrong_version() {
        let err = PublicStatementFile::from_json_bytes(
            br#"{
                "version":"tabula.cli.public_statement.v1",
                "old_root_hex":"1111111111111111111111111111111111111111111111111111111111111111",
                "new_root_hex":"2222222222222222222222222222222222222222222222222222222222222222",
                "public_context_digest_hex":"3333333333333333333333333333333333333333333333333333333333333333",
                "applied_tx_digest_hex":"4444444444444444444444444444444444444444444444444444444444444444",
                "event_digest_hex":"5555555555555555555555555555555555555555555555555555555555555555"
            }"#,
        )
        .expect_err("wrong version must fail");

        assert!(matches!(
            err,
            PublicStatementFileError::UnsupportedVersion { .. }
        ));
    }

    #[test]
    fn public_statement_file_rejects_non_ascii_hex_without_panicking() {
        let json = r#"{
                "version":"tabula.public_statement.v1",
                "old_root_hex":"11111111111111111111111111111111111111111111111111111111111111😀",
                "new_root_hex":"2222222222222222222222222222222222222222222222222222222222222222",
                "public_context_digest_hex":"3333333333333333333333333333333333333333333333333333333333333333",
                "applied_tx_digest_hex":"4444444444444444444444444444444444444444444444444444444444444444",
                "event_digest_hex":"5555555555555555555555555555555555555555555555555555555555555555"
            }"#;
        let err = PublicStatementFile::from_json_bytes(json.as_bytes())
            .expect_err("non-ascii hex must fail");

        assert!(matches!(err, PublicStatementFileError::InvalidHex { .. }));
    }
}
