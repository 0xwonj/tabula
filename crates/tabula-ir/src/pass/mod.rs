//! IR processing passes: canonicalize, typecheck, validate.
//!
//! Pipeline order: `canonicalize` → `typecheck` → `validate`.

pub mod canonicalize;
pub mod typecheck;
pub mod validate;

pub use typecheck::BodyTypeInfo;

// ---------------------------------------------------------------------------
// Shared utilities
// ---------------------------------------------------------------------------

use crate::RowExpr;

/// Result of comparing two `RowExpr`s for alias resolution (§2.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RowRelation {
    /// Provably the same row key.
    Equal,
    /// Provably different row keys.
    Distinct,
    /// Cannot determine — program must be rejected.
    Ambiguous,
}

/// Determine the static relationship between two row expressions.
///
/// - `Lit(a) == Lit(a)` → Equal
/// - `Param(p) == Param(p)` → Equal
/// - `Slot(s) == Slot(s)` → Equal
/// - `Lit(a) vs Lit(b)` where `a ≠ b` → Distinct
/// - Everything else → Ambiguous
pub(crate) fn row_relation(a: &RowExpr, b: &RowExpr) -> RowRelation {
    match (a, b) {
        (RowExpr::Literal(x), RowExpr::Literal(y)) => {
            if x == y {
                RowRelation::Equal
            } else {
                RowRelation::Distinct
            }
        }
        (RowExpr::Param(x), RowExpr::Param(y)) => {
            if x == y {
                RowRelation::Equal
            } else {
                RowRelation::Ambiguous
            }
        }
        (RowExpr::Slot(x), RowExpr::Slot(y)) => {
            if x == y {
                RowRelation::Equal
            } else {
                RowRelation::Ambiguous
            }
        }
        _ => RowRelation::Ambiguous,
    }
}
