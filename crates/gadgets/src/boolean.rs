//! Boolean and `is_real` prefix constraint gadgets.
//!
//! Pure functions generic over `AB: AirBuilder`. Each gadget encodes
//! a small, well-defined constraint pattern referenced by proof-spec.

use p3_air::AirBuilder;
use p3_field::PrimeCharacteristicRing;

/// `is_real` prefix constraint (proof-spec §4.2.G):
/// `is_real` must transition `1 → 0` at most once, then stay `0`.
///
/// Encoded as: `is_real_next * (1 - is_real) = 0` on transition rows.
/// When `is_real = 0`, `is_real_next` must also be `0`.
pub fn constrain_is_real_prefix<AB: AirBuilder>(
    builder: &mut AB,
    is_real: AB::Var,
    next_is_real: AB::Var,
) {
    // is_real must be boolean.
    builder.assert_bool(is_real.clone());
    // Prefix: if is_real=0 then next_is_real=0.
    // Equivalently: next_is_real * (1 - is_real) = 0.
    builder
        .when_transition()
        .assert_zero(next_is_real.into() * (AB::Expr::ONE - is_real.into()));
}

/// Constant identity constraint: `table_id` and `col_id` must remain unchanged
/// across consecutive real rows.
///
/// Gated by `both_real` (typically `local.is_real * next.is_real`) so that
/// transitions into or out of padding rows are unconstrained.
pub fn constrain_constant_identity<AB: AirBuilder>(
    builder: &mut AB,
    local_table_id: AB::Var,
    next_table_id: AB::Var,
    local_col_id: AB::Var,
    next_col_id: AB::Var,
    both_real: AB::Expr,
) {
    let table_diff: AB::Expr = next_table_id.into() - local_table_id.into();
    let col_diff: AB::Expr = next_col_id.into() - local_col_id.into();
    builder
        .when_transition()
        .assert_zero(both_real.clone() * table_diff);
    builder.when_transition().assert_zero(both_real * col_diff);
}
