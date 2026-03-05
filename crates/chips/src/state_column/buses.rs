//! LogUp bus interactions for the StateColumnChip.

use p3_field::PrimeCharacteristicRing;

use tabula_gadgets::{send_key_range_checks, send_lex_range_checks, send_ordering_range_checks};
use tabula_stark::air::builder::InteractionAirBuilder;
use tabula_stark::air::bus::{
    BaseStateEntryAirBuilder, CoalescedWriteAirBuilder, CommitmentAirBuilder, PoseidonAirBuilder,
};

use super::columns::StateColumnCols;
use super::derived::{derive_in_write, derive_is_write_only};

/// Emit all LogUp bus sends/receives for the StateColumn chip.
pub(super) fn send_receive_buses<AB: InteractionAirBuilder, const W: usize>(
    builder: &mut AB,
    local: &StateColumnCols<AB::Var, W>,
    is_real: AB::Expr,
    in_old: AB::Expr,
    in_new: AB::Expr,
) {
    // C13 BaseStateEntry receive: in_old entries + gap rows + write_only (from InterTxOrder)
    {
        let is_write_only = derive_is_write_only::<AB, W>(local);
        // In-old entries (old_only, both, delete) → (t, c, key, old_val, 0)
        builder.receive_base_state_entry(
            local.table_id.clone().into(),
            local.col_id.clone().into(),
            &local.key.limbs,
            &local.old_val,
            AB::Expr::ZERO,
            is_real.clone() * in_old.clone() * local.read_mult_witness.clone().into(),
        );
        // Gap rows → (t, c, key, zeros, 1)
        builder.receive_base_state_entry(
            local.table_id.clone().into(),
            local.col_id.clone().into(),
            &local.key.limbs,
            &local.new_val, // zeros for gap rows (constrained)
            AB::Expr::ONE,
            is_real.clone() * local.is_gap.clone().into() * local.read_mult_witness.clone().into(),
        );
        // Write-only entries → (t, c, key, zeros=old_val, 1)
        builder.receive_base_state_entry(
            local.table_id.clone().into(),
            local.col_id.clone().into(),
            &local.key.limbs,
            &local.old_val, // zeros for write_only (constrained by merge logic)
            AB::Expr::ONE,
            is_real.clone() * is_write_only * local.read_mult_witness.clone().into(),
        );
    }

    // C14 CoalescedWrite receive: write entries (write_only, both, delete) (from InterTxOrder)
    {
        let in_write = derive_in_write::<AB, W>(local);
        let is_delete: AB::Expr = local.s1.clone().into() * local.s0.clone().into();
        builder.receive_coalesced_write(
            local.table_id.clone().into(),
            local.col_id.clone().into(),
            &local.key.limbs,
            &local.new_val,
            is_delete,
            is_real.clone() * in_write * local.write_mult_witness.clone().into(),
        );
    }

    // C5 PoseidonPerm send: old chain
    builder.send_poseidon_perm(
        &local.old_hash_chain.perm_input,
        &local.old_hash_acc,
        is_real.clone() * in_old,
    );

    // C5 PoseidonPerm send: new chain
    builder.send_poseidon_perm(
        &local.new_hash_chain.perm_input,
        &local.new_hash_acc,
        is_real.clone() * in_new,
    );

    // C6 CommitmentVerification send: Com_old at segment end
    builder.send_commitment(
        local.table_id.clone().into(),
        local.col_id.clone().into(),
        AB::Expr::ZERO, // comm_type = 0 (Com_old)
        local.segment_is_touched.clone().into(),
        &local.old_hash_acc,
        is_real.clone() * local.is_last_old_entry.clone().into(),
    );

    // C6 CommitmentVerification send: Com_new at segment end (only if touched)
    builder.send_commitment(
        local.table_id.clone().into(),
        local.col_id.clone().into(),
        AB::Expr::ONE, // comm_type = 1 (Com_new)
        AB::Expr::ONE,
        &local.new_hash_acc,
        is_real.clone()
            * local.is_last_new_entry.clone().into()
            * local.segment_is_touched.clone().into(),
    );

    // C8 RangeCheck sends
    send_key_range_checks(builder, &local.key, is_real.clone());
    {
        let same_segment: AB::Expr = AB::Expr::ONE - local.segment.tc_changed.clone().into();
        send_ordering_range_checks(builder, &local.key_ordering, is_real.clone() * same_segment);
    }
    {
        let tc: AB::Expr = local.segment.tc_changed.clone().into();
        send_lex_range_checks(builder, &local.lex, is_real * tc);
    }
}
