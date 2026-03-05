//! Memory constraint helpers: null canonicality and read/write transitions.
//!
//! Pure constraint functions generic over `AB: AirBuilder`. These encode the
//! memory semantics from proof-spec sections on GlobalSortedMem.

use p3_air::AirBuilder;

/// Null canonicality constraint: when `val_is_null = 1`, all value limbs must be zero.
///
/// Emits W constraints: `val_is_null * val[i] = 0` for each i in 0..W.
///
/// Callers must separately assert `val_is_null ∈ {0, 1}`.
pub fn constrain_null_canon<AB: AirBuilder>(
    builder: &mut AB,
    val_is_null: AB::Expr,
    val: &[AB::Var],
) {
    for v in val {
        builder.assert_zero(val_is_null.clone() * v.clone().into());
    }
}

/// Memory read constraint: the value read must equal the running memory.
///
/// Emits W + 1 constraints:
/// - `val[i] = mem[i]` for each i
/// - `val_is_null = mem_is_null`
///
/// The caller gates this on `is_read` (i.e., `is_write = 0`).
pub fn constrain_mem_read<AB: AirBuilder>(
    builder: &mut AB,
    val: &[AB::Var],
    val_is_null: AB::Var,
    mem: &[AB::Var],
    mem_is_null: AB::Var,
) {
    assert_eq!(
        val.len(),
        mem.len(),
        "constrain_mem_read: val/mem width mismatch"
    );
    for (v, m) in val.iter().zip(mem.iter()) {
        builder.assert_eq(v.clone(), m.clone());
    }
    builder.assert_eq(val_is_null, mem_is_null);
}

/// Memory write constraint: the running memory is updated to the written value.
///
/// Emits W + 1 constraints on the **next** row's memory:
/// - `next_mem[i] = val[i]` for each i
/// - `next_mem_is_null = val_is_null`
///
/// The caller gates this on `is_write = 1` and same-key continuation.
pub fn constrain_mem_write<AB: AirBuilder>(
    builder: &mut AB,
    val: &[AB::Var],
    val_is_null: AB::Var,
    next_mem: &[AB::Var],
    next_mem_is_null: AB::Var,
) {
    assert_eq!(
        val.len(),
        next_mem.len(),
        "constrain_mem_write: val/next_mem width mismatch"
    );
    for (v, nm) in val.iter().zip(next_mem.iter()) {
        builder.assert_eq(v.clone(), nm.clone());
    }
    builder.assert_eq(val_is_null, next_mem_is_null);
}
