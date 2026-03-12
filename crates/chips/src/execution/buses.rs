//! LogUp bus interactions for the ExecutionChip.

use p3_field::PrimeCharacteristicRing;

use tabula_stark::air::builder::InteractionAirBuilder;
use tabula_stark::air::bus::{
    AccessTupleExpr, PropertyReadAirBuilder, ReadAccessAirBuilder, WriteAccessAirBuilder,
};
use tabula_stark::air::interaction::{AirInteraction, core_buses};

use super::air::{HASH_INSTRUCTION_DOMAIN_TAG, HASH_INSTRUCTION_INPUT_COUNT};
use super::columns::ExecutionCols;

/// C10 ReadAccess bus send: non-empty reads.
///
/// Tuple: `(t, c, key[3], tx_index, val[W], is_null)`.
/// Multiplicity: `is_real * op_read * (1 - is_empty_col)`.
pub(super) fn send_read_access<AB: InteractionAirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
) {
    let mult: AB::Expr = local.is_real.clone().into()
        * local.op_read.clone().into()
        * (AB::Expr::ONE - local.is_empty_col.clone().into());

    let value = local
        .access_val
        .iter()
        .cloned()
        .map(Into::into)
        .collect::<Vec<AB::Expr>>();
    builder.send_read_access(
        AccessTupleExpr {
            table_id: local.access_t.clone().into(),
            col_id: local.access_c.clone().into(),
            key_limb0: local.access_r.limbs.limb0.clone().into(),
            key_limb1: local.access_r.limbs.limb1.clone().into(),
            key_limb2: local.access_r.limbs.limb2.clone().into(),
            tx_index: local.tx_index.clone().into(),
            value,
            is_null: local.access_is_null.clone().into(),
        },
        mult,
    );
}

/// C11 WriteAccess bus send: writes.
///
/// Tuple: `(t, c, key[3], tx_index, val[W], is_null)`.
/// Multiplicity: `is_real * op_write`.
pub(super) fn send_write_access<AB: InteractionAirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
) {
    let mult: AB::Expr = local.is_real.clone().into() * local.op_write.clone().into();

    let value = local
        .access_val
        .iter()
        .cloned()
        .map(Into::into)
        .collect::<Vec<AB::Expr>>();
    builder.send_write_access(
        AccessTupleExpr {
            table_id: local.access_t.clone().into(),
            col_id: local.access_c.clone().into(),
            key_limb0: local.access_r.limbs.limb0.clone().into(),
            key_limb1: local.access_r.limbs.limb1.clone().into(),
            key_limb2: local.access_r.limbs.limb2.clone().into(),
            tx_index: local.tx_index.clone().into(),
            value,
            is_null: local.access_is_null.clone().into(),
        },
        mult,
    );
}

/// C12 EmptyColRead bus send: reads from empty columns.
///
/// Tuple: `(t, c)`.
/// Multiplicity: `is_real * is_empty_col`.
pub(super) fn send_empty_col_read<AB: InteractionAirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
) {
    let mult: AB::Expr = local.is_real.clone().into() * local.is_empty_col.clone().into();

    builder.send(AirInteraction {
        values: vec![local.access_t.clone().into(), local.access_c.clone().into()],
        multiplicity: mult,
        bus: core_buses::EMPTY_COL_READ,
    });
}

/// C8 RangeCheck bus sends.
pub(super) fn send_range_checks<AB: InteractionAirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
) {
    let mult: AB::Expr = local.is_real.clone().into() * local.is_access.clone().into();

    let mut send_rc = |val: AB::Expr| {
        builder.send(AirInteraction {
            values: vec![val],
            multiplicity: mult.clone(),
            bus: core_buses::RANGE_CHECK,
        });
    };

    // access_r limbs (limb2 proven by Limb2Bits, no RC send needed)
    send_rc(local.access_r.l0_halves.lo.clone().into());
    send_rc(local.access_r.l0_halves.hi.clone().into());
    send_rc(local.access_r.l1_halves.lo.clone().into());
    send_rc(local.access_r.l1_halves.hi.clone().into());

    // Cmp inequality diff limbs
    let cmp_mult: AB::Expr = local.is_real.clone().into()
        * local.op_cmp.clone().into()
        * (AB::Expr::ONE - local.cmp.eq_witness.clone().into());
    let mut send_cmp_rc = |val: AB::Expr| {
        builder.send(AirInteraction {
            values: vec![val],
            multiplicity: cmp_mult.clone(),
            bus: core_buses::RANGE_CHECK,
        });
    };
    send_cmp_rc(local.cmp.ineq_diff0_halves.lo.clone().into());
    send_cmp_rc(local.cmp.ineq_diff0_halves.hi.clone().into());
    send_cmp_rc(local.cmp.ineq_diff1_halves.lo.clone().into());
    send_cmp_rc(local.cmp.ineq_diff1_halves.hi.clone().into());
    // diff2 proven by Limb2Bits, no RC send needed

    // Mul carry range checks
    let mul_mult: AB::Expr = local.is_real.clone().into()
        * local.op_arith.clone().into()
        * local.arith_is_mul.clone().into();
    let mut send_mul_rc = |val: AB::Expr| {
        builder.send(AirInteraction {
            values: vec![val],
            multiplicity: mul_mult.clone(),
            bus: core_buses::RANGE_CHECK,
        });
    };
    send_mul_rc(local.mul.c0_halves.lo.clone().into());
    send_mul_rc(local.mul.c0_halves.hi.clone().into());
    send_mul_rc(local.mul.c1_lo.clone().into());
    send_mul_rc(local.mul.c1_hi.clone().into());

    // DivMod range checks
    let divmod_mult: AB::Expr = local.is_real.clone().into() * local.op_divmod.clone().into();
    let mut send_divmod_rc = |val: AB::Expr| {
        builder.send(AirInteraction {
            values: vec![val],
            multiplicity: divmod_mult.clone(),
            bus: core_buses::RANGE_CHECK,
        });
    };
    send_divmod_rc(local.divmod.c0_halves.lo.clone().into());
    send_divmod_rc(local.divmod.c0_halves.hi.clone().into());
    send_divmod_rc(local.divmod.c1_lo.clone().into());
    send_divmod_rc(local.divmod.c1_hi.clone().into());
    send_divmod_rc(local.divmod.rem_diff0_halves.lo.clone().into());
    send_divmod_rc(local.divmod.rem_diff0_halves.hi.clone().into());
    send_divmod_rc(local.divmod.rem_diff1_halves.lo.clone().into());
    send_divmod_rc(local.divmod.rem_diff1_halves.hi.clone().into());
    // diff2 proven by Limb2Bits, no RC send needed
}

/// C5 PoseidonPermutation bus send for Hash and Precompile opcodes.
///
/// Both opcodes use the Poseidon permutation columns, so both contribute
/// to the POSEIDON_PERM bus.
pub(super) fn send_hash_permutation<AB: InteractionAirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
) {
    // Suppress unused-constant warnings: these constants are used by the
    // Hash opcode's trace builder to construct perm_input. The AIR sends
    // whatever perm_input/output the trace provides.
    let _ = HASH_INSTRUCTION_DOMAIN_TAG;
    let _ = HASH_INSTRUCTION_INPUT_COUNT;

    let multiplicity: AB::Expr = local.is_real.clone().into()
        * (local.op_hash.clone().into() + local.op_precompile.clone().into());

    let mut values: Vec<AB::Expr> = Vec::with_capacity(24);
    for i in 0..16 {
        values.push(local.hash_perm_input[i].clone().into());
    }
    for i in 0..8 {
        values.push(local.hash_perm_output[i].clone().into());
    }

    builder.send(AirInteraction {
        values,
        multiplicity,
        bus: core_buses::POSEIDON_PERM,
    });
}

/// C9 StaticTableLookup bus send for Lookup opcode.
pub(super) fn send_static_table_lookup<AB: InteractionAirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
) {
    let multiplicity: AB::Expr = local.is_real.clone().into() * local.op_lookup.clone().into();

    let mut values: Vec<AB::Expr> = vec![
        local.access_t.clone().into(),
        local.access_c.clone().into(),
        local.access_r.limbs.limb0.clone().into(),
        local.access_r.limbs.limb1.clone().into(),
        local.access_r.limbs.limb2.clone().into(),
    ];
    for i in 0..W {
        values.push(local.access_val[i].clone().into());
    }

    builder.send(AirInteraction {
        values,
        multiplicity,
        bus: core_buses::STATIC_TABLE_LOOKUP,
    });
}

/// C18 PropertyRead bus send: structural query results.
///
/// Tuple: `(t, c, query_type, result_val[W], result_key[W], is_null)`.
/// Multiplicity: `is_real * op_property_read`.
pub(super) fn send_property_read<AB: InteractionAirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
) {
    let multiplicity: AB::Expr =
        local.is_real.clone().into() * local.op_property_read.clone().into();

    builder.send_property_read(
        local.access_t.clone().into(),
        local.access_c.clone().into(),
        local.property_query_type.clone().into(),
        &local.property_result_val,
        &local.property_result_key,
        local.property_result_is_null.clone().into(),
        multiplicity,
    );
}

/// C17 Precompile bus send: I/O commitment for precompile calls.
///
/// Tuple: `(precompile_id, hash_perm_output[0..8])`.
/// Multiplicity: `is_real * op_precompile`.
pub(super) fn send_precompile<AB: InteractionAirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
) {
    let multiplicity: AB::Expr = local.is_real.clone().into() * local.op_precompile.clone().into();

    // 9 FE: (precompile_id, hash_perm_output[0..8])
    let mut values: Vec<AB::Expr> = Vec::with_capacity(9);
    values.push(local.precompile_id.clone().into());
    for i in 0..8 {
        values.push(local.hash_perm_output[i].clone().into());
    }

    builder.send(AirInteraction {
        values,
        multiplicity,
        bus: core_buses::PRECOMPILE,
    });
}
