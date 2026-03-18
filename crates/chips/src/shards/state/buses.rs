//! LogUp bus interactions for the StateShard chip.

use p3_field::PrimeCharacteristicRing;

use tabula_gadgets::{send_key_range_checks, send_ordering_range_checks};
use tabula_stark::air::builder::InteractionAirBuilder;
use tabula_stark::air::bus::{
    BaseStateEntryAirBuilder, CoalescedWriteAirBuilder, CommitmentAirBuilder, PoseidonAirBuilder,
    SsmcOldEntryAirBuilder,
};

use super::columns::StateShardCols;
use super::derived::{derive_in_write, derive_is_write_only};

/// Emit all LogUp bus sends/receives for the StateShard chip.
pub(super) fn send_receive_buses<AB: InteractionAirBuilder, const W: usize>(
    builder: &mut AB,
    local: &StateShardCols<AB::Var, W>,
    is_real: AB::Expr,
    in_old: &AB::Expr,
    in_new: AB::Expr,
) {
    // C13 BaseStateEntry receive: in_old entries + gap rows + write_only
    {
        let is_write_only = derive_is_write_only::<AB, W>(local);
        // In-old entries (old_only, both, delete)
        builder.receive_base_state_entry(
            local.table_id.into(),
            local.col_id.into(),
            &local.key.limbs,
            &local.old_val,
            AB::Expr::ZERO,
            is_real.clone() * (*in_old).clone() * local.read_mult_witness.into(),
        );
        // Gap rows
        builder.receive_base_state_entry(
            local.table_id.into(),
            local.col_id.into(),
            &local.key.limbs,
            &local.new_val, // zeros for gap rows (constrained)
            AB::Expr::ONE,
            is_real.clone() * local.is_gap.into() * local.read_mult_witness.into(),
        );
        // Write-only entries
        builder.receive_base_state_entry(
            local.table_id.into(),
            local.col_id.into(),
            &local.key.limbs,
            &local.old_val, // zeros for write_only (constrained by merge logic)
            AB::Expr::ONE,
            is_real.clone() * is_write_only * local.read_mult_witness.into(),
        );
    }

    // C14 CoalescedWrite receive: write entries (write_only, both, delete)
    {
        let in_write = derive_in_write::<AB, W>(local);
        let is_delete: AB::Expr = local.s1.into() * local.s0.into();
        builder.receive_coalesced_write(
            local.table_id.into(),
            local.col_id.into(),
            &local.key.limbs,
            &local.new_val,
            is_delete,
            is_real.clone() * in_write * local.write_mult_witness.into(),
        );
    }

    // C5 PoseidonPerm send: old chain
    builder.send_poseidon_perm(
        &local.old_hash_chain.perm_input,
        &local.old_hash_acc,
        is_real.clone() * (*in_old).clone(),
    );

    // C5 PoseidonPerm send: new chain
    builder.send_poseidon_perm(
        &local.new_hash_chain.perm_input,
        &local.new_hash_acc,
        is_real.clone() * in_new,
    );

    // C6 CommitmentVerification send: Com_old at end
    builder.send_commitment(
        local.table_id.into(),
        local.col_id.into(),
        AB::Expr::ZERO, // comm_type = 0 (Com_old)
        local.segment_is_touched.into(),
        &local.old_hash_acc,
        is_real.clone() * local.is_last_old_entry.into(),
    );

    // C6 CommitmentVerification send: Com_new (only if touched)
    builder.send_commitment(
        local.table_id.into(),
        local.col_id.into(),
        AB::Expr::ONE, // comm_type = 1 (Com_new)
        AB::Expr::ONE,
        &local.new_hash_acc,
        is_real.clone() * local.is_last_new_entry.into() * local.segment_is_touched.into(),
    );

    // C8 RangeCheck sends
    send_key_range_checks(builder, &local.key, is_real.clone());
    // Key ordering range checks (always active — no segment boundaries)
    send_ordering_range_checks(builder, &local.key_ordering, is_real.clone());

    // C20 SsmcOldEntry send: old-entry anchor + adjacency metadata.
    builder.send_ssmc_old_entry(
        local.table_id.into(),
        local.col_id.into(),
        &local.key.limbs,
        &local.old_val,
        local.has_prev_old_entry.into(),
        &local.prev_old_key.limbs,
        local.is_last_old_entry.into(),
        &local.next_old_key.limbs,
        is_real * local.property_anchor_mult.into(),
    );
}
