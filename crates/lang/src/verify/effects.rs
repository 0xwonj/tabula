#![allow(missing_docs)]

#[allow(clippy::wildcard_imports)]
use super::*;
pub(super) fn single_output(outputs: &[TypeRef]) -> Option<TypeRef> {
    match outputs {
        [ty] => Some(*ty),
        _ => None,
    }
}

pub(super) fn value_to_fingerprint(expr: &ConstExpr) -> Result<String, FrontendError> {
    Ok(match expr {
        ConstExpr::Literal(value) => format!("lit:{}:{:?}", value.type_id().0, value.payload()),
        ConstExpr::Unary { op, expr } => {
            format!("unary:{op:?}:{}", value_to_fingerprint(expr)?)
        }
        ConstExpr::Binary { op, lhs, rhs } => format!(
            "binary:{op:?}:{}:{}",
            value_to_fingerprint(lhs)?,
            value_to_fingerprint(rhs)?
        ),
    })
}

pub(super) fn ensure_type(
    actual: TypeRef,
    expected: TypeRef,
    span: crate::span::Span,
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
