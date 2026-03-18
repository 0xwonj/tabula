//! MemoryShardChip — AIR constraints for per-column memory ordering.
//!
//! Per-column version of `InterTxOrderChip`. All rows belong to one `(t, c)`,
//! eliminating segment detection and lex ordering.
//!
//! Constraint groups:
//! 1. Boolean fields
//! 2. `is_real` prefix: monotonic 1→0
//! 3. Constant identity: table_id, col_id same across all real rows
//! 4. Init first: new key → must be init row
//! 5. Init shape: init → no read/write, output=input
//! 6. Access minimum: non-init → has_read OR has_write
//! 7. Read consistency: same_key read → input = prev.output
//! 8. Output derivation: no write → output = input
//! 9. Key ordering: different key → strict inequality
//! 10. Tx ordering: same_key → tx_diff = next.tx_index - tx_index - 1
//! 11. is_last_for_key: ↔ next row has different key
//! 12. has_ever_written: monotone within key; init→0; has_write→1
//! 13. Range checks: key halves, ordering halves, tx_diff
//!
//! LogUp buses:
//! - C10 ReadAccess receive
//! - C11 WriteAccess receive
//! - C13 BaseStateEntry send (init rows)
//! - C14 CoalescedWrite send (last-for-key with write)
//! - C8 RangeCheck sends

use p3_air::{Air, AirBuilder, BaseAir, WindowAccess};
use p3_field::PrimeCharacteristicRing;

use tabula_gadgets::{
    constrain_constant_identity, constrain_is_real_prefix, constrain_is_zero, constrain_key_halves,
    constrain_ordering_halves, constrain_strict_ineq, send_key_range_checks,
    send_ordering_range_checks,
};
use tabula_stark::air::builder::InteractionAirBuilder;
use tabula_stark::air::bus::{
    AccessTupleExpr, BaseStateEntryAirBuilder, CoalescedWriteAirBuilder, ReadAccessAirBuilder,
    WriteAccessAirBuilder,
};
use tabula_stark::air::columns::borrow_cols;
use tabula_stark::chips::ChipId;

use crate::ChipSpec;

use super::columns::{MemoryShardCols, memory_shard_width};

/// Per-column memory shard AIR chip.
///
/// Each instance operates on a single `(table_id, col_id)` pair.
/// The `chip_id` is dynamically allocated at construction time.
#[derive(Debug, Clone)]
pub struct MemoryShardChip<const W: usize> {
    chip_id: ChipId,
    table_id: u32,
    col_id: u16,
}

impl<const W: usize> MemoryShardChip<W> {
    /// Create a new memory shard chip for a specific column.
    pub fn new(chip_id: ChipId, table_id: u32, col_id: u16) -> Self {
        Self {
            chip_id,
            table_id,
            col_id,
        }
    }

    /// Table identifier this shard operates on.
    pub fn table_id(&self) -> u32 {
        self.table_id
    }

    /// Column identifier this shard operates on.
    pub fn col_id(&self) -> u16 {
        self.col_id
    }
}

impl<const W: usize> ChipSpec for MemoryShardChip<W> {
    fn chip_id(&self) -> ChipId {
        self.chip_id
    }

    fn chip_name(&self) -> &'static str {
        "MemoryShard"
    }
}

impl<F, const W: usize> BaseAir<F> for MemoryShardChip<W> {
    fn width(&self) -> usize {
        memory_shard_width::<W>()
    }
}

impl<AB: InteractionAirBuilder, const W: usize> Air<AB> for MemoryShardChip<W> {
    fn eval(&self, builder: &mut AB) {
        let main = builder.main();
        let local_row = main.current_slice();
        let next_row = main.next_slice();
        let local: &MemoryShardCols<AB::Var, W> = borrow_cols(local_row);
        let next: &MemoryShardCols<AB::Var, W> = borrow_cols(next_row);

        let is_real: AB::Expr = local.is_real.into();
        let both_real: AB::Expr = is_real.clone() * next.is_real.into();

        // Derive same_key from key limb IsZero gadgets.
        let limb0_same: AB::Expr = local.r_limb0_iz.is_zero.into();
        let limb1_same: AB::Expr = local.r_limb1_iz.is_zero.into();
        let limb2_same: AB::Expr = local.r_limb2_iz.is_zero.into();
        let same_key: AB::Expr = limb0_same * limb1_same * limb2_same;

        // 1. Boolean constraints
        constrain_booleans(builder, local);

        // 2. is_real prefix
        constrain_is_real_prefix(builder, local.is_real, next.is_real);

        // 3. Constant identity: table_id and col_id must not change between real rows
        constrain_constant_identity(
            builder,
            local.table_id,
            next.table_id,
            local.col_id,
            next.col_id,
            both_real.clone(),
        );

        // 4. Init first
        constrain_init_first(builder, local, next, both_real.clone(), same_key.clone());

        // 5. Init shape
        constrain_init_shape(builder, local, is_real.clone());

        // 6. Access minimum
        constrain_access_minimum(builder, local, is_real.clone());

        // 7. Read consistency
        constrain_read_consistency::<AB, W>(
            builder,
            local,
            next,
            both_real.clone(),
            same_key.clone(),
        );

        // 8. Output derivation
        constrain_output_derivation(builder, local, is_real.clone());

        // 9. Key ordering
        constrain_key_ordering(builder, local, next, both_real.clone(), same_key.clone());

        // 10. Tx ordering
        constrain_tx_ordering(builder, local, next, both_real.clone(), same_key.clone());

        // 11. is_last_for_key
        constrain_is_last_for_key(
            builder,
            local,
            next,
            is_real.clone(),
            both_real.clone(),
            same_key.clone(),
        );

        // 12. has_ever_written
        constrain_has_ever_written(
            builder,
            local,
            next,
            is_real.clone(),
            both_real.clone(),
            same_key.clone(),
        );

        // 13. Decomposition constraints
        constrain_key_halves(builder, &local.key);
        constrain_ordering_halves(builder, &local.key_ordering);

        // Key limb IsZero gadgets (unconditional — must have valid witnesses everywhere)
        {
            let diff0: AB::Expr = next.key.limbs.limb0.into() - local.key.limbs.limb0.into();
            constrain_is_zero(builder, diff0, &local.r_limb0_iz);

            let diff1: AB::Expr = next.key.limbs.limb1.into() - local.key.limbs.limb1.into();
            constrain_is_zero(builder, diff1, &local.r_limb1_iz);

            let diff2: AB::Expr = next.key.limbs.limb2.into() - local.key.limbs.limb2.into();
            constrain_is_zero(builder, diff2, &local.r_limb2_iz);
        }

        // C8 RangeCheck sends
        send_key_range_checks(builder, &local.key, is_real.clone());
        {
            let diff_key: AB::Expr = AB::Expr::ONE - same_key.clone();
            send_ordering_range_checks(builder, &local.key_ordering, both_real.clone() * diff_key);
        }
        // tx_diff range check (u16)
        {
            let not_init_local: AB::Expr = AB::Expr::ONE - local.is_init.into();
            let not_init_next: AB::Expr = AB::Expr::ONE - next.is_init.into();
            builder.send(tabula_stark::air::interaction::AirInteraction {
                values: vec![local.tx_diff.into()],
                multiplicity: both_real.clone() * same_key.clone() * not_init_local * not_init_next,
                bus: tabula_stark::air::interaction::core_buses::RANGE_CHECK,
            });
        }

        // LogUp buses
        send_receive_bus_interactions(builder, local, is_real);
    }
}

// ── Constraint helpers ───────────────────────────────────────────────────────

/// 1. Boolean constraints on all flag columns.
fn constrain_booleans<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &MemoryShardCols<AB::Var, W>,
) {
    builder.assert_bool(local.is_init);
    builder.assert_bool(local.has_read);
    builder.assert_bool(local.has_write);
    builder.assert_bool(local.is_last_for_key);
    builder.assert_bool(local.has_ever_written);
    builder.assert_bool(local.input_is_null);
    builder.assert_bool(local.output_is_null);
}

/// 4. Init first: when key changes, next row must be init. First real row must be init.
fn constrain_init_first<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &MemoryShardCols<AB::Var, W>,
    next: &MemoryShardCols<AB::Var, W>,
    both_real: AB::Expr,
    same_key: AB::Expr,
) {
    let diff_key: AB::Expr = AB::Expr::ONE - same_key;
    builder
        .when_transition()
        .assert_zero(both_real * diff_key * (AB::Expr::ONE - next.is_init.into()));
    builder
        .when_first_row()
        .assert_zero(local.is_real.into() * (AB::Expr::ONE - local.is_init.into()));
}

/// 5. Init shape: init → has_read=0, has_write=0, output=input.
fn constrain_init_shape<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &MemoryShardCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    let gate: AB::Expr = is_real * local.is_init.into();
    builder.assert_zero(gate.clone() * local.has_read.into());
    builder.assert_zero(gate.clone() * local.has_write.into());
    for i in 0..W {
        let diff: AB::Expr = local.output_val[i].into() - local.input_val[i].into();
        builder.assert_zero(gate.clone() * diff);
    }
    let null_diff: AB::Expr = local.output_is_null.into() - local.input_is_null.into();
    builder.assert_zero(gate * null_diff);
}

/// 6. Access minimum: non-init real rows must have at least has_read or has_write.
fn constrain_access_minimum<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &MemoryShardCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    let not_init: AB::Expr = AB::Expr::ONE - local.is_init.into();
    let no_read: AB::Expr = AB::Expr::ONE - local.has_read.into();
    let no_write: AB::Expr = AB::Expr::ONE - local.has_write.into();
    builder.assert_zero(is_real * not_init * no_read * no_write);
}

/// 7. Read consistency: when same_key and next row reads, next.input = local.output.
fn constrain_read_consistency<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &MemoryShardCols<AB::Var, W>,
    next: &MemoryShardCols<AB::Var, W>,
    both_real: AB::Expr,
    same_key: AB::Expr,
) {
    let gate: AB::Expr = both_real * same_key * next.has_read.into();
    for i in 0..W {
        let diff: AB::Expr = next.input_val[i].into() - local.output_val[i].into();
        builder.when_transition().assert_zero(gate.clone() * diff);
    }
    let null_diff: AB::Expr = next.input_is_null.into() - local.output_is_null.into();
    builder.when_transition().assert_zero(gate * null_diff);
}

/// 8. Output derivation: no write → output = input.
fn constrain_output_derivation<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &MemoryShardCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    let not_init: AB::Expr = AB::Expr::ONE - local.is_init.into();
    let no_write: AB::Expr = AB::Expr::ONE - local.has_write.into();
    let gate: AB::Expr = is_real * not_init * no_write;
    for i in 0..W {
        let diff: AB::Expr = local.output_val[i].into() - local.input_val[i].into();
        builder.assert_zero(gate.clone() * diff);
    }
    let null_diff: AB::Expr = local.output_is_null.into() - local.input_is_null.into();
    builder.assert_zero(gate * null_diff);
}

/// 9. Key ordering: when keys differ, strict inequality `local.key < next.key`.
fn constrain_key_ordering<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &MemoryShardCols<AB::Var, W>,
    next: &MemoryShardCols<AB::Var, W>,
    both_real: AB::Expr,
    same_key: AB::Expr,
) {
    let diff_key: AB::Expr = AB::Expr::ONE - same_key;
    let gate: AB::Expr = both_real * diff_key;
    let mut when_transition = builder.when_transition();
    let mut when_gate = when_transition.when(gate);
    constrain_strict_ineq(
        &mut when_gate,
        &local.key.limbs,
        &next.key.limbs,
        &local.key_ordering.ineq,
    );
}

/// 10. Tx ordering: between consecutive access rows for the same key,
///     tx_diff = next.tx_index - local.tx_index - 1.
fn constrain_tx_ordering<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &MemoryShardCols<AB::Var, W>,
    next: &MemoryShardCols<AB::Var, W>,
    both_real: AB::Expr,
    same_key: AB::Expr,
) {
    let not_init_local: AB::Expr = AB::Expr::ONE - local.is_init.into();
    let not_init_next: AB::Expr = AB::Expr::ONE - next.is_init.into();
    let gate: AB::Expr = both_real * same_key * not_init_local * not_init_next;
    let expected: AB::Expr = next.tx_index.into() - local.tx_index.into() - AB::Expr::ONE;
    builder
        .when_transition()
        .assert_zero(gate * (local.tx_diff.into() - expected));
}

/// 11. is_last_for_key: true iff next row has different key (or is padding).
fn constrain_is_last_for_key<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &MemoryShardCols<AB::Var, W>,
    next: &MemoryShardCols<AB::Var, W>,
    is_real: AB::Expr,
    both_real: AB::Expr,
    same_key: AB::Expr,
) {
    let not_same_key: AB::Expr = AB::Expr::ONE - same_key;

    // When both rows real: is_last_for_key = !same_key
    builder
        .when_transition()
        .assert_zero(both_real * (local.is_last_for_key.into() - not_same_key));

    // When real→padding: must be last for key
    let real_to_padding: AB::Expr = is_real * (AB::Expr::ONE - next.is_real.into());
    builder
        .when_transition()
        .assert_zero(real_to_padding * (AB::Expr::ONE - local.is_last_for_key.into()));
}

/// 12. has_ever_written: init→0, same-key transition OR propagation.
fn constrain_has_ever_written<AB: AirBuilder, const W: usize>(
    builder: &mut AB,
    local: &MemoryShardCols<AB::Var, W>,
    next: &MemoryShardCols<AB::Var, W>,
    is_real: AB::Expr,
    both_real: AB::Expr,
    same_key: AB::Expr,
) {
    // Init → has_ever_written = 0
    builder.assert_zero(is_real * local.is_init.into() * local.has_ever_written.into());

    // Same-key transition: next.hew = local.hew OR next.has_write
    let local_hew: AB::Expr = local.has_ever_written.into();
    let next_hw: AB::Expr = next.has_write.into();
    let expected: AB::Expr = local_hew.clone() + next_hw.clone() - local_hew * next_hw;
    builder
        .when_transition()
        .assert_zero(both_real * same_key * (next.has_ever_written.into() - expected));
}

/// LogUp bus interactions (C10, C11, C13, C14).
fn send_receive_bus_interactions<AB: InteractionAirBuilder, const W: usize>(
    builder: &mut AB,
    local: &MemoryShardCols<AB::Var, W>,
    is_real: AB::Expr,
) {
    // C10 ReadAccess receive: non-init rows with has_read
    builder.receive_read_access(
        AccessTupleExpr {
            table_id: local.table_id.into(),
            col_id: local.col_id.into(),
            key_limb0: local.key.limbs.limb0.into(),
            key_limb1: local.key.limbs.limb1.into(),
            key_limb2: local.key.limbs.limb2.into(),
            tx_index: local.tx_index.into(),
            value: local
                .input_val
                .iter()
                .copied()
                .map(Into::into)
                .collect::<Vec<AB::Expr>>(),
            is_null: local.input_is_null.into(),
        },
        is_real.clone() * local.has_read.into() * (AB::Expr::ONE - local.is_init.into()),
    );

    // C11 WriteAccess receive: non-init rows with has_write
    builder.receive_write_access(
        AccessTupleExpr {
            table_id: local.table_id.into(),
            col_id: local.col_id.into(),
            key_limb0: local.key.limbs.limb0.into(),
            key_limb1: local.key.limbs.limb1.into(),
            key_limb2: local.key.limbs.limb2.into(),
            tx_index: local.tx_index.into(),
            value: local
                .output_val
                .iter()
                .copied()
                .map(Into::into)
                .collect::<Vec<AB::Expr>>(),
            is_null: local.output_is_null.into(),
        },
        is_real.clone() * local.has_write.into() * (AB::Expr::ONE - local.is_init.into()),
    );

    // C13 BaseStateEntry send: init rows
    builder.send_base_state_entry(
        local.table_id.into(),
        local.col_id.into(),
        &local.key.limbs,
        &local.input_val,
        local.input_is_null.into(),
        is_real.clone() * local.is_init.into(),
    );

    // C14 CoalescedWrite send: last-for-key rows that had a write
    builder.send_coalesced_write(
        local.table_id.into(),
        local.col_id.into(),
        &local.key.limbs,
        &local.output_val,
        local.output_is_null.into(),
        is_real * local.is_last_for_key.into() * local.has_ever_written.into(),
    );
}
