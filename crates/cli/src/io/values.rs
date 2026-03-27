//! CLI input parsing helpers for JSON literals and arrays.

use anyhow::{Context as _, anyhow, bail};
use tabula_profile::{TYPE_BOOL_ID, TYPE_BYTES32_ID, TYPE_I64_ID, TYPE_U64_ID};
use tabula_sdk::interop::TypeRef;

use crate::io::decode_hex;
use crate::output::type_name;

/// Parse a single CLI JSON literal and encode it as a portable value.
pub(crate) fn encode_json_literal(
    raw: &str,
    expected: TypeRef,
) -> anyhow::Result<tabula_sdk::interop::PortableValue> {
    let json: serde_json::Value = serde_json::from_str(raw)
        .with_context(|| format!("failed to parse JSON literal `{raw}`"))?;
    encode_json_value(&json, expected)
}

/// Parse a JSON array and encode each element according to the expected types.
pub(crate) fn encode_json_args(
    raw: &str,
    expected: &[TypeRef],
) -> anyhow::Result<Vec<tabula_sdk::interop::PortableValue>> {
    let json: serde_json::Value =
        serde_json::from_str(raw).with_context(|| format!("failed to parse JSON array `{raw}`"))?;
    let serde_json::Value::Array(items) = json else {
        bail!("expected a JSON array for arguments");
    };
    if items.len() != expected.len() {
        bail!(
            "expected {} argument(s) but received {}",
            expected.len(),
            items.len()
        );
    }
    items
        .iter()
        .zip(expected.iter().copied())
        .map(|(item, ty)| encode_json_value(item, ty))
        .collect()
}

/// Encode one JSON value using the schema's expected type.
pub(crate) fn encode_json_value(
    value: &serde_json::Value,
    expected: TypeRef,
) -> anyhow::Result<tabula_sdk::interop::PortableValue> {
    use tabula_sdk::interop::PortableValue;

    match expected {
        TYPE_BOOL_ID => match value {
            serde_json::Value::Bool(raw) => Ok(PortableValue::new(expected, borsh::to_vec(raw)?)),
            _ => bail!("expected bool for `{}`", type_name(expected)),
        },
        TYPE_U64_ID => match value.as_u64() {
            Some(raw) => Ok(PortableValue::new(expected, borsh::to_vec(&raw)?)),
            None => bail!("expected u64 for `{}`", type_name(expected)),
        },
        TYPE_I64_ID => match value.as_i64() {
            Some(raw) => Ok(PortableValue::new(expected, borsh::to_vec(&raw)?)),
            None => bail!("expected i64 for `{}`", type_name(expected)),
        },
        TYPE_BYTES32_ID => match value {
            serde_json::Value::String(raw) => {
                let bytes = decode_hex(raw).context("failed to parse bytes32 hex string")?;
                let length = bytes.len();
                let array: [u8; 32] = bytes
                    .try_into()
                    .map_err(|_| anyhow!("expected 32 bytes but received {length}"))?;
                Ok(PortableValue::new(expected, borsh::to_vec(&array)?))
            }
            _ => bail!("expected hex string for `{}`", type_name(expected)),
        },
        _ => {
            let serde_json::Value::Object(map) = value else {
                bail!(
                    "unsupported type {} in CLI JSON; pass an object like {{\"type_id\":{},\"payload_hex\":\"0x...\"}}",
                    expected.0,
                    expected.0
                );
            };
            let Some(type_id) = map.get("type_id").and_then(serde_json::Value::as_u64) else {
                bail!("portable value object must include numeric `type_id`");
            };
            if type_id != u64::from(expected.0) {
                bail!(
                    "portable value carries type_id {} but schema expects {}",
                    type_id,
                    expected.0
                );
            }
            let Some(payload_hex) = map.get("payload_hex").and_then(serde_json::Value::as_str)
            else {
                bail!("portable value object must include string `payload_hex`");
            };
            let payload = decode_hex(payload_hex)?;
            Ok(PortableValue::new(expected, payload))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{encode_json_args, encode_json_literal};
    use crate::output::value_output;
    use tabula_profile::{TYPE_BOOL_ID, TYPE_BYTES32_ID, TYPE_I64_ID, TYPE_U64_ID};

    #[test]
    fn parses_builtin_literals() {
        assert_eq!(
            value_output(&encode_json_literal("7", TYPE_U64_ID).unwrap()),
            crate::output::ValueOutputV1::U64 { value: 7 }
        );
        assert_eq!(
            value_output(&encode_json_literal("-2", TYPE_I64_ID).unwrap()),
            crate::output::ValueOutputV1::I64 { value: -2 }
        );
        assert_eq!(
            value_output(&encode_json_literal("true", TYPE_BOOL_ID).unwrap()),
            crate::output::ValueOutputV1::Bool { value: true }
        );
        assert_eq!(
            value_output(
                &encode_json_literal(
                    "\"0x0000000000000000000000000000000000000000000000000000000000000001\"",
                    TYPE_BYTES32_ID
                )
                .unwrap()
            ),
            crate::output::ValueOutputV1::Bytes32 {
                hex: "0x0000000000000000000000000000000000000000000000000000000000000001"
                    .to_string()
            }
        );
    }

    #[test]
    fn parses_argument_arrays() {
        let values = encode_json_args("[1, true]", &[TYPE_U64_ID, TYPE_BOOL_ID]).unwrap();
        assert_eq!(values.len(), 2);
    }
}
