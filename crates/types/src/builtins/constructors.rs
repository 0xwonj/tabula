use tabula_core::PortableValue;
use tabula_profile::{TYPE_BOOL_ID, TYPE_BYTES32_ID, TYPE_I64_ID, TYPE_U64_ID};

use crate::TypedValue;

/// Build a canonical portable `u64` boundary value.
pub fn u64_portable(value: u64) -> PortableValue {
    PortableValue::new(TYPE_U64_ID, borsh::to_vec(&value).expect("u64 portable"))
}

/// Build a canonical portable `i64` boundary value.
pub fn i64_portable(value: i64) -> PortableValue {
    PortableValue::new(TYPE_I64_ID, borsh::to_vec(&value).expect("i64 portable"))
}

/// Build a canonical portable `bool` boundary value.
pub fn bool_portable(value: bool) -> PortableValue {
    PortableValue::new(TYPE_BOOL_ID, borsh::to_vec(&value).expect("bool portable"))
}

/// Build a canonical portable fixed-width `bytes32` boundary value.
pub fn bytes32_portable(value: [u8; 32]) -> PortableValue {
    PortableValue::new(
        TYPE_BYTES32_ID,
        borsh::to_vec(&value).expect("bytes32 portable"),
    )
}

/// Build a runtime typed `u64` value.
pub fn u64_typed(value: u64) -> TypedValue {
    TypedValue::new(TYPE_U64_ID, borsh::to_vec(&value).expect("u64 typed"))
}

/// Build a runtime typed `i64` value.
pub fn i64_typed(value: i64) -> TypedValue {
    TypedValue::new(TYPE_I64_ID, borsh::to_vec(&value).expect("i64 typed"))
}

/// Build a runtime typed `bool` value.
pub fn bool_typed(value: bool) -> TypedValue {
    TypedValue::new(TYPE_BOOL_ID, borsh::to_vec(&value).expect("bool typed"))
}

/// Build a runtime typed fixed-width `bytes32` value.
pub fn bytes32_typed(value: [u8; 32]) -> TypedValue {
    TypedValue::new(
        TYPE_BYTES32_ID,
        borsh::to_vec(&value).expect("bytes32 typed"),
    )
}
