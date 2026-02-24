//! Derived flag expressions for the StateColumn chip.

use p3_air::AirBuilder;
use p3_field::PrimeCharacteristicRing;

use super::columns::StateColumnCols;

/// `in_old = !is_gap * (1 - (1-s1)*s0)` — old_only, both, delete.
pub(super) fn derive_in_old<AB: AirBuilder, const W: usize>(
    local: &StateColumnCols<AB::Var, W>,
) -> AB::Expr {
    let not_gap: AB::Expr = AB::Expr::ONE - local.is_gap.clone().into();
    // in_old = !is_gap * (1 - s0 + s1*s0)
    let s0: AB::Expr = local.s0.clone().into();
    let s1: AB::Expr = local.s1.clone().into();
    not_gap * (AB::Expr::ONE - s0.clone() + s1 * s0)
}

/// `in_new = !is_gap * (1 - s1*s0)` — old_only, write_only, both.
pub(super) fn derive_in_new<AB: AirBuilder, const W: usize>(
    local: &StateColumnCols<AB::Var, W>,
) -> AB::Expr {
    let not_gap: AB::Expr = AB::Expr::ONE - local.is_gap.clone().into();
    let s1_s0: AB::Expr = local.s1.clone().into() * local.s0.clone().into();
    not_gap * (AB::Expr::ONE - s1_s0)
}

/// `is_write_only = !is_gap * !s1 * s0` — write_only only.
pub(super) fn derive_is_write_only<AB: AirBuilder, const W: usize>(
    local: &StateColumnCols<AB::Var, W>,
) -> AB::Expr {
    let not_gap: AB::Expr = AB::Expr::ONE - local.is_gap.clone().into();
    let not_s1: AB::Expr = AB::Expr::ONE - local.s1.clone().into();
    not_gap * not_s1 * local.s0.clone().into()
}

/// `in_write = !is_gap * (s0 + s1 - s0*s1)` — write_only, both, delete.
pub(super) fn derive_in_write<AB: AirBuilder, const W: usize>(
    local: &StateColumnCols<AB::Var, W>,
) -> AB::Expr {
    let not_gap: AB::Expr = AB::Expr::ONE - local.is_gap.clone().into();
    let s0: AB::Expr = local.s0.clone().into();
    let s1: AB::Expr = local.s1.clone().into();
    not_gap * (s0.clone() + s1.clone() - s0 * s1)
}
