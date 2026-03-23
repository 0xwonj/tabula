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

use tabula_core::PortableValue;
use tabula_profile::TYPE_U64_ID;

use crate::{RowExpr, ValueExpr};

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

/// Convert a `RowExpr` to the equivalent `ValueExpr` for use in Cmp instructions.
pub(crate) fn row_to_value_expr(row: &RowExpr) -> ValueExpr {
    match row {
        RowExpr::Literal(rk) => {
            ValueExpr::Literal(PortableValue::new(TYPE_U64_ID, rk.0.to_le_bytes().to_vec()))
        }
        RowExpr::Param(p) => ValueExpr::Param(*p),
        RowExpr::Slot(s) => ValueExpr::Slot(*s),
    }
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
