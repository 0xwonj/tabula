//! Shared hex codecs for CLI boundaries.

use anyhow::{Context as _, bail};

/// Encode bytes as lowercase hexadecimal without a prefix.
pub(crate) fn encode_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(&mut out, "{byte:02x}");
    }
    out
}

/// Decode a lowercase or `0x`-prefixed hexadecimal string.
pub(crate) fn decode_hex(input: &str) -> anyhow::Result<Vec<u8>> {
    let trimmed = input.trim();
    let hex = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
        .unwrap_or(trimmed);
    if !hex.len().is_multiple_of(2) {
        bail!("hex input must contain an even number of digits");
    }

    let mut bytes = Vec::with_capacity(hex.len() / 2);
    let chars = hex.as_bytes().chunks(2);
    for pair in chars {
        let text = std::str::from_utf8(pair).context("hex input must be valid UTF-8")?;
        let byte =
            u8::from_str_radix(text, 16).with_context(|| format!("invalid hex byte `{text}`"))?;
        bytes.push(byte);
    }
    Ok(bytes)
}
