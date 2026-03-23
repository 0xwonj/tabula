//! LogUp bus interactions for the ExecutionChip.

use p3_field::PrimeCharacteristicRing;

use tabula_stark::air::builder::InteractionAirBuilder;
use tabula_stark::air::bus::{
    AccessTupleExpr, PropertyReadAirBuilder, ReadAccessAirBuilder, WriteAccessAirBuilder,
};
use tabula_stark::air::interaction::{AirInteraction, core_buses};

use super::columns::ExecutionCols;
use crate::ir_hash::IR_HASH_BUS;

/// C10 ReadAccess bus send: non-empty reads.
///
/// Tuple: `(t, c, key[3], tx_index, val[W], is_null)`.
/// Multiplicity: `is_real * op_read * (1 - is_empty_col)`.
pub(super) fn send_read_access<AB: InteractionAirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
) {
    let mult: AB::Expr =
        local.is_real.into() * local.op_read.into() * (AB::Expr::ONE - local.is_empty_col.into());

    let value = local
        .access_val
        .iter()
        .copied()
        .map(Into::into)
        .collect::<Vec<AB::Expr>>();
    builder.send_read_access(
        AccessTupleExpr {
            table_id: local.access_t.into(),
            col_id: local.access_c.into(),
            key_limb0: local.access_r.limbs.limb0.into(),
            key_limb1: local.access_r.limbs.limb1.into(),
            key_limb2: local.access_r.limbs.limb2.into(),
            tx_index: local.tx_index.into(),
            value,
            is_null: local.access_is_null.into(),
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
    let mult: AB::Expr = local.is_real.into() * local.op_write.into();

    let value = local
        .access_val
        .iter()
        .copied()
        .map(Into::into)
        .collect::<Vec<AB::Expr>>();
    builder.send_write_access(
        AccessTupleExpr {
            table_id: local.access_t.into(),
            col_id: local.access_c.into(),
            key_limb0: local.access_r.limbs.limb0.into(),
            key_limb1: local.access_r.limbs.limb1.into(),
            key_limb2: local.access_r.limbs.limb2.into(),
            tx_index: local.tx_index.into(),
            value,
            is_null: local.access_is_null.into(),
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
    let mult: AB::Expr = local.is_real.into() * local.is_empty_col.into();

    builder.send(AirInteraction {
        values: vec![local.access_t.into(), local.access_c.into()],
        multiplicity: mult,
        bus: core_buses::EMPTY_COL_READ,
    });
}

/// C8 RangeCheck bus sends.
pub(super) fn send_range_checks<AB: InteractionAirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
) {
    let mult: AB::Expr = local.is_real.into() * local.is_access.into();

    let mut send_rc = |val: AB::Expr| {
        builder.send(AirInteraction {
            values: vec![val],
            multiplicity: mult.clone(),
            bus: core_buses::RANGE_CHECK,
        });
    };

    // access_r limbs (limb2 proven by Limb2Bits, no RC send needed)
    send_rc(local.access_r.l0_halves.lo.into());
    send_rc(local.access_r.l0_halves.hi.into());
    send_rc(local.access_r.l1_halves.lo.into());
    send_rc(local.access_r.l1_halves.hi.into());

    // Cmp inequality diff limbs
    let cmp_mult: AB::Expr =
        local.is_real.into() * local.op_cmp.into() * (AB::Expr::ONE - local.cmp.eq_witness.into());
    let mut send_cmp_rc = |val: AB::Expr| {
        builder.send(AirInteraction {
            values: vec![val],
            multiplicity: cmp_mult.clone(),
            bus: core_buses::RANGE_CHECK,
        });
    };
    send_cmp_rc(local.cmp.ineq_diff0_halves.lo.into());
    send_cmp_rc(local.cmp.ineq_diff0_halves.hi.into());
    send_cmp_rc(local.cmp.ineq_diff1_halves.lo.into());
    send_cmp_rc(local.cmp.ineq_diff1_halves.hi.into());
    // diff2 proven by Limb2Bits, no RC send needed

    // Mul carry range checks
    let mul_mult: AB::Expr =
        local.is_real.into() * local.op_arith.into() * local.arith_is_mul.into();
    let mut send_mul_rc = |val: AB::Expr| {
        builder.send(AirInteraction {
            values: vec![val],
            multiplicity: mul_mult.clone(),
            bus: core_buses::RANGE_CHECK,
        });
    };
    send_mul_rc(local.mul.c0_halves.lo.into());
    send_mul_rc(local.mul.c0_halves.hi.into());
    send_mul_rc(local.mul.c1_lo.into());
    send_mul_rc(local.mul.c1_hi.into());

    // DivMod range checks
    let divmod_mult: AB::Expr = local.is_real.into() * local.op_divmod.into();
    let mut send_divmod_rc = |val: AB::Expr| {
        builder.send(AirInteraction {
            values: vec![val],
            multiplicity: divmod_mult.clone(),
            bus: core_buses::RANGE_CHECK,
        });
    };
    send_divmod_rc(local.divmod.c0_halves.lo.into());
    send_divmod_rc(local.divmod.c0_halves.hi.into());
    send_divmod_rc(local.divmod.c1_lo.into());
    send_divmod_rc(local.divmod.c1_hi.into());
    send_divmod_rc(local.divmod.rem_diff0_halves.lo.into());
    send_divmod_rc(local.divmod.rem_diff0_halves.hi.into());
    send_divmod_rc(local.divmod.rem_diff1_halves.lo.into());
    send_divmod_rc(local.divmod.rem_diff1_halves.hi.into());
    // diff2 proven by Limb2Bits, no RC send needed
}

/// Hash bus send: relay the canonical digest to the dedicated IR-hash lane.
pub(super) fn send_hash_relay<AB: InteractionAirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
) {
    let multiplicity: AB::Expr = local.is_real.into() * local.op_hash.into();

    let mut values: Vec<AB::Expr> = Vec::with_capacity(10);
    values.push(local.tx_index.into());
    values.push(local.instruction_index.into());
    for i in 0..8 {
        values.push(local.hash_digest[i].into());
    }

    builder.send(AirInteraction {
        values,
        multiplicity,
        bus: IR_HASH_BUS,
    });
}

/// C9 StaticTableLookup bus send for Lookup opcode.
pub(super) fn send_static_table_lookup<AB: InteractionAirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
) {
    let multiplicity: AB::Expr = local.is_real.into() * local.op_lookup.into();

    let mut values: Vec<AB::Expr> = vec![
        local.access_t.into(),
        local.access_c.into(),
        local.access_r.limbs.limb0.into(),
        local.access_r.limbs.limb1.into(),
        local.access_r.limbs.limb2.into(),
    ];
    for i in 0..W {
        values.push(local.access_val[i].into());
    }

    builder.send(AirInteraction {
        values,
        multiplicity,
        bus: core_buses::STATIC_TABLE_LOOKUP,
    });
}

/// C18 PropertyRead bus send: structural query results.
///
/// Tuple:
/// `(t, c, query_type, query_arg0[W], query_arg1[W], result_val[W], result_key[W], is_null)`.
/// Multiplicity: `is_real * op_property_read`.
pub(super) fn send_property_read<AB: InteractionAirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
) {
    let multiplicity: AB::Expr = local.is_real.into() * local.op_property_read.into();

    builder.send_property_read(
        local.access_t.into(),
        local.access_c.into(),
        local.property_query_type.into(),
        &local.property_query_arg0,
        &local.property_query_arg1,
        &local.property_result_val,
        &local.property_result_key,
        local.property_result_is_null.into(),
        multiplicity,
    );
}

/// C17 Precompile bus send: canonical precompile call header.
///
/// Tuple:
/// `(tx_index, instruction_index, precompile_id, input_count, output_count, event_digest[0..8])`.
/// Multiplicity: `is_real * op_precompile`.
pub(super) fn send_precompile<AB: InteractionAirBuilder, const W: usize>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var, W>,
) {
    let multiplicity: AB::Expr = local.is_real.into() * local.op_precompile.into();

    let mut values: Vec<AB::Expr> = Vec::with_capacity(13);
    values.push(local.tx_index.into());
    values.push(local.instruction_index.into());
    values.push(local.precompile_id.into());
    values.push(local.precompile_input_count.into());
    values.push(local.precompile_output_count.into());
    for i in 0..8 {
        values.push(local.precompile_event_digest[i].into());
    }

    builder.send(AirInteraction {
        values,
        multiplicity,
        bus: core_buses::PRECOMPILE,
    });
}
