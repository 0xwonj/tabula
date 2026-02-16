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
