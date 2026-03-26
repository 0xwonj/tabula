use anyhow::anyhow;

use tabula_core::PortableValue;
use tabula_ir as ir;
use tabula_lang::hir;
use tabula_profile::{TYPE_BOOL_ID, TYPE_I64_ID, TYPE_U64_ID};

use crate::error::CompilerError;

pub(crate) fn eval_const_expr(expr: &hir::ConstExpr) -> Result<PortableValue, CompilerError> {
    match expr {
        hir::ConstExpr::Literal(value) => Ok(value.clone()),
        hir::ConstExpr::Unary { op, expr } => {
            let value = eval_const_expr(expr)?;
            match op {
                hir::UnaryOp::Not => {
                    let flag = decode_bool(&value)?;
                    Ok(portable_bool(!flag))
                }
                hir::UnaryOp::Neg => {
                    let number = decode_i64(&value)?;
                    Ok(portable_i64(-number))
                }
            }
        }
        hir::ConstExpr::Binary { op, lhs, rhs } => {
            let lhs = eval_const_expr(lhs)?;
            let rhs = eval_const_expr(rhs)?;
            eval_const_binary(*op, &lhs, &rhs)
        }
    }
}

pub(crate) fn eval_const_binary(
    op: hir::BinaryOp,
    lhs: &PortableValue,
    rhs: &PortableValue,
) -> Result<PortableValue, CompilerError> {
    match op {
        hir::BinaryOp::Add => {
            if lhs.type_id() == TYPE_U64_ID {
                Ok(portable_u64(decode_u64(lhs)? + decode_u64(rhs)?))
            } else {
                Ok(portable_i64(decode_i64(lhs)? + decode_i64(rhs)?))
            }
        }
        hir::BinaryOp::Sub => {
            if lhs.type_id() == TYPE_U64_ID {
                Ok(portable_u64(decode_u64(lhs)? - decode_u64(rhs)?))
            } else {
                Ok(portable_i64(decode_i64(lhs)? - decode_i64(rhs)?))
            }
        }
        hir::BinaryOp::Mul => {
            if lhs.type_id() == TYPE_U64_ID {
                Ok(portable_u64(decode_u64(lhs)? * decode_u64(rhs)?))
            } else {
                Ok(portable_i64(decode_i64(lhs)? * decode_i64(rhs)?))
            }
        }
        hir::BinaryOp::Div => {
            if lhs.type_id() == TYPE_U64_ID {
                Ok(portable_u64(decode_u64(lhs)? / decode_u64(rhs)?))
            } else {
                Ok(portable_i64(decode_i64(lhs)? / decode_i64(rhs)?))
            }
        }
        hir::BinaryOp::Mod => {
            if lhs.type_id() == TYPE_U64_ID {
                Ok(portable_u64(decode_u64(lhs)? % decode_u64(rhs)?))
            } else {
                Ok(portable_i64(decode_i64(lhs)? % decode_i64(rhs)?))
            }
        }
        hir::BinaryOp::Eq => Ok(portable_bool(lhs == rhs)),
        hir::BinaryOp::Ne => Ok(portable_bool(lhs != rhs)),
        hir::BinaryOp::Lt => {
            if lhs.type_id() == TYPE_U64_ID {
                Ok(portable_bool(decode_u64(lhs)? < decode_u64(rhs)?))
            } else {
                Ok(portable_bool(decode_i64(lhs)? < decode_i64(rhs)?))
            }
        }
        hir::BinaryOp::Le => {
            if lhs.type_id() == TYPE_U64_ID {
                Ok(portable_bool(decode_u64(lhs)? <= decode_u64(rhs)?))
            } else {
                Ok(portable_bool(decode_i64(lhs)? <= decode_i64(rhs)?))
            }
        }
        hir::BinaryOp::Gt => {
            if lhs.type_id() == TYPE_U64_ID {
                Ok(portable_bool(decode_u64(lhs)? > decode_u64(rhs)?))
            } else {
                Ok(portable_bool(decode_i64(lhs)? > decode_i64(rhs)?))
            }
        }
        hir::BinaryOp::Ge => {
            if lhs.type_id() == TYPE_U64_ID {
                Ok(portable_bool(decode_u64(lhs)? >= decode_u64(rhs)?))
            } else {
                Ok(portable_bool(decode_i64(lhs)? >= decode_i64(rhs)?))
            }
        }
        hir::BinaryOp::And => Ok(portable_bool(decode_bool(lhs)? && decode_bool(rhs)?)),
        hir::BinaryOp::Or => Ok(portable_bool(decode_bool(lhs)? || decode_bool(rhs)?)),
    }
}

pub(crate) fn decode_u64(value: &PortableValue) -> Result<u64, CompilerError> {
    if value.type_id() != TYPE_U64_ID {
        return Err(invalid("expected u64 portable value"));
    }
    let bytes: [u8; 8] = value
        .payload()
        .try_into()
        .map_err(|_| invalid("invalid u64 payload"))?;
    Ok(u64::from_le_bytes(bytes))
}

pub(crate) fn decode_i64(value: &PortableValue) -> Result<i64, CompilerError> {
    if value.type_id() != TYPE_I64_ID {
        return Err(invalid("expected i64 portable value"));
    }
    let bytes: [u8; 8] = value
        .payload()
        .try_into()
        .map_err(|_| invalid("invalid i64 payload"))?;
    Ok(i64::from_le_bytes(bytes))
}

pub(crate) fn decode_bool(value: &PortableValue) -> Result<bool, CompilerError> {
    if value.type_id() != TYPE_BOOL_ID {
        return Err(invalid("expected bool portable value"));
    }
    match value.payload() {
        [0] => Ok(false),
        [1] => Ok(true),
        _ => Err(invalid("invalid bool payload")),
    }
}

pub(crate) fn portable_u64(value: u64) -> PortableValue {
    PortableValue::new(TYPE_U64_ID, value.to_le_bytes().to_vec())
}

pub(crate) fn portable_i64(value: i64) -> PortableValue {
    PortableValue::new(TYPE_I64_ID, value.to_le_bytes().to_vec())
}

pub(crate) fn portable_bool(value: bool) -> PortableValue {
    PortableValue::new(TYPE_BOOL_ID, vec![u8::from(value)])
}

pub(crate) fn zero_for_type(ty: ir::TypeRef) -> Result<PortableValue, CompilerError> {
    match ty {
        TYPE_U64_ID => Ok(portable_u64(0)),
        TYPE_I64_ID => Ok(portable_i64(0)),
        _ => Err(invalid("unsupported unary neg type")),
    }
}

pub(crate) fn single_output(outputs: &[ir::TypeRef]) -> Result<ir::TypeRef, CompilerError> {
    match outputs {
        [ty] => Ok(*ty),
        _ => Err(invalid("value context requires single-output expression")),
    }
}

pub(crate) fn invalid(message: impl Into<String>) -> CompilerError {
    CompilerError::InvalidProgram(anyhow!(message.into()))
}
