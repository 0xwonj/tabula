use crate::error::{FrontendError, FrontendErrorKind};
use crate::span::Span;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DeferredSyntaxFeature {
    Requires,
    Ensures,
    ForLoop,
    Predicate,
    Invariant,
    ElseIf,
    MatchGuard,
    ExpressionMatchArm,
    PathMatchPattern,
    TupleMatchPattern,
}

impl DeferredSyntaxFeature {
    fn label(self) -> &'static str {
        match self {
            Self::Requires => "requires",
            Self::Ensures => "ensures",
            Self::ForLoop => "for",
            Self::Predicate => "predicate",
            Self::Invariant => "invariant",
            Self::ElseIf => "else-if",
            Self::MatchGuard => "match guards",
            Self::ExpressionMatchArm => "expression match arms",
            Self::PathMatchPattern => "path/variant match patterns",
            Self::TupleMatchPattern => "tuple match patterns",
        }
    }
}

pub(super) fn deferred_feature_error(span: Span, feature: DeferredSyntaxFeature) -> FrontendError {
    FrontendError::new(
        FrontendErrorKind::UnsupportedFeature,
        span,
        format!(
            "{} is intentionally deferred to a later phase",
            feature.label()
        ),
    )
}
