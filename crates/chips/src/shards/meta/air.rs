//! MetaShardChip — AIR constraints for per-column commitment metadata.
//!
//! Per-column version of `ColumnMetaChip`. Each instance handles a single
//! `(table_id, col_id)` pair, eliminating ordering constraints.
//!
//! The commitment scheme is parameterized via constructor fields:
//! - `scheme_tag`: domain-separation value for leaf digest hashing
//! - `receives_commitment`: whether to receive on the C6 CommitmentVerif bus
//!
//! This design allows unlimited commitment schemes without modifying AIR code.
//!
//! Constraint groups:
//! 1. Boolean fields
//! 2. `is_real` prefix: monotonic 1→0
//! 3. Constant identity: table_id, col_id same across all real rows
//! 4. Untouched binding: `is_touched=0 ⟹ com_new = com_old`
//! 5. Touched consistency: `is_touched=0 ⟹ is_empty_new = is_empty_old`
//! 6. Empty→non-empty: `is_empty_old ∧ is_touched ⟹ ¬is_empty_new`
//! 7. Com_empty verification (Poseidon hash)
//! 8. Leaf digest composition
//!
//! LogUp buses:
//! - C12 EmptyColRead receive
//! - C6 CommitmentVerification receive (conditional on `receives_commitment`)
//! - C5 PoseidonPerm send (Com_empty + leaf_old + leaf_new)
//! - C15 SmtLeafDigest send

use p3_air::{Air, AirBuilder, BaseAir, WindowAccess};
use p3_field::PrimeCharacteristicRing;

use tabula_gadgets::{constrain_constant_identity, constrain_is_real_prefix};
use tabula_stark::air::builder::InteractionAirBuilder;
use tabula_stark::air::bus::{
    CommitmentAirBuilder, EmptyColReadAirBuilder, EmptyOldColumnAirBuilder, PoseidonAirBuilder,
    SmtLeafDigestAirBuilder,
};
use tabula_stark::air::columns::borrow_cols;
use tabula_stark::chips::ChipId;

use crate::ChipSpec;

use super::columns::{DIGEST_WIDTH, META_SHARD_WIDTH, MetaShardCols};

/// Per-column meta shard AIR chip.
///
/// Each instance operates on a single `(table_id, col_id)` pair.
/// Tracks commitment transitions (Com_old → Com_new) for that column.
///
/// The `scheme_tag` is used for leaf digest domain separation (e.g., 0=SSMC, 1=SMT).
/// The `receives_commitment` flag controls whether this chip receives on the
/// C6 CommitmentVerification bus (true for SSMC, false for SMT).
#[derive(Debug, Clone)]
pub struct MetaShardChip {
    chip_id: ChipId,
    table_id: u32,
    col_id: u16,
    /// Scheme tag for leaf digest domain separation.
    scheme_tag: u16,
    /// Whether to receive on C6 CommitmentVerification bus.
    receives_commitment: bool,
}

impl MetaShardChip {
    /// Create a new meta shard chip for a specific column.
    ///
    /// - `scheme_tag`: domain-separation value (e.g., `scheme_tags::SSMC` = 0)
    /// - `receives_commitment`: true if this scheme sends commitments via C6
    ///   (SSMC does; SMT does not — it uses C15 SmtLeafDigest instead)
    pub fn new(
        chip_id: ChipId,
        table_id: u32,
        col_id: u16,
        scheme_tag: u16,
        receives_commitment: bool,
    ) -> Self {
        Self {
            chip_id,
            table_id,
            col_id,
            scheme_tag,
            receives_commitment,
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

    /// Scheme tag value as u16.
    pub fn scheme_tag(&self) -> u16 {
        self.scheme_tag
    }
}

impl ChipSpec for MetaShardChip {
    fn chip_id(&self) -> ChipId {
        self.chip_id
    }

    fn chip_name(&self) -> &'static str {
        "MetaShard"
    }

    fn num_public_values(&self) -> usize {
        0
    }

    fn preprocessed_width(&self) -> usize {
        0
    }

    fn has_interactions(&self) -> bool {
        true
    }
}

impl<F> BaseAir<F> for MetaShardChip {
    fn width(&self) -> usize {
        META_SHARD_WIDTH
    }
}

impl<AB: InteractionAirBuilder> Air<AB> for MetaShardChip {
    fn eval(&self, builder: &mut AB) {
        let main = builder.main();
        let local_row = main.current_slice();
        let next_row = main.next_slice();
        let local: &MetaShardCols<AB::Var> = borrow_cols(local_row);
        let next: &MetaShardCols<AB::Var> = borrow_cols(next_row);

        // ── 1. Boolean constraints ──
        constrain_booleans(builder, local);

        // ── 2. is_real prefix ──
        constrain_is_real_prefix(builder, local.is_real, next.is_real);

        // ── 3. Constant identity ──
        {
            let both_real: AB::Expr = local.is_real.into() * next.is_real.into();
            constrain_constant_identity(
                builder,
                local.table_id,
                next.table_id,
                local.col_id,
                next.col_id,
                both_real,
            );
        }

        // ── 4. Untouched binding: is_touched=0 ⟹ com_new = com_old ──
        constrain_untouched_binding(builder, local);

        // ── 5. Touched consistency: is_touched=0 ⟹ is_empty_new = is_empty_old ──
        constrain_touched_consistency(builder, local);

        // ── 6. Empty→non-empty: is_empty_old ∧ is_touched ⟹ ¬is_empty_new ──
        builder.assert_zero(
            local.is_real.into()
                * local.is_empty_old.into()
                * local.is_touched.into()
                * local.is_empty_new.into(),
        );

        // ── 7. Com_empty verification ──
        constrain_com_empty(builder, local);

        // ── 8. Leaf digest composition ──
        constrain_leaf_digest(builder, local, self.scheme_tag);

        // ── LogUp buses ──

        // C12 EmptyColRead receive
        builder.receive_empty_col_read(
            local.table_id.into(),
            local.col_id.into(),
            local.is_real.into() * local.is_empty_old.into() * local.empty_read_mult.into(),
        );

        builder.send_empty_old_column(
            local.table_id.into(),
            local.col_id.into(),
            local.is_real.into() * local.is_empty_old.into(),
        );

        // C6 CommitmentVerification receive (conditional on scheme)
        if self.receives_commitment {
            // Com_old
            {
                let not_empty_old: AB::Expr = AB::Expr::ONE - local.is_empty_old.into();
                builder.receive_commitment(
                    local.table_id.into(),
                    local.col_id.into(),
                    AB::Expr::ZERO, // comm_type = 0 (Com_old)
                    local.is_touched.into(),
                    &local.com_old,
                    local.is_real.into() * not_empty_old,
                );
            }

            // Com_new
            {
                let non_empty_new: AB::Expr = AB::Expr::ONE - local.is_empty_new.into();
                builder.receive_commitment(
                    local.table_id.into(),
                    local.col_id.into(),
                    AB::Expr::ONE, // comm_type = 1 (Com_new)
                    local.is_touched.into(),
                    &local.com_new,
                    local.is_real.into() * local.is_touched.into() * non_empty_new,
                );
            }
        }

        // C5 PoseidonPermutation send: Com_empty verification
        builder.send_poseidon_perm(
            &local.empty_perm_input,
            &local.empty_perm_output,
            local.is_real.into() * local.has_empty_check.into(),
        );

        // C5 PoseidonPermutation send: leaf digest old
        builder.send_poseidon_perm(
            &local.leaf_perm_input_old,
            &local.leaf_digest_old,
            local.is_real.into(),
        );

        // C5 PoseidonPermutation send: leaf digest new
        builder.send_poseidon_perm(
            &local.leaf_perm_input_new,
            &local.leaf_digest_new,
            local.is_real.into(),
        );

        // C15 SmtLeafDigest send
        builder.send_smt_leaf_digest(
            local.table_id.into(),
            local.col_id.into(),
            &local.leaf_digest_old,
            &local.leaf_digest_new,
            local.is_real.into(),
        );
    }
}

// ── Constraint helpers ──

/// 1. Boolean constraints for all flag columns.
fn constrain_booleans<AB: AirBuilder>(builder: &mut AB, local: &MetaShardCols<AB::Var>) {
    builder.assert_bool(local.is_empty_old);
    builder.assert_bool(local.is_empty_new);
    builder.assert_bool(local.is_touched);
}

/// 4. Untouched binding: `is_touched=0 ⟹ com_new = com_old`.
fn constrain_untouched_binding<AB: AirBuilder>(builder: &mut AB, local: &MetaShardCols<AB::Var>) {
    let not_touched: AB::Expr = AB::Expr::ONE - local.is_touched.into();
    for i in 0..DIGEST_WIDTH {
        builder
            .when(local.is_real)
            .when(not_touched.clone())
            .assert_eq(local.com_new[i], local.com_old[i]);
    }
}

/// 5. Touched consistency: `is_touched=0 ⟹ is_empty_new = is_empty_old`.
fn constrain_touched_consistency<AB: AirBuilder>(builder: &mut AB, local: &MetaShardCols<AB::Var>) {
    let not_touched: AB::Expr = AB::Expr::ONE - local.is_touched.into();
    let empty_diff: AB::Expr = local.is_empty_new.into() - local.is_empty_old.into();
    builder
        .when(local.is_real)
        .assert_zero(not_touched * empty_diff);
}

/// 7. Com_empty verification.
///
/// When `is_empty_old` or `is_empty_new`, verify the commitment equals
/// `Poseidon(0x00 || t || c || 0..)`.
fn constrain_com_empty<AB: AirBuilder>(builder: &mut AB, local: &MetaShardCols<AB::Var>) {
    let is_real: AB::Expr = local.is_real.into();

    // has_empty_check = is_empty_old OR is_empty_new
    let expected_has_empty: AB::Expr = local.is_empty_old.into() + local.is_empty_new.into()
        - local.is_empty_old.into() * local.is_empty_new.into();
    builder.assert_zero(is_real.clone() * (local.has_empty_check.into() - expected_has_empty));
    builder.assert_bool(local.has_empty_check);

    let gate: AB::Expr = is_real.clone() * local.has_empty_check.into();

    // Input composition: perm_input[0] = 0x00 (SSMC domain tag)
    builder.assert_zero(gate.clone() * local.empty_perm_input[0].into());

    // perm_input[1] = table_id
    builder.assert_zero(gate.clone() * (local.empty_perm_input[1].into() - local.table_id.into()));

    // perm_input[2] = col_id
    builder.assert_zero(gate.clone() * (local.empty_perm_input[2].into() - local.col_id.into()));

    // perm_input[3..16] = 0 (zero padding)
    for i in 3..16 {
        builder.assert_zero(gate.clone() * local.empty_perm_input[i].into());
    }

    // Com_old = perm_output when is_empty_old
    let old_gate: AB::Expr = is_real.clone() * local.is_empty_old.into();
    for i in 0..DIGEST_WIDTH {
        builder.assert_zero(
            old_gate.clone() * (local.com_old[i].into() - local.empty_perm_output[i].into()),
        );
    }

    // Com_new = perm_output when is_empty_new
    let new_gate: AB::Expr = is_real * local.is_empty_new.into();
    for i in 0..DIGEST_WIDTH {
        builder.assert_zero(
            new_gate.clone() * (local.com_new[i].into() - local.empty_perm_output[i].into()),
        );
    }
}

/// 8. Leaf digest perm input composition.
///
/// `leaf_perm_input = [0x10, table_id, col_id, scheme_tag, 0,0,0,0, com[8]]`
///
/// `scheme_tag` is a constructor constant, not a trace column.
fn constrain_leaf_digest<AB: AirBuilder>(
    builder: &mut AB,
    local: &MetaShardCols<AB::Var>,
    scheme_tag: u16,
) {
    let is_real: AB::Expr = local.is_real.into();
    let tag_expr = AB::Expr::from_u16(scheme_tag);

    // Old leaf
    builder.assert_zero(
        is_real.clone() * (local.leaf_perm_input_old[0].into() - AB::Expr::from_u64(0x10)),
    );
    builder.assert_zero(
        is_real.clone() * (local.leaf_perm_input_old[1].into() - local.table_id.into()),
    );
    builder
        .assert_zero(is_real.clone() * (local.leaf_perm_input_old[2].into() - local.col_id.into()));
    builder.assert_zero(is_real.clone() * (local.leaf_perm_input_old[3].into() - tag_expr.clone()));
    for i in 4..8 {
        builder.assert_zero(is_real.clone() * local.leaf_perm_input_old[i].into());
    }
    for i in 0..DIGEST_WIDTH {
        builder.assert_zero(
            is_real.clone() * (local.leaf_perm_input_old[8 + i].into() - local.com_old[i].into()),
        );
    }

    // New leaf
    builder.assert_zero(
        is_real.clone() * (local.leaf_perm_input_new[0].into() - AB::Expr::from_u64(0x10)),
    );
    builder.assert_zero(
        is_real.clone() * (local.leaf_perm_input_new[1].into() - local.table_id.into()),
    );
    builder
        .assert_zero(is_real.clone() * (local.leaf_perm_input_new[2].into() - local.col_id.into()));
    builder.assert_zero(is_real.clone() * (local.leaf_perm_input_new[3].into() - tag_expr));
    for i in 4..8 {
        builder.assert_zero(is_real.clone() * local.leaf_perm_input_new[i].into());
    }
    for i in 0..DIGEST_WIDTH {
        builder.assert_zero(
            is_real.clone() * (local.leaf_perm_input_new[8 + i].into() - local.com_new[i].into()),
        );
    }
}
