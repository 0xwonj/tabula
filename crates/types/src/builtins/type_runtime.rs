use std::cmp::Ordering;
use std::sync::Arc;

use borsh::BorshDeserialize;
use tabula_core::PortableValue;
use tabula_core::TypeId;
use tabula_core::error::TabulaError;
use tabula_profile::{TYPE_BOOL_ID, TYPE_BYTES32_ID, TYPE_I64_ID, TYPE_U64_ID, builtin_catalog};

use super::codecs::{
    decode_bool_payload, decode_bytes32_payload, decode_i64_payload, decode_u64_payload,
    ensure_type,
};
use super::constructors::{bool_typed, bytes32_typed, i64_typed, u64_typed};
use super::lookup_type;
use crate::{ArithmeticOp, TypeRuntime, TypedValue};

/// Build the seeded standard built-in type runtime registrations.
pub fn builtin_type_runtimes() -> Result<Vec<Arc<dyn TypeRuntime>>, TabulaError> {
    let catalog = builtin_catalog().map_err(|err| TabulaError::Custom(err.to_string()))?;
    Ok(vec![
        Arc::new(BuiltinTypeRuntime::new(
            TYPE_U64_ID,
            lookup_type(&catalog, TYPE_U64_ID)?,
            BuiltinKind::U64,
        )),
        Arc::new(BuiltinTypeRuntime::new(
            TYPE_I64_ID,
            lookup_type(&catalog, TYPE_I64_ID)?,
            BuiltinKind::I64,
        )),
        Arc::new(BuiltinTypeRuntime::new(
            TYPE_BOOL_ID,
            lookup_type(&catalog, TYPE_BOOL_ID)?,
            BuiltinKind::Bool,
        )),
        Arc::new(BuiltinTypeRuntime::new(
            TYPE_BYTES32_ID,
            lookup_type(&catalog, TYPE_BYTES32_ID)?,
            BuiltinKind::Bytes32,
        )),
    ])
}

#[derive(Debug, Clone, Copy)]
enum BuiltinKind {
    U64,
    I64,
    Bool,
    Bytes32,
}

struct BuiltinTypeRuntime {
    type_id: TypeId,
    descriptor: tabula_profile::TypeDescriptor,
    kind: BuiltinKind,
}

impl BuiltinTypeRuntime {
    fn new(type_id: TypeId, descriptor: tabula_profile::TypeDescriptor, kind: BuiltinKind) -> Self {
        Self {
            type_id,
            descriptor,
            kind,
        }
    }

    fn decode_u64(&self, value: &TypedValue) -> Result<u64, TabulaError> {
        ensure_type(value, self.type_id)?;
        u64::try_from_slice(value.payload()).map_err(|err| TabulaError::Custom(err.to_string()))
    }

    fn decode_i64(&self, value: &TypedValue) -> Result<i64, TabulaError> {
        ensure_type(value, self.type_id)?;
        i64::try_from_slice(value.payload()).map_err(|err| TabulaError::Custom(err.to_string()))
    }

    fn decode_bool(&self, value: &TypedValue) -> Result<bool, TabulaError> {
        ensure_type(value, self.type_id)?;
        bool::try_from_slice(value.payload()).map_err(|err| TabulaError::Custom(err.to_string()))
    }

    fn decode_bytes32(&self, value: &TypedValue) -> Result<[u8; 32], TabulaError> {
        ensure_type(value, self.type_id)?;
        <[u8; 32]>::try_from_slice(value.payload())
            .map_err(|err| TabulaError::Custom(err.to_string()))
    }
}

impl TypeRuntime for BuiltinTypeRuntime {
    fn type_id(&self) -> TypeId {
        self.type_id
    }

    fn descriptor(&self) -> &tabula_profile::TypeDescriptor {
        &self.descriptor
    }

    fn zero_typed(&self) -> TypedValue {
        match self.kind {
            BuiltinKind::U64 => u64_typed(0),
            BuiltinKind::I64 => i64_typed(0),
            BuiltinKind::Bool => bool_typed(false),
            BuiltinKind::Bytes32 => bytes32_typed([0; 32]),
        }
    }

    fn encode_portable(&self, value: &TypedValue) -> Result<PortableValue, TabulaError> {
        self.validate(value)?;
        Ok(PortableValue::new(self.type_id, value.payload().to_vec()))
    }

    fn decode_portable(&self, value: &PortableValue) -> Result<TypedValue, TabulaError> {
        if value.type_id() != self.type_id {
            return Err(TabulaError::TypeMismatch {
                expected: format!("type {}", self.type_id.0),
                actual: format!("type {}", value.type_id().0),
            });
        }
        let typed = TypedValue::new(self.type_id, value.payload().to_vec());
        self.validate(&typed)?;
        Ok(typed)
    }

    fn validate(&self, value: &TypedValue) -> Result<(), TabulaError> {
        match self.kind {
            BuiltinKind::U64 => {
                let _ = decode_u64_payload(value, self.type_id)?;
            }
            BuiltinKind::I64 => {
                let _ = decode_i64_payload(value, self.type_id)?;
            }
            BuiltinKind::Bool => {
                let _ = decode_bool_payload(value, self.type_id)?;
            }
            BuiltinKind::Bytes32 => {
                let _ = decode_bytes32_payload(value, self.type_id)?;
            }
        }
        Ok(())
    }

    fn eq_value(&self, lhs: &TypedValue, rhs: &TypedValue) -> Result<bool, TabulaError> {
        self.validate(lhs)?;
        self.validate(rhs)?;
        Ok(lhs.payload() == rhs.payload())
    }

    fn cmp_value(&self, lhs: &TypedValue, rhs: &TypedValue) -> Result<Ordering, TabulaError> {
        match self.kind {
            BuiltinKind::U64 => Ok(self.decode_u64(lhs)?.cmp(&self.decode_u64(rhs)?)),
            BuiltinKind::I64 => Ok(self.decode_i64(lhs)?.cmp(&self.decode_i64(rhs)?)),
            BuiltinKind::Bool => Ok(self.decode_bool(lhs)?.cmp(&self.decode_bool(rhs)?)),
            BuiltinKind::Bytes32 => Err(TabulaError::TypeMismatch {
                expected: "ordered type".to_string(),
                actual: "Bytes32".to_string(),
            }),
        }
    }

    fn apply_arithmetic(
        &self,
        op: ArithmeticOp,
        lhs: &TypedValue,
        rhs: &TypedValue,
    ) -> Result<TypedValue, TabulaError> {
        match self.kind {
            BuiltinKind::U64 => {
                let lhs = self.decode_u64(lhs)?;
                let rhs = self.decode_u64(rhs)?;
                let value = match op {
                    ArithmeticOp::Add => lhs.checked_add(rhs),
                    ArithmeticOp::Sub => lhs.checked_sub(rhs),
                    ArithmeticOp::Mul => lhs.checked_mul(rhs),
                }
                .ok_or(TabulaError::ArithmeticOverflow)?;
                Ok(u64_typed(value))
            }
            BuiltinKind::I64 => {
                let lhs = self.decode_i64(lhs)?;
                let rhs = self.decode_i64(rhs)?;
                let value = match op {
                    ArithmeticOp::Add => lhs.checked_add(rhs),
                    ArithmeticOp::Sub => lhs.checked_sub(rhs),
                    ArithmeticOp::Mul => lhs.checked_mul(rhs),
                }
                .ok_or(TabulaError::ArithmeticOverflow)?;
                Ok(i64_typed(value))
            }
            BuiltinKind::Bool | BuiltinKind::Bytes32 => Err(TabulaError::TypeMismatch {
                expected: "arithmetic type".to_string(),
                actual: self.descriptor.display_name.clone(),
            }),
        }
    }

    fn divmod(
        &self,
        lhs: &TypedValue,
        rhs: &TypedValue,
    ) -> Result<(TypedValue, TypedValue), TabulaError> {
        match self.kind {
            BuiltinKind::U64 => {
                let lhs = self.decode_u64(lhs)?;
                let rhs = self.decode_u64(rhs)?;
                if rhs == 0 {
                    return Err(TabulaError::DivisionByZero);
                }
                Ok((u64_typed(lhs / rhs), u64_typed(lhs % rhs)))
            }
            BuiltinKind::I64 => {
                let lhs = self.decode_i64(lhs)?;
                let rhs = self.decode_i64(rhs)?;
                if rhs == 0 {
                    return Err(TabulaError::DivisionByZero);
                }
                let quotient = lhs
                    .checked_div(rhs)
                    .ok_or(TabulaError::ArithmeticOverflow)?;
                let remainder = lhs
                    .checked_rem(rhs)
                    .ok_or(TabulaError::ArithmeticOverflow)?;
                Ok((i64_typed(quotient), i64_typed(remainder)))
            }
            BuiltinKind::Bool | BuiltinKind::Bytes32 => Err(TabulaError::TypeMismatch {
                expected: "divmod-capable type".to_string(),
                actual: self.descriptor.display_name.clone(),
            }),
        }
    }

    fn debug_display(&self, value: &TypedValue) -> Result<String, TabulaError> {
        Ok(match self.kind {
            BuiltinKind::U64 => format!("{}u64", self.decode_u64(value)?),
            BuiltinKind::I64 => format!("{}i64", self.decode_i64(value)?),
            BuiltinKind::Bool => format!("{}", self.decode_bool(value)?),
            BuiltinKind::Bytes32 => {
                let bytes = self.decode_bytes32(value)?;
                format!(
                    "0x{:02x}{:02x}{:02x}{:02x}..{:02x}",
                    bytes[0], bytes[1], bytes[2], bytes[3], bytes[31]
                )
            }
        })
    }
}
