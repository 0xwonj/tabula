//! Runtime type and encoding behavior for profile-defined types.
//!
//! This crate owns the process-local behavior layer:
//! - internal typed value carriers
//! - runtime type semantics
//! - runtime encoding semantics
//! - seeded built-in runtime registrations

/// Built-in constructors and low-level seeded built-in helpers.
pub mod builtins;
mod encoding_runtime;
mod helpers;
mod key_codec;
mod type_runtime;
mod value;

pub use builtins::{
    bool_portable, bool_typed, builtin_encoding_runtimes, builtin_type_runtimes, bytes32_portable,
    bytes32_typed, i64_portable, i64_typed, u64_portable, u64_typed,
};
pub use encoding_runtime::{EncodingRuntime, EncodingRuntimeRegistry, OrderedKeySegmentKind};
pub use helpers::{
    decode_trace_with_null_flag, encode_structural_u64, encode_trace_with_null_flag,
    encode_value_with_null_flag, typed_bool,
};
pub use key_codec::{
    KeyComponentPayloadLayout, NATIVE_KEY_PAYLOAD_WIDTH, NativeKeyPayload, TableKeyCodec,
    zero_key_payload,
};
pub use type_runtime::{ArithmeticOp, TypeRuntime, TypeRuntimeRegistry};
pub use value::{
    CommittedColumnEntry, TypedCommittedPropertyQueryResult, TypedLogicalStateCell, TypedValue,
};
