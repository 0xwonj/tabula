//! Output-side value and type rendering helpers.

use borsh::BorshDeserialize as _;
use tabula_profile::{TYPE_BOOL_ID, TYPE_BYTES32_ID, TYPE_I64_ID, TYPE_U64_ID};
use tabula_sdk::interop::TypeRef;

use crate::io::encode_hex;

use super::ValueOutput;

/// Return a human-friendly type name for one schema type reference.
pub(crate) fn type_name(ty: TypeRef) -> String {
    match ty {
        TYPE_U64_ID => "u64".to_string(),
        TYPE_I64_ID => "i64".to_string(),
        TYPE_BOOL_ID => "bool".to_string(),
        TYPE_BYTES32_ID => "bytes32".to_string(),
        _ => format!("type#{}", ty.0),
    }
}

/// Convert one portable value into the versioned JSON contract representation.
pub(crate) fn value_output(value: &tabula_sdk::interop::PortableValue) -> ValueOutput {
    match value.type_id() {
        TYPE_BOOL_ID => ValueOutput::Bool {
            value: bool::try_from_slice(value.payload()).unwrap_or(false),
        },
        TYPE_U64_ID => ValueOutput::U64 {
            value: u64::try_from_slice(value.payload()).unwrap_or_default(),
        },
        TYPE_I64_ID => ValueOutput::I64 {
            value: i64::try_from_slice(value.payload()).unwrap_or_default(),
        },
        TYPE_BYTES32_ID => {
            let hex = format!("0x{}", encode_hex(value.payload()));
            ValueOutput::Bytes32 { hex }
        }
        type_id => ValueOutput::Portable {
            type_id: type_id.0,
            payload_hex: format!("0x{}", encode_hex(value.payload())),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::value_output;

    #[test]
    fn portable_value_projects_into_json_contract() {
        let value = tabula_sdk::interop::PortableValue::new(
            tabula_profile::TYPE_U64_ID,
            borsh::to_vec(&7u64).unwrap(),
        );
        assert_eq!(
            value_output(&value),
            crate::output::ValueOutput::U64 { value: 7 }
        );
    }
}
