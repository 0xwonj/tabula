use std::collections::BTreeSet;

use tabula_core::PortableValue;
use tabula_profile::{TYPE_BOOL_ID, TYPE_BYTES32_ID, TYPE_I64_ID, TYPE_U64_ID};

use crate::ast;
use crate::error::{FrontendError, FrontendErrorKind};
use crate::hir;
use crate::span::Span;

pub(super) fn single_segment(path: &ast::IdentPath, span: Span) -> Result<&str, FrontendError> {
    if path.segments.len() == 1 {
        Ok(path.segments[0].as_str())
    } else {
        Err(FrontendError::new(
            FrontendErrorKind::InvalidProgram,
            span,
            "qualified paths are only allowed in use declarations and type names in V2",
        ))
    }
}

pub(super) fn body_span(body: &ast::RelationBody) -> Span {
    match body {
        ast::RelationBody::Enum { span, .. }
        | ast::RelationBody::Range { span, .. }
        | ast::RelationBody::Map { span, .. }
        | ast::RelationBody::Set { span, .. }
        | ast::RelationBody::Extern { span } => *span,
    }
}

pub(super) fn insert_top_name(
    names: &mut BTreeSet<String>,
    symbol: &str,
    span: Span,
) -> Result<(), FrontendError> {
    if !names.insert(symbol.to_string()) {
        return Err(FrontendError::new(
            FrontendErrorKind::DuplicateSymbol,
            span,
            format!("duplicate top-level symbol {symbol}"),
        ));
    }
    Ok(())
}

pub(super) fn build_literal_value(
    kind: &ast::LiteralKind,
    expected_ty: Option<hir::TypeRef>,
    span: Span,
) -> Result<PortableValue, FrontendError> {
    match kind {
        ast::LiteralKind::Integer(value) => {
            let ty = expected_ty.unwrap_or(TYPE_U64_ID);
            match ty {
                TYPE_U64_ID => Ok(PortableValue::new(ty, value.to_le_bytes().to_vec())),
                TYPE_I64_ID => Ok(PortableValue::new(
                    ty,
                    (*value as i64).to_le_bytes().to_vec(),
                )),
                _ => Err(FrontendError::new(
                    FrontendErrorKind::TypeMismatch,
                    span,
                    "integer literal type mismatch",
                )),
            }
        }
        ast::LiteralKind::Bool(value) => {
            let ty = expected_ty.unwrap_or(TYPE_BOOL_ID);
            ensure_type(ty, TYPE_BOOL_ID, span, "bool literal type mismatch")?;
            Ok(PortableValue::new(TYPE_BOOL_ID, vec![u8::from(*value)]))
        }
        ast::LiteralKind::Bytes32(value) => {
            let ty = expected_ty.unwrap_or(TYPE_BYTES32_ID);
            ensure_type(ty, TYPE_BYTES32_ID, span, "bytes32 literal type mismatch")?;
            Ok(PortableValue::new(TYPE_BYTES32_ID, value.to_vec()))
        }
    }
}

pub(super) fn literal_type(kind: &ast::LiteralKind) -> hir::TypeRef {
    match kind {
        ast::LiteralKind::Integer(_) => TYPE_U64_ID,
        ast::LiteralKind::Bool(_) => TYPE_BOOL_ID,
        ast::LiteralKind::Bytes32(_) => TYPE_BYTES32_ID,
    }
}

pub(super) fn convert_unary_op(op: ast::UnaryOp) -> hir::UnaryOp {
    match op {
        ast::UnaryOp::Not => hir::UnaryOp::Not,
        ast::UnaryOp::Neg => hir::UnaryOp::Neg,
    }
}

pub(super) fn convert_binary_op(op: ast::BinaryOp) -> hir::BinaryOp {
    match op {
        ast::BinaryOp::Add => hir::BinaryOp::Add,
        ast::BinaryOp::Sub => hir::BinaryOp::Sub,
        ast::BinaryOp::Mul => hir::BinaryOp::Mul,
        ast::BinaryOp::Div => hir::BinaryOp::Div,
        ast::BinaryOp::Mod => hir::BinaryOp::Mod,
        ast::BinaryOp::Eq => hir::BinaryOp::Eq,
        ast::BinaryOp::Ne => hir::BinaryOp::Ne,
        ast::BinaryOp::Lt => hir::BinaryOp::Lt,
        ast::BinaryOp::Le => hir::BinaryOp::Le,
        ast::BinaryOp::Gt => hir::BinaryOp::Gt,
        ast::BinaryOp::Ge => hir::BinaryOp::Ge,
        ast::BinaryOp::And => hir::BinaryOp::And,
        ast::BinaryOp::Or => hir::BinaryOp::Or,
    }
}

pub(super) fn ensure_type(
    actual: hir::TypeRef,
    expected: hir::TypeRef,
    span: Span,
    message: &'static str,
) -> Result<(), FrontendError> {
    if actual == expected {
        Ok(())
    } else {
        Err(FrontendError::new(
            FrontendErrorKind::TypeMismatch,
            span,
            format!("{message}: expected {}, got {}", expected.0, actual.0),
        ))
    }
}

pub(super) fn ensure_capability(
    ok: bool,
    span: Span,
    message: &'static str,
) -> Result<(), FrontendError> {
    if ok {
        Ok(())
    } else {
        Err(FrontendError::new(
            FrontendErrorKind::TypeMismatch,
            span,
            message,
        ))
    }
}

pub(super) fn single_output_ty(
    outputs: &[hir::TypeRef],
    span: Span,
) -> Result<Option<hir::TypeRef>, FrontendError> {
    match outputs {
        [] => Ok(None),
        [ty] => Ok(Some(*ty)),
        _ => Err(FrontendError::new(
            FrontendErrorKind::UnsupportedFeature,
            span,
            "multi-result expressions are intentionally deferred to a later phase",
        )),
    }
}
